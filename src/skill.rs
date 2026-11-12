//! The generated agent skill: `scree skill` and `scree skill --check PATH`.
//!
//! The body is a template filled from hand-written prose files under `skill/`
//! plus a reference built from the clap tree, so the reference cannot drift
//! from the CLI. See `docs/skill.md`.

use std::fmt::Write as _;
use std::path::Path;

use clap::{Arg, Command, CommandFactory};
use sha2::{Digest, Sha256};

use crate::cli::Cli;
use crate::error::EXIT_CODES;

const TEMPLATE: &str = include_str!("../skill/template.md");

/// Prose slots, hand-written. Each `{{name}}` in the template names one.
const SLOTS: [(&str, &str); 5] = [
    ("purpose", include_str!("../skill/purpose.md")),
    ("workflow", include_str!("../skill/workflow.md")),
    ("human-tickets", include_str!("../skill/human-tickets.md")),
    ("scoping", include_str!("../skill/scoping.md")),
    ("output", include_str!("../skill/output.md")),
];

/// Human and maintenance commands the skill does not document.
const OMITTED: [&str; 5] = ["skill", "project", "sweep", "archive", "unarchive"];

const DESCRIPTION: &str =
    "Track and work tickets with scree. Use when picking up, creating or finishing work items.";

/// The prose slots, for tests that check the commands they mention.
#[cfg(test)]
pub(crate) fn prose() -> impl Iterator<Item = &'static str> {
    SLOTS.iter().map(|(_, v)| *v)
}

/// The complete SKILL.md: frontmatter, then the body the hash covers.
/// Always ends in exactly one newline.
pub fn generate() -> String {
    let body = body();
    let hash = hex(&Sha256::digest(body.as_bytes()));
    format!(
        "---\nname: scree\ndescription: {DESCRIPTION}\nscree_version: {}\nhash: {hash}\n---\n{body}",
        env!("CARGO_PKG_VERSION"),
    )
}

/// True only when the file at `path` is byte-identical to `generated`.
/// A missing or unreadable file is stale.
pub fn check(path: &Path, generated: &str) -> bool {
    std::fs::read(path).is_ok_and(|b| b == generated.as_bytes())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn body() -> String {
    let reference = reference();
    let mut out = String::new();
    // One pass: inserted text is never rescanned for placeholders.
    let mut rest = TEMPLATE;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .expect("unterminated slot in skill template");
        let name = &after[..end];
        let value = if name == "reference" {
            reference.as_str()
        } else {
            SLOTS
                .iter()
                .find(|(n, _)| *n == name)
                .unwrap_or_else(|| panic!("unknown slot {{{{{name}}}}} in skill template"))
                .1
        };
        out.push_str(value.trim_end());
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out.trim_end().to_string() + "\n"
}

fn reference() -> String {
    let mut root = Cli::command();
    root.build();
    let mut out = String::from("Global flags, accepted by every command:\n\n");
    for arg in root.get_arguments().filter(|a| a.is_global_set()) {
        let _ = writeln!(out, "- {}", describe(arg));
    }
    out.push('\n');
    for sub in root.get_subcommands() {
        command_section(sub, &mut out);
    }
    out.push_str("Exit codes:\n\n");
    for (code, meaning) in EXIT_CODES {
        let _ = writeln!(out, "- {code}: {meaning}");
    }
    out
}

fn command_section(cmd: &Command, out: &mut String) {
    if cmd.get_name() == "help" {
        return;
    }
    // Only top-level names are filtered; nested ones (criteria add) are never omitted.
    if cmd
        .get_bin_name()
        .is_some_and(|b| b.split(' ').count() == 2)
        && OMITTED.contains(&cmd.get_name())
    {
        return;
    }
    let mut cmd = cmd.clone();
    let usage = cmd.render_usage().to_string();
    let usage = usage.trim().trim_start_matches("Usage:").trim().to_string();
    let _ = writeln!(out, "### `{usage}`\n");
    if let Some(about) = cmd.get_about() {
        let _ = writeln!(out, "{about}\n");
    }
    let args: Vec<&Arg> = cmd
        .get_arguments()
        .filter(|a| !a.is_global_set() && a.get_id() != "help" && a.get_id() != "version")
        .collect();
    for arg in &args {
        let _ = writeln!(out, "- {}", describe(arg));
    }
    if !args.is_empty() {
        out.push('\n');
    }
    for sub in cmd.get_subcommands() {
        command_section(sub, out);
    }
}

/// One argument as `--flag <VALUE>: help (values: a, b; default x; env X)`.
fn describe(arg: &Arg) -> String {
    let mut s = match arg.get_long() {
        Some(long) => {
            let mut s = format!("`--{long}");
            if arg.get_action().takes_values() {
                let name = arg
                    .get_value_names()
                    .and_then(|n| n.first().map(|n| n.to_string()))
                    .unwrap_or_else(|| arg.get_id().as_str().to_uppercase());
                let _ = write!(s, " <{name}>");
            }
            s.push('`');
            s
        }
        None => format!("`<{}>`", arg.get_id().as_str().to_uppercase()),
    };
    if let Some(help) = arg.get_help() {
        let _ = write!(s, ": {help}");
    }
    let mut notes = Vec::new();
    let values: Vec<String> = arg
        .get_possible_values()
        .iter()
        .filter(|v| !v.is_hide_set())
        .map(|v| v.get_name().to_string())
        .collect();
    if !values.is_empty() {
        notes.push(format!("values: {}", values.join(", ")));
    }
    let defaults: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|d| d.to_string_lossy().into_owned())
        .collect();
    if arg.get_action().takes_values() && !defaults.is_empty() {
        notes.push(format!("default {}", defaults.join(", ")));
    }
    if arg.is_required_set() {
        notes.push("required".into());
    }
    if let Some(env) = arg.get_env() {
        notes.push(format!("env {}", env.to_string_lossy()));
    }
    if !notes.is_empty() {
        let _ = write!(s, " ({})", notes.join("; "));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_and_hash() {
        let text = generate();
        let rest = text.strip_prefix("---\nname: scree\n").unwrap();
        let (front, body) = rest.split_once("\n---\n").unwrap();
        assert!(front.contains(&format!("scree_version: {}", env!("CARGO_PKG_VERSION"))));
        let hash = front
            .lines()
            .find_map(|l| l.strip_prefix("hash: "))
            .unwrap();
        assert_eq!(hash, hex(&Sha256::digest(body.as_bytes())));
        assert!(text.ends_with('\n') && !text.ends_with("\n\n"));
    }

    #[test]
    fn reference_omits_maintenance_commands() {
        let r = reference();
        for gone in OMITTED {
            assert!(
                !r.contains(&format!("### `Usage: scree {gone}"))
                    && !r.contains(&format!("`scree {gone}")),
                "{gone} must be omitted"
            );
        }
        for kept in [
            "scree dep add",
            "scree criteria add",
            "scree check",
            "scree comment add",
            "scree move",
            "scree reorder",
        ] {
            assert!(r.contains(&format!("`{kept}")), "{kept} must be documented");
        }
        assert!(r.contains("- 5: "));
    }

    #[test]
    fn slots_are_all_used_and_known() {
        for (name, _) in SLOTS {
            assert!(
                TEMPLATE.contains(&format!("{{{{{name}}}}}")),
                "{name} unused"
            );
        }
        assert!(!body().contains("{{"));
    }

    #[test]
    fn check_detects_any_difference() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("SKILL.md");
        let g = generate();
        assert!(!check(&p, &g));
        std::fs::write(&p, &g).unwrap();
        assert!(check(&p, &g));
        std::fs::write(&p, format!("{g} ")).unwrap();
        assert!(!check(&p, &g));
    }

    /// Every `scree ...` line in the prose must parse against the clap tree.
    #[test]
    fn prose_commands_parse() {
        let mut seen = 0;
        for text in prose() {
            for line in text.lines() {
                let Some(cmd) = line.trim().strip_prefix("scree ") else {
                    continue;
                };
                let args = shlex_split(cmd);
                let mut argv = vec!["scree".to_string()];
                argv.extend(args);
                if let Err(e) = <Cli as clap::Parser>::try_parse_from(&argv) {
                    panic!("prose line does not parse: `{line}`\n{e}");
                }
                seen += 1;
            }
        }
        assert!(seen >= 10, "expected many prose commands, found {seen}");
    }

    /// Split on spaces, keeping double-quoted runs together.
    fn shlex_split(s: &str) -> Vec<String> {
        let (mut out, mut cur, mut quoted, mut any) = (Vec::new(), String::new(), false, false);
        for c in s.chars() {
            match c {
                '"' => {
                    quoted = !quoted;
                    any = true;
                }
                ' ' if !quoted => {
                    if any {
                        out.push(std::mem::take(&mut cur));
                        any = false;
                    }
                }
                c => {
                    cur.push(c);
                    any = true;
                }
            }
        }
        if any {
            out.push(cur);
        }
        out
    }

    #[test]
    fn snapshot() {
        let mut text = generate();
        // Version and hash change on bump; their shape is asserted elsewhere.
        let (front, body) = text.split_once("\n---\n").unwrap();
        let front: Vec<_> = front
            .lines()
            .map(|l| {
                if l.starts_with("scree_version:") || l.starts_with("hash:") {
                    format!("{}: <redacted>", l.split(':').next().unwrap())
                } else {
                    l.to_string()
                }
            })
            .collect();
        text = format!("{}\n---\n{body}", front.join("\n"));
        insta::assert_snapshot!(text);
    }
}

use clap::Parser;
use scree::Outcome;
use scree::cli::Cli;
use scree::identity::Env;

/// An empty `SCREE_*` variable counts as unset. clap reads the environment
/// itself and rejects an empty value for some arguments, so drop the empty
/// ones before it parses; this covers every current and future variable.
fn drop_empty_scree_vars() {
    let empty: Vec<_> = std::env::vars_os()
        .filter(|(k, v)| v.is_empty() && k.to_string_lossy().starts_with("SCREE_"))
        .map(|(k, _)| k)
        .collect();
    for k in empty {
        // SAFETY: called first in main, before any other thread exists.
        unsafe { std::env::remove_var(k) };
    }
}

fn main() {
    drop_empty_scree_vars();
    let cli = Cli::parse();
    let cwd = match std::env::current_dir() {
        Ok(c) => c,
        Err(e) => {
            let e = scree::error::Error::Other(format!("cannot read the current directory: {e}"));
            eprintln!("{}", e.to_json());
            std::process::exit(e.exit_code());
        }
    };
    match scree::run(cli, &Env::from_process(), &cwd) {
        Ok(Outcome::Print(out)) => println!("{out}"),
        Ok(Outcome::Help) => {
            print!("{}", scree::help_text());
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("{}", e.to_json());
            std::process::exit(e.exit_code());
        }
    }
}

# Scree demo

A reproducible, scripted recording of scree for a roughly 2 minute demo. The video is silent; narrate over it with `SCRIPT.md`.

Everything runs against a throwaway database (`SCREE_DB=demo/.work/scree.db`) and a throwaway git repo (`demo/.work/shopfront`), so your real tickets are never touched. `demo/.work` and `demo/out` are gitignored.

## Use

```sh
nix develop .#demo        # vhs, ffmpeg, ttyd, jq, git, and scree built from this checkout
demo/record.sh            # all scenes into demo/out/*.mp4
demo/record.sh 03-tui     # one scene
demo/seed.sh && source demo/env.sh   # poke at the sample project by hand
```

## Scene 4 (Claude Code)

`04-claude` runs the real `claude` CLI, so it needs two things the other scenes do not:

- `claude` signed in on this machine. It runs as Haiku (`claude-haiku-5-5`), so it is quick and cheap.
- The sample project trusted once. Claude asks about an unfamiliar folder on first run, and trust is remembered per path in `~/.claude.json`. Do it once by hand: `demo/seed.sh claude && source demo/env.sh && claude`, accept the prompt, then `/exit`.

It is sandboxed by the `claude` function in `env.sh`: only the Bash and Skill tools, only `scree *` commands allowed, everything else denied without a prompt, no user settings, no MCP servers, and a throwaway repo as the working directory. Claude's response time varies, so the tape waits on screen text (`Wait+Screen`) instead of fixed sleeps.

## Layout

- `seed.sh`: builds the sample project and backlog from nothing. `seed.sh tui` adds what scenes 1 and 2 leave behind; `seed.sh claude` installs the scree skill in the sample project.
- `env.sh`: points scree at the demo database and clears inherited agent/role variables.
- `scenes/*.tape`: one VHS tape per scene. `common.tape` holds the shared theme and size.
- `record.sh`: seeds the right state for each scene, then runs `vhs`.
- `SCRIPT.md`: the narration, with timings.

## Publishing the clips

`demo/out` is gitignored. The clips linked from the top-level README are committed copies in `docs/videos/`; after re-recording, refresh them with `cp demo/out/0*.mp4 docs/videos/`.

## Status

All four scenes have been recorded. `03-tui.tape` and `04-claude.tape` depend on timing (the live-update delay, Claude's response time), so retune their sleeps if a run looks off. The clips are silent; narrate over them with `SCRIPT.md`.

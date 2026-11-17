#!/usr/bin/env bash
# Record the demo scenes into demo/out/*.mp4. Run inside `nix develop .#demo`.
#
#   record.sh            all scenes
#   record.sh 03-tui     one scene (tape name without .tape)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
mkdir -p out

for tool in vhs ffmpeg ttyd jq scree; do
  command -v "$tool" >/dev/null || { echo "missing $tool: run inside 'nix develop .#demo'" >&2; exit 1; }
done

scenes=("$@")
[ ${#scenes[@]} -gt 0 ] || scenes=(01-backlog 02-agent 03-tui 04-claude)

for s in "${scenes[@]}"; do
  # Scene 3 opens on the state that scenes 1 and 2 leave behind; the others start from the base backlog.
  case "$s" in
    03-tui) ./seed.sh tui ;;
    04-claude) ./seed.sh claude ;;
    *) ./seed.sh ;;
  esac
  # Scene 2 expects the wishlist ticket that scene 1 adds on screen.
  if [ "$s" = "02-agent" ]; then
    # Subshell: env.sh cds into the sample project, which must not leak into this script.
    (
      source ./env.sh
      SCREE_ROLE=human SCREE_FORMAT=json scree add "Add a wishlist page" --status ready --blocked-by SHOP-3 >/dev/null
    )
  fi
  echo "recording $s"
  vhs "scenes/$s.tape"
done

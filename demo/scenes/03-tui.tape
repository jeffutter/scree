# Scene 3: the terminal UI (about 40s). Seed: tui.
#
# UNTESTED DRAFT. The key sequence assumes the board opens focused on the first
# populated column; tune the `l`/`j` counts and sleeps while watching it run.
Source scenes/common.tape
Output out/03-tui.mp4

Hide
Type "source ./env.sh && clear"
Enter
# Live-update beat: a second agent claims SHOP-3 from outside the TUI. The delay
# is measured from here, so tune it to land mid-tour (the TUI polls every 500ms).
Type "(sleep 26; SCREE_ROLE=agent SCREE_AUTHOR=claude SCREE_SESSION=sess-c scree start SHOP-3 >/dev/null 2>&1) &"
Enter
Type "clear"
Enter
Show

Type "scree tui"
Enter
Sleep 4s

# Move to the Active column.
Type "ll"
Sleep 1s

# Detail view of the active ticket: criteria with evidence.
Enter
Sleep 4s
Escape
Sleep 1s

# Search.
Type "/"
Sleep 500ms
Type "search"
Enter
Sleep 2s
Escape
Sleep 1s

# Promote a draft to ready with `r`.
Type "hh"
Sleep 800ms
Type "r"
Sleep 2s

# List view.
Tab
Sleep 3s
Tab
Sleep 1s

# Help popup.
Type "?"
Sleep 3s
Escape

# Wait here: the second agent's claim of SHOP-3 appears without a refresh.
Sleep 8s

Type "q"
Sleep 1s

# Scene 4: Claude Code using the scree skill (about 40s). Seed: claude.
#
# UNTESTED DRAFT. Needs `claude` already signed in and the sample project already
# trusted (see demo/README.md). Claude's response time varies, so this waits on
# screen text instead of fixed sleeps; tune the patterns to what you actually see.
Source scenes/common.tape
Set TypingSpeed 50ms
Output out/04-claude.mp4

Hide
Type "source ./env.sh && clear"
Enter
Show

Sleep 1s
Type "claude"
Enter

# Wait for the input box (the footer shows the permission mode once it is ready).
Wait+Screen@60s /don't ask on/
Sleep 1s

Type "What is available to work on? Then file a ticket for letting shoppers leave product reviews, with two acceptance criteria."
Sleep 500ms
Enter

# VHS's Wait only sees the first screenful of the buffer, so text that has scrolled
# off (the summary, "done") can never match. Wait for the `scree add` tool call, which
# appears early, then give Claude time to write its summary.
Wait+Screen@120s /scree add/
Sleep 15s

# Leave Claude, then show what it filed.
Ctrl+C
Sleep 500ms
Ctrl+C
Sleep 1s
Type "scree show SHOP-10"
Enter
Sleep 6s

# Scene 1: the human's view of a backlog (about 40s). Seed: base.
# Comments are typed a little faster than commands so they read as captions.
Source scenes/common.tape
Output out/01-backlog.mp4

Hide
Type "source ./env.sh && clear"
Enter
Show

Sleep 1s
Type@50ms "# a small web shop's backlog, in one sqlite file"
Enter
Type "scree list"
Enter
Sleep 5s

Type@50ms "# some work is for humans only; agents are never handed it"
Enter
Type "scree list --for human"
Enter
Sleep 4s

Type@50ms "# add a ticket that waits on one that is not finished"
Enter
Type 'scree add "Add a wishlist page" --status ready --blocked-by SHOP-3'
Enter
Sleep 4s

Type "clear"
Enter
Type@50ms "# blocked tickets say what they are waiting on"
Enter
Type "scree list"
Enter
Sleep 6s

Type@50ms "# every ticket carries its description and acceptance criteria"
Enter
Type "scree show SHOP-2"
Enter
Sleep 6s

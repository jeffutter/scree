# Scene 2: an agent claims work and finishes it (about 30s). Seed: base plus the
# wishlist ticket from scene 1, so this scene needs `seed.sh base` and one extra add.
Source scenes/common.tape
# Comments are read, not watched: type faster than the other scenes.
Set TypingSpeed 50ms
Output out/02-agent.mp4

Hide
Type "source ./env.sh && export SCREE_ROLE=agent SCREE_FORMAT=text SCREE_AUTHOR=claude SCREE_SESSION=sess-b && clear"
Enter
Show

Sleep 1s
Type "# an agent asks what it can work on"
Enter
Type "scree next"
Enter
Sleep 4s

Type "# claim it, atomically"
Enter
Type "scree start --next"
Enter
Sleep 4s

Type 'scree check SHOP-2#1 --evidence "ranked results returned"'
Enter
Sleep 2s
Type 'scree check SHOP-2#2 --evidence "400 on empty query"'
Enter
Sleep 2s
Type 'scree check SHOP-2#3 --evidence "reindex test passes"'
Enter
Sleep 2s
Type 'scree comment add SHOP-2 "Used FTS5, reindexed on product write."'
Enter
Sleep 3s

Type "scree done SHOP-2"
Enter
Sleep 3s

Type "clear"
Enter
Type "# finishing it unblocked the next ticket"
Enter
Type "scree next"
Enter
Sleep 5s

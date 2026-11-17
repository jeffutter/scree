# Sourced by seed.sh and by every tape. Points scree at a throwaway database and
# drops the shell into the sample project, so nothing touches your real data.
DEMO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export DEMO_DIR
export SCREE_DB="$DEMO_DIR/.work/scree.db"
export SCREE_NO_SWEEP=1
# Clear anything inherited from an agent session so the human/agent roles are explicit.
unset CLAUDECODE CLAUDE_CODE_CHILD_SESSION CLAUDE_CODE_SESSION_ID AI_AGENT SCREE_ROLE SCREE_AUTHOR SCREE_SESSION SCREE_PROJECT SCREE_FORMAT
export GIT_CONFIG_GLOBAL=/dev/null
export PS1=$'\n$ '
export NO_COLOR=
cd "$DEMO_DIR/.work/shopfront" 2>/dev/null || true

# `claude` as typed on screen, sandboxed for scene 4: Haiku, only the Bash and Skill tools,
# only `scree` commands allowed, anything else denied without prompting, no user settings,
# and no MCP servers. The sample repo is a throwaway, so nothing here
# can reach real work.
claude() {
  command claude --model claude-haiku-5-5 --tools "Bash,Skill" \
    --allowedTools "Bash(scree *)" "Bash(echo *)" --permission-mode dontAsk \
    --append-system-prompt "Run exactly one scree command per Bash call. Never chain commands with ;, && or pipes." \
    --setting-sources project --strict-mcp-config "$@"
}

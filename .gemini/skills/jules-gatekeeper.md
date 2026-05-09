# Skill: Jules Gatekeeper

## Purpose
Proactively monitors, validates, and integrates Pull Requests created by Jules agents into the MemFuse codebase.

## Triggering Logic
- Run at the start of any Gemini-CLI session.
- Run after any agentic work package (Work Package transition).
- Can be triggered manually via command: `integrate jules`.

## Protocol
1. **Fetch Status**: Use `gh pr list --label jules` to identify candidate PRs.
2. **Validate Quality Gates**:
    - Check if the PR is `MERGEABLE`.
    - Check if CI status is `SUCCESS`.
    - Verify against `AGENTS.md` and `docs/specs/` that the PR references a valid Work Package.
3. **Execution**:
    - Run `.agent/scripts/jules-integrate.sh`.
    - If successful, announce the integration and the new system state.
4. **Error Handling**:
    - If a PR has conflicts, notify the user as a "Gate Blocking" event.
    - If CI fails, document the failure in the PR and skip.

## Commands
- `just integrate`: Meta-command to run the full integration suite.

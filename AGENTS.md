# Agent Instructions

See [DEVELOPMENT.md](DEVELOPMENT.md) for development setup and test structure.

## Quality Gates

Run these before pushing; CI fails on fmt and clippy:

```bash
cargo fmt --all
cargo clippy --all-targets
cargo test
./script/install && cctr test/   # corpus tests use the installed binary
```

Note that `cctr test/` invokes `cctr` recursively, so the nested runs use the
*installed* binary rather than the debug build. Install before running them.

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **Note remaining work** - Record anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
4. **Clean up** - Clear stashes, prune remote branches
5. **Verify** - All changes committed AND pushed
6. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds

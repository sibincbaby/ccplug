# Changelog

## 0.1.0

Initial release.

- `ccplug list` — global plugin inventory with skills, `provides`, and effective enabled state.
- `ccplug status` — effective per-project plugins (with winning source scope) and skill overrides.
- `ccplug enable` / `ccplug disable` — bulk toggles for `plugin`, `plugin@market`, `plugin:skill`, `plugin:*`.
  - Targets via positional args, `--from FILE`, or `--stdin` (JSON array).
  - `--scope project|local|user`, `--json`, `--dry-run`.
  - Safe read-modify-write of the settings cascade (preserves all unrelated keys + order; `.bak` backup).
  - Partial-batch semantics: plugin-owned skills return `unsupported-v1` without failing the batch; exit non-zero only if every target fails.
  - Warns when disabling a plugin also removes its MCP server / LSP / agents.
- Ships `SKILL.md` so Claude Code can drive it.

Verified empirically (2026-06-19): a project-scope `enabledPlugins: {"x@mkt": false}` disables a user-enabled plugin via Claude Code's own resolver; ccplug's output format is byte-identical to native `claude plugin disable`.

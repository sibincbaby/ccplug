# Changelog

## 0.2.1

- `ccplug status` now shows a per-plugin **COST (tok)** column (it previously only had the `enabledEst` total); `status --json` adds `estTokens` per effective plugin.
- Cost column headers in both `list` and `status` are labelled `COST (tok)` so the numbers read as tokens.

## 0.2.0

Cost-aware listing.

- `ccplug list` now shows an estimated **always-on token cost** per plugin (COST column), plus a footer total of enabled vs all cost — so you can see what each plugin costs before deciding to enable/disable it.
- `ccplug list --sort cost` ranks the most expensive plugins first.
- `ccplug status` reports `enabledEst` — the project's estimated always-on cost after the cascade.
- `--json`: `list` gains `estTokens` per plugin and a `summary{totalEst,enabledEst}`; `status` gains `enabledEst`.
- Cost is a local estimate from skill descriptions (`chars/4`, per the spec §2 cost model); `claude plugin details <name>` gives exact numbers. No new dependencies, no subprocess — stays fast.
- Design note: project-type **presets** and **per-skill** (plugin-owned) toggling were evaluated and deliberately rejected — presets ossify a per-project-varying mix, and per-skill control requires fragile extraction that severs a skill from its plugin's hooks/MCP/commands. The plugin remains the unit of on/off.

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

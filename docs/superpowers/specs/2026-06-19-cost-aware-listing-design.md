# ccplug — cost-aware listing (v0.2.0) design

**Date:** 2026-06-19
**Status:** approved design, ready for implementation plan
**Author:** brainstormed with Claude
**Builds on:** `docs/spec.md` (v1)

Show an estimated **always-on token cost** per plugin in `list`/`status`, so per-project whole-plugin decisions are informed by what each plugin actually costs the session.

---

## 1. Why

v1 lets you enable/disable whole plugins per project, but gives no signal about *which* plugins are worth disabling. The motivation for the whole tool is the **context tax**: every enabled plugin loads its skill **descriptions** at session start. Cost-aware listing surfaces that tax so the headline decision ("this plugin costs ~X and I don't use it here → disable it") is data-driven.

Two v2 ideas were explored and **rejected** during brainstorming:
- **Presets** — would ossify a plugin mix the user says varies per project (YAGNI).
- **Per-skill control of plugin-owned skills** — the only mechanism is *extraction* (disable plugin + copy skills into `.claude/skills/`), which severs the skill from the plugin's hooks/MCP/commands and goes stale on every auto-update, all to shave a skill description (~150–220 tok). Bad trade; the plugin stays the correct unit of on/off.

So v2 is this one small, sound feature.

## 2. Cost model (what we estimate, and why locally)

Per v1 spec §2: **only skill descriptions load at session start; the body is lazy.** So a plugin's always-on tax ≈ the sum of its skills' description sizes.

- **Estimator:** `est_tokens(text) = text.chars().count() / 4` (the standard rough chars→tokens heuristic). Applied to each skill's `name + "\n" + description`.
- **Plugin cost:** `estTokens(plugin) = Σ est_tokens(name + description)` over its skills. A plugin with no skills costs 0.
- **Accuracy:** this is an **estimate**, intentionally. It lands in `claude plugin details`' ~150–220 tok/skill ballpark and is purely for *relative ranking* ("which plugins are expensive"), not billing.

**Why not call `claude plugin details`:** it has no `--json` and costs ~1.3s/call (~34s across 26 plugins) — unusable inline for a tool whose value is being fast. It remains the manual escape-hatch for exact per-plugin numbers; ccplug's output points users to it.

**Scope of the estimate (deliberately skills-only):** agent/command descriptions and plugin `CLAUDE.md` also add always-on context, but the spec's cost model is skill descriptions, and keeping the estimate to skills keeps the inventory parser unchanged. Documented as a known under-count; extendable later.

## 3. Surface changes (no new commands)

### `ccplug list`
- New **COST** column: per-plugin `estTokens` (e.g. `~780`).
- Footer gains two totals:
  - **all:** Σ estTokens over every plugin.
  - **enabled:** Σ estTokens over plugins effectively enabled in the cwd project — the real per-session tax here.
- Existing columns unchanged.

### `ccplug list --sort cost`
- New optional sort key ranking plugins by `estTokens` descending (expensive first), so unused-and-expensive plugins surface. Default sort stays by name (current behavior). `--sort name` available for explicitness.

### `ccplug status`
- Add a one-line headline: estimated enabled always-on cost for the cwd project (`enabledEst`), since `status` is the "what's active here" view.

### `--json` (list)
- Each plugin object gains `"estTokens": <int>`.
- A top-level `"summary": { "totalEst": <int>, "enabledEst": <int> }`.

### `--json` (status)
- Add `"enabledEst": <int>` alongside `effectivePlugins`.

### Honesty note
- Human output prints a short footer once: `cost = estimated always-on tokens from skill descriptions; exact: claude plugin details <name>`.

## 4. Output schema deltas (additive — v1 keys unchanged)

```jsonc
// ccplug list --json
{ "plugins": [ { /* …v1 fields… */ "estTokens": 780 } ],
  "summary": { "totalEst": 5400, "enabledEst": 2100 } }

// ccplug status --json
{ /* …v1 fields… */ "enabledEst": 2100 }
```

## 5. Implementation shape

- **`src/inventory.rs`:** add `est_tokens(&str) -> u32` (free fn) and a `Plugin::est_tokens()` method (or a field computed at load). Pure, unit-tested.
- **`src/cli.rs`:** extend the `list` sort option to accept `cost` (add a `--sort {name,cost}` `ValueEnum`, default `name`).
- **`src/output.rs`:** add the COST column + footer totals to `list_table`; inject `estTokens` + `summary` into `list_json`; add `enabledEst` to status renderers.
- **`src/main.rs`:** compute `enabledEst`/`totalEst` from the inventory + effective-enabled set; thread the sort key into `cmd_list`.

No new dependencies, no subprocess, no settings changes. Stays fast (pure local computation over already-parsed inventory).

## 6. Testing

- `est_tokens`: empty → 0; longer text → strictly greater; ballpark (a ~600-char description estimates 100–200 tok).
- Plugin cost = sum of skill estimates (fixture with 2 skills).
- `summary.enabledEst` only sums enabled plugins (integration: two plugins, one disabled in project scope → enabledEst excludes it).
- `--sort cost` orders the higher-cost plugin first.

## 7. Out of scope (unchanged from v1 §9, minus the rejected items)

- Presets / auto-suggest — rejected.
- Per-skill plugin-owned skill control / extraction / `sync` — rejected (plugin is the unit).
- `ccplug pick` TUI — still possible later, not here.
- Exact (non-estimated) token accounting — use `claude plugin details`.

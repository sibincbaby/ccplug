# Cost-aware listing (v0.2.0) Implementation Plan

> **For agentic workers:** Use superpowers-extended-cc:subagent-driven-development or executing-plans to implement task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Show an estimated always-on token cost per plugin in `ccplug list`/`status` so per-project whole-plugin decisions are informed by what each plugin costs the session.

**Architecture:** Pure local estimate — `est_tokens(text)=chars/4` over each skill's `name+description`, summed per plugin. No subprocess, no new deps, stays fast. Additive to v1 output schemas (no existing keys change). Builds on `docs/superpowers/specs/2026-06-19-cost-aware-listing-design.md`.

**Tech Stack:** existing — Rust 2021, clap, serde_json, comfy-table.

**User decisions (already made):**
- Self-estimate locally; do NOT shell out to `claude plugin details` (no `--json`, ~1.3s/call → ~34s for 26 plugins).
- Estimate from **skill descriptions only** (spec §2 cost model); agent/command/CLAUDE.md context is a known under-count, out of scope.
- Presets and per-skill extraction were considered and **rejected**; the plugin stays the unit of control.

---

## File Structure (all existing files)

| File | Change |
|---|---|
| `src/inventory.rs` | add `est_tokens(&str)->u32` + per-plugin cost; serialize `estTokens` |
| `src/cli.rs` | add `--sort {name,cost}` to `list` (default name) |
| `src/output.rs` | COST column + footer totals in `list_table`; `summary` in `list_json`; `enabledEst` in status |
| `src/main.rs` | compute `totalEst`/`enabledEst`, thread sort key into `cmd_list` |
| `Cargo.toml`, `CHANGELOG.md` | bump to 0.2.0, changelog entry |

---

## Task 1: Token estimator + per-plugin cost

**Goal:** A pure, tested estimate of a plugin's always-on token cost, exposed in the JSON.

**Files:**
- Modify: `src/inventory.rs` (+ `#[cfg(test)]`)

**Acceptance Criteria:**
- [ ] `est_tokens("")==0`; longer text → strictly greater; a ~600-char string estimates 100–200 tok
- [ ] `Plugin` serializes an `estTokens` field = Σ `est_tokens(name+"\n"+description)` over its skills
- [ ] A plugin with no skills has `estTokens==0`

**Verify:** `cargo test inventory` → estimator + cost tests pass

**Design:**
```rust
/// Rough chars→tokens heuristic (~4 chars/token); for relative ranking, not billing.
pub fn est_tokens(text: &str) -> u32 {
    (text.chars().count() / 4) as u32
}

impl Plugin {
    pub fn est_tokens(&self) -> u32 {
        self.skills
            .iter()
            .map(|s| est_tokens(&format!("{}\n{}", s.name, s.description)))
            .sum()
    }
}
```
Serialize it. `Plugin` currently `#[derive(Serialize)]` with named fields — add a computed value at serialization via a helper in `output`/`main` rather than a struct field (keeps `Plugin` a plain data holder). Decision: compute in the JSON-building code (Task 2/3) by calling `p.est_tokens()`; the **method** lives here in Task 1, tested here.

**Steps:**
- [ ] Add `est_tokens` free fn + `Plugin::est_tokens` method.
- [ ] Tests: empty→0, monotonic, ballpark, no-skills→0, two-skill sum.

---

## Task 2: Cost in `ccplug list` (column, totals, sort, JSON summary)

**Goal:** Surface per-plugin cost and the two totals in `list`, sortable by cost, in both table and JSON.

**Files:**
- Modify: `src/cli.rs`, `src/output.rs`, `src/main.rs`

**Acceptance Criteria:**
- [ ] `list` table has a `COST` column (`~<n>`) per plugin
- [ ] footer shows `all: ~<totalEst> tok` and `enabled: ~<enabledEst> tok`
- [ ] one honesty line printed: `cost = est. always-on tokens from skill descriptions; exact: claude plugin details <name>`
- [ ] `list --sort cost` orders highest-cost first; default (or `--sort name`) keeps name order
- [ ] `list --json` adds `estTokens` per plugin and top-level `summary:{totalEst,enabledEst}`

**Verify:** `cargo test` (unit `output` + integration in `tests/cli.rs`): JSON has `summary.enabledEst` excluding a project-disabled plugin; `--sort cost` test orders the costlier fixture plugin first.

**Design:**
- `cli.rs`: add an enum + flag scoped to list. Since `List(CommonFlags)` shares flags with status, add an optional sort that status ignores:
  ```rust
  #[derive(ValueEnum, Clone, Copy, PartialEq, Default)]
  pub enum SortKey { #[default] Name, Cost }
  // in CommonFlags:
  #[arg(long, value_enum, default_value_t = SortKey::Name)] pub sort: SortKey,
  ```
  (Lives on `CommonFlags`; only `cmd_list` reads it. `// ponytail: shared flag, status ignores it — cheaper than splitting List into its own args struct.`)
- `output.rs`:
  - `list_table(plugins, enabled, est: &dyn Fn(&Plugin)->u32, total, enabled_total)` → add `COST` col + footer. (Pass an `est` closure or just call `p.est_tokens()` inline — inline is simpler; pass the two precomputed totals.)
  - `list_json(plugins, enabled)` → inject `"estTokens": p.est_tokens()` per plugin and compute `summary{totalEst,enabledEst}`.
- `main.rs cmd_list`: compute `total = Σ est`, `enabled_total = Σ est where enabled`; sort plugins by key before rendering (`SortKey::Cost` → sort_by est desc, tie-break name; `Name` → existing).

**Steps:**
- [ ] `cli.rs`: `SortKey` enum + `sort` flag on `CommonFlags`.
- [ ] `main.rs`: compute totals, apply sort, pass to renderers.
- [ ] `output.rs`: COST column + footer + honesty line; `summary` in JSON.
- [ ] Integration test: `--json` summary + `--sort cost` ordering.

---

## Task 3: `enabledEst` in `ccplug status`

**Goal:** Status shows the project's estimated enabled always-on cost — the headline trim number.

**Files:**
- Modify: `src/output.rs`, `src/main.rs`

**Acceptance Criteria:**
- [ ] `status` human output prints `enabled cost: ~<enabledEst> tok` line
- [ ] `status --json` adds `"enabledEst": <int>`
- [ ] `enabledEst` = Σ `est_tokens` over plugins effectively enabled in cwd (matches list's `enabled` total)

**Verify:** integration: a project with one plugin disabled → `status --json enabledEst` excludes it and equals `list --json summary.enabledEst`.

**Design:** `cmd_status` already has `effective` + inventory is loadable; load `plugins`, sum `est_tokens` for ids enabled in `eff.plugins`. Add to both the JSON object and the human print.

**Steps:**
- [ ] `main.rs cmd_status`: load inventory, compute `enabledEst`, add to JSON + human output.
- [ ] Integration test asserting parity with list summary.

---

## Task 4: Ship v0.2.0

**Goal:** Version bump, changelog, tag → release+publish pipeline runs.

**Files:**
- Modify: `Cargo.toml` (version `0.2.0`), `CHANGELOG.md`, `README.md` (mention cost column)

**Acceptance Criteria:**
- [ ] `Cargo.toml` version = `0.2.0`
- [ ] `CHANGELOG.md` has a `0.2.0` entry (cost-aware listing; notes presets/per-skill rejected)
- [ ] README `list` section mentions the COST column + `--sort cost`
- [ ] `cargo test --all` + `cargo clippy --all-targets -- -D warnings` + `cargo fmt --check` clean

**Verify:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --all`; then `git tag v0.2.0 && git push origin v0.2.0` triggers CI + release (publish job needs a fresh version, satisfied by the bump).

**Steps:**
- [ ] Bump version, write changelog, update README.
- [ ] Run the full gate locally, commit, push main.
- [ ] Tag `v0.2.0`, push tag, confirm release + crates.io publish succeed.

---

## Self-review
- Spec §2 estimator → Task 1. §3 list surface + §4 list JSON → Task 2. §3 status + §4 status JSON → Task 3. §5 shape covered across 1–3. §6 tests distributed per task. §7 out-of-scope respected (no subprocess, no presets, no per-skill).
- Type/name consistency: `est_tokens` (fn) + `Plugin::est_tokens` (method), `SortKey{Name,Cost}`, JSON keys `estTokens`/`summary.totalEst`/`summary.enabledEst`/`enabledEst` — used identically across tasks.
- No user-gate tasks (no ordering/proof language in the brief).

# ccplug — design spec

**Date:** 2026-06-19
**Status:** approved design, ready for implementation plan
**Author:** brainstormed with Claude

A fast Rust CLI to manage **which Claude Code plugins and skills are enabled per project**, so a 168-project workspace doesn't pay the context/startup cost of all globally-installed plugins in every project.

Model: install everything globally (auto-update on); `ccplug` writes a per-project enabled subset into the settings cascade. Ships a `SKILL.md` so Claude can drive it.

---

## 1. Problem & goal

- All plugins are installed at **user scope** in `~/.claude/settings.json` → every project loads every plugin's skill **descriptions** at session start (the context tax).
- Native Claude Code can scope this, but only via the levers in §2. There is **no** built-in "per-project plugin profile" feature.
- Goal: one small tool that lists the global inventory and applies a chosen subset per project, usable identically by a human and by an AI agent.

## 2. Verified native mechanisms (the constraints — do not re-research)

Confirmed against `code.claude.com/docs` + local inspection on 2026-06-19.

| Lever | Granularity | Per-project? | How |
|---|---|---|---|
| `enabledPlugins` | **per-plugin** | ✅ | object in settings.json, keys `"name@marketplace": bool`. Project settings override user settings **per key**. |
| `skillOverrides` | **per-skill** (`"on"` / `"name-only"` / `"off"`) | ✅ | object in settings, keys = skill `name`. **Only affects user/project/MCP skills — NOT plugin skills** ("manage those through `/plugin`"). |
| skill frontmatter `disable-model-invocation: true` | per-skill | ✗ (global + clobbered by update) | removes description from context; unusable for per-project. |
| `.claude/skills/` dir | per-skill | ✅ | project-local "loose" skills. |

**Cost model:** only skill *descriptions* load at session start; full body is lazy. So savings come from reducing the set of enabled-plugin skill descriptions.

**Settings cascade (highest→lowest):** managed → CLI args → `.claude/settings.local.json` → `.claude/settings.json` → `~/.claude/settings.json`. `enabledPlugins` / `skillOverrides` merge **per key**, nearer scope wins. (Confirmed indirectly by `/plugin disable name@mkt --scope project` writing project settings; verify empirically in the first test.)

**The decisive constraint:** there is **no native per-project toggle for an individual *plugin-owned* skill**. The only way is *extraction* (disable the plugin, copy its chosen skills into `.claude/skills/`, govern via `skillOverrides`) — which breaks on every auto-update (paths are versioned) and needs a repair/`sync` step. **Deferred to v2.**

## 3. Inventory data sources

- Active install path per plugin: `~/.claude/plugins/installed_plugins.json` (`version: 2`; `plugins["name@marketplace"]` → array of records with `scope` (`user`/`local`), `installPath`, `version`). Use the `user`-scope record's `installPath`.
- Skills of a plugin: glob `<installPath>/skills/*/SKILL.md`; parse YAML frontmatter `name`, `description` (+ note `disable-model-invocation`, `user-invocable`).
- A plugin may also ship MCP servers / agents / hooks / commands / an LSP — detect their presence so `disable` can **warn** "also disables this plugin's MCP server / LSP".
- Enabled state: `enabledPlugins` from the merged cascade.

## 4. Command surface (v1)

Noun-verb, cargo/git style. One mutation path; `enable`/`disable` differ only by a boolean.

```
ccplug list                  # inventory: every global plugin + its skills + enabled state
ccplug status                # what is EFFECTIVELY active in the cwd project after cascade
ccplug enable  <target>...   # bulk enable
ccplug disable <target>...   # bulk disable
```

**Target grammar (flat, self-describing — no nested arrays):**
```
target := <plugin>            e.g. vercel           → whole plugin
        | <plugin>:<skill>    e.g. vercel:nextjs    → one skill
        | <plugin>:*          e.g. vercel:*         → all skills of that plugin
```
- `<plugin>` resolves a bare name to `name@marketplace` via inventory; if ambiguous across marketplaces, require the full `name@marketplace`.
- `<skill>` is the SKILL.md `name`.

**Shared flags (all commands):**
- `--json` — machine-readable output (list/status: inventory; enable/disable: result `{changed, file, warnings}`).
- `--scope project|local|user` (default `project` → `.claude/settings.json`). `user` edits `~/.claude/settings.json`.
- `--from FILE` / `--stdin` — read targets as a JSON array, e.g. `["vercel","superpowers:*"]`. Same effect as positional args. This is the agent bulk path.
- `--dry-run` — print the diff, write nothing.

**Same core for every caller** (no human/agent split):
```
human:  ccplug disable vercel ponytail:ponytail-audit
agent:  ccplug disable --stdin <<< '["vercel","superpowers:*"]'   (or --from targets.json)
```
Optional later sugar `ccplug pick` (interactive multi-select) would only collect targets and call the same enable/disable — not in v1, no duplicate logic ever.

## 5. v1 behavior by target type

- **`<plugin>` / bare plugin** → write `enabledPlugins["name@mkt"] = true|false` in the chosen scope file (create file/keys if absent; never drop unrelated keys).
- **`<plugin>:<skill>` where skill is a LOOSE skill** (lives in a `.claude/skills/`) → write `skillOverrides[name] = "on"|"off"`.
- **`<plugin>:<skill>` where skill is PLUGIN-owned** → **not supported in v1**: exit non-zero with a clear message ("plugin-owned skills can't be toggled individually yet; disable the whole plugin, or wait for v2 `--extract`"). `--json` returns this as a structured `unsupported` entry rather than failing the whole batch.
- **`<plugin>:*`** → in v1, equivalent to toggling the whole plugin (since per-skill plugin control isn't available); emit a note saying so.

Partial-batch semantics: apply all resolvable targets, collect per-target results; exit non-zero only if *every* target failed. `--json` always lists per-target outcome.

## 6. Output schemas (`--json`)

```jsonc
// ccplug list --json
{ "plugins": [ { "id": "vercel@claude-plugins-official", "name": "vercel",
                 "marketplace": "claude-plugins-official", "version": "0.44.0",
                 "enabled": true, "provides": ["skills","mcp"],
                 "skills": [ { "name": "nextjs", "description": "…", "owner": "plugin" } ] } ] }

// ccplug status --json  (cwd project)
{ "project": "/abs/path", "scopeFiles": { "project": ".claude/settings.json", ... },
  "effectivePlugins": [ { "id": "...", "enabled": true, "source": "user|project|local" } ],
  "skillOverrides": { "deploy": "off" } }

// ccplug enable/disable --json
{ "file": ".claude/settings.json", "scope": "project", "dryRun": false,
  "results": [ { "target": "vercel", "type": "plugin", "action": "disabled", "ok": true },
               { "target": "vercel:nextjs", "type": "plugin-skill", "ok": false, "reason": "unsupported-v1" } ],
  "warnings": [ "vercel also provides an MCP server; disabling removes it here" ] }
```

## 7. Project conventions (mirror `csess` at /home/sibin/my-works/csess)

- Rust 2021, `clap` derive, `serde`/`serde_json`, `comfy-table` (human tables), `anyhow`, `dirs`, `rayon` (parallel SKILL.md parse). YAML frontmatter parse: `serde_yaml` (or hand-split `---`).
- `[profile.release] strip=true, lto=true`. `rustfmt.toml`.
- Files: `src/{main,cli,inventory,settings,target,output}.rs` (split by responsibility, keep each focused).
- `install.sh`, `README.md`, `CHANGELOG.md`, `SKILL.md`, `LICENSE` (MIT), crates.io metadata (keywords/categories), `tests/` with `assert_cmd` + `tempfile`.
- `SKILL.md`: teach Claude to `ccplug list --json` → decide subset for the project → `ccplug disable/enable … --json` → `ccplug status --json` to verify. Mark `disable-model-invocation` NOT set (Claude may invoke), but mutations should be obvious/safe (project scope, `--dry-run` available).

## 8. Safety

- Never rewrite a settings file wholesale: read → modify only `enabledPlugins` / `skillOverrides` keys → write back preserving everything else and formatting as much as practical.
- Back up the target settings file (`.bak`) on first mutation, or rely on git if the project is a repo.
- `--dry-run` shows unified diff.

## 9. Out of scope for v1 (v2+)

- Plugin-owned skill **extraction** (`ccplug skill disable --extract`) + `ccplug sync` to repair versioned-path symlinks after auto-update.
- `ccplug pick` interactive TUI.
- Project-type **presets** ("nuxt", "flutter" → recommended plugin set) and an auto-suggest based on files present.

## 10. First-test checklist (verify the one unconfirmed assumption)

1. In a throwaway project, write `.claude/settings.json` with `{"enabledPlugins":{"vercel@claude-plugins-official":false}}`.
2. Start a session there; confirm vercel's skills are gone from context while still enabled globally.
3. If per-key project override does NOT disable a user-enabled plugin, fall back to documenting that `disable` must operate at `user` scope or via `/plugin disable --scope project`, and adjust §5.

---

## Appendix: account note

New repo → per global CLAUDE.md, confirm **personal vs work** before `git init`/remote. `csess` is personal (`sibincbaby`, on crates.io); `ccplug` likely personal too — confirm before first push.

---
name: ccplug
description: Use when you want to reduce a project's Claude Code startup/context cost by enabling only the plugins (and loose skills) it actually needs. Manages per-project `enabledPlugins`/`skillOverrides` in the settings cascade. Run `ccplug list/status/enable/disable`.
---

# ccplug

A fast CLI to control **which globally-installed Claude Code plugins are enabled per project**. Everything stays installed globally (auto-update on); ccplug writes a per-project enabled subset into `.claude/settings.json`, so a project only pays the context cost of the plugins it needs.

## The loop (do this)

1. **See the inventory + current state:**
   ```bash
   ccplug list --json
   ```
   → every global plugin with `id`, `enabled`, `provides`, and its `skills`.

2. **Decide the subset** this project needs (e.g. a Nuxt app probably wants `vercel` but not `firebase` or the Python LSPs).

3. **Apply** — disable what's not needed (or enable a curated set). Bulk via positional args or a JSON array on stdin:
   ```bash
   ccplug disable firebase pyright-lsp --json
   ccplug enable --stdin --json <<< '["vercel","ponytail"]'
   ```
   Preview first with `--dry-run`.

4. **Verify:**
   ```bash
   ccplug status --json
   ```
   → `effectivePlugins` (with the winning `source` scope) after the cascade.

## Targets

```
plugin            e.g. vercel                 → the whole plugin
plugin@market     e.g. vercel@claude-...      → disambiguate same name across marketplaces
plugin:skill      e.g. vercel:nextjs          → a single skill  (see v1 limit below)
plugin:*          e.g. vercel:*               → all skills of a plugin (v1: same as whole plugin)
```
A bare `plugin` name auto-resolves to `name@marketplace`. If a name exists in two marketplaces, use the full `name@marketplace`.

## Flags

- `--json` — machine-readable output (use this when driving ccplug as an agent).
- `--scope project|local|user` (default `project` = `.claude/settings.json`; `local` = `.claude/settings.local.json`; `user` = `~/.claude/settings.json`).
- `--from FILE` / `--stdin` — read targets as a JSON array of strings.
- `--dry-run` — print what would change, write nothing.

## v1 limits (important)

- **Per-skill toggling of a plugin-owned skill is NOT supported.** Claude Code's native switch is per-plugin only. A `plugin:skill` target where the skill belongs to a plugin returns `{"ok":false,"reason":"unsupported-v1"}` — it does **not** fail the rest of the batch. To drop those skills, disable the whole plugin, or wait for v2 extraction.
- `plugin:*` is treated as the whole plugin (with a warning), for the same reason.
- Disabling a plugin also removes any MCP server / LSP / agents it ships in that project — ccplug warns when this applies.

## Safety

- Mutations only touch `enabledPlugins` / `skillOverrides`; all other settings keys are preserved.
- The target settings file is backed up to `<file>.bak` before the first write.
- Restart the Claude Code session for changes to take effect.

This skill leaves `disable-model-invocation` unset — you may invoke `ccplug`. Mutations are project-scoped and reversible; prefer `--dry-run` first when unsure.

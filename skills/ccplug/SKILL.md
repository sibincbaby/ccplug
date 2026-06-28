---
name: ccplug
description: Use when you want to reduce a project's Claude Code startup/context cost by enabling only the plugins (and loose skills) it actually needs. Manages per-project `enabledPlugins`/`skillOverrides` in the settings cascade. Run `ccplug list/status/enable/disable`.
---

# ccplug

A fast CLI to control **which globally-installed Claude Code plugins are enabled per project**. Everything stays installed globally (auto-update on); ccplug writes a per-project enabled subset into `.claude/settings.json`, so a project only pays the context cost of the plugins it needs.

**Prerequisite:** this skill drives the `ccplug` binary — it does not ship it. If `ccplug` is not on PATH, tell the user to install it (`cargo install ccplug`, or the `install.sh` one-liner in the README) and stop; do not try to build it from source yourself.

## The loop (do this)

1. **See the inventory + current state:**
   ```bash
   ccplug list --json
   ```
   → every global plugin with `id`, `enabled`, `provides`, and its `skills` — and **each skill carries its own `description`**.

2. **Decide the subset by reading descriptions, not names.** Each skill's `description` is its "what it does + when to use it" summary (the same text Claude uses to decide when to fire the skill). Judge fit from that, not the plugin name. A name like `vercel` tells you little; its skills' descriptions tell you whether this project will ever call them. Only crack open a skill's full `SKILL.md` body if a description is genuinely ambiguous — bodies are large and lazy-loaded, so reading them wholesale re-pays the very context tax ccplug exists to remove.

   Decide on **fit × cost**: a high `estTokens` skill the project will never trigger is the first to disable; a cheap, occasionally-useful one can stay.

3. **Re-evaluate when the work changes, not just at startup.** The right subset is dynamic. Re-run `ccplug list --json` and adjust when: the task moves into a new domain (added a DB → maybe enable its plugin), a skill keeps not firing (disable it), or a newly-installed plugin appears in the list. ccplug only governs the *already-installed* global set — discovering and installing a new plugin from a marketplace is a separate `claude plugin install` step; once installed it shows up here and you reason about it like any other.

4. **Apply** — disable what's not needed (or enable a curated set). Bulk via positional args or a JSON array on stdin:
   ```bash
   ccplug disable firebase pyright-lsp --json
   ccplug enable --stdin --json <<< '["vercel","ponytail"]'
   ```
   Preview first with `--dry-run`.

5. **Verify:**
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
- **ccplug refuses to disable itself.** A `disable` target that resolves to ccplug (its own plugin or a loose `ccplug` skill) returns `{"ok":false,"reason":"self-protect: refusing to disable ccplug"}` and does not fail the rest of the batch — disabling it would remove the very tool running the command. Enabling ccplug is allowed.
- Restart the Claude Code session for changes to take effect.

This skill leaves `disable-model-invocation` unset — you may invoke `ccplug`. Mutations are project-scoped and reversible; prefer `--dry-run` first when unsure.

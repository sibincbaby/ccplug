# ccplug

Manage **which Claude Code plugins and skills are enabled per project**.

Claude Code installs plugins at **user scope**, so *every* project loads *every* plugin's skill descriptions at session start — a context/startup tax that adds up across a large workspace. ccplug keeps everything installed globally (auto-update on) and writes a per-project **enabled subset** into the settings cascade, so each project only pays for what it uses.

Built to be driven identically by a human or by Claude itself (ships a `SKILL.md`).

## Install

```bash
cargo install ccplug
```

Or grab a prebuilt Linux binary (downloads the latest release tarball into `/usr/local/bin`, override with `DEST=`):

```bash
curl -fsSL https://raw.githubusercontent.com/sibincbaby/ccplug/main/install.sh | bash
```

Or from a checkout: `cargo install --path .`

The `install.sh` path also installs the agent-facing skill automatically. `cargo install` only installs the binary — run `ccplug skill install` once afterwards to drop the skill in (it's embedded in the binary; `--force` refreshes it).

### As a Claude Code plugin

This repo is also a self-hosted plugin marketplace. The plugin ships the skill (not the binary), so install both:

```bash
cargo install ccplug                            # the CLI the skill drives
claude plugin marketplace add sibincbaby/ccplug
claude plugin install ccplug@ccplug             # the skill
```

## Commands

```
ccplug list                  # every global plugin + its skills + enabled state + est cost
ccplug list --sort cost      # rank plugins by estimated always-on token cost (expensive first)
ccplug status                # what is EFFECTIVELY active in the cwd project after the cascade
ccplug enable  <target>...   # bulk enable
ccplug disable <target>...   # bulk disable
ccplug skill install         # write the bundled skill to ~/.claude/skills/ccplug (--force to refresh)
```

`list` shows a **COST** column — an estimate of each plugin's always-on token cost (its skill descriptions, which load every session) — with a footer totalling enabled vs all. `status` reports the project's enabled cost (`enabledEst`). Cost is a local `chars/4` estimate; `claude plugin details <name>` gives exact numbers.

### Deciding what to enable (for Claude, or you)

`ccplug list --json` returns each skill with its own `description` — the "what it does + when to use it" summary (the same text Claude uses to decide when to fire a skill). **Decide by description, not by plugin name.** A name like `vercel` says little; its skills' descriptions tell you whether this project will ever call them. Only open a skill's full `SKILL.md` body if a description is genuinely ambiguous — bodies are large and lazy-loaded, so reading them wholesale re-pays the very context tax ccplug removes.

Weigh **fit × cost**: a high-`estTokens` skill the project will never trigger is the first to disable; a cheap, occasionally-useful one can stay.

The right subset is **dynamic** — re-run `list` and adjust when the task moves into a new domain, a skill keeps not firing, or a newly-installed plugin appears. ccplug governs only the *already-installed* global set; discovering and installing a new plugin from a marketplace is a separate `claude plugin install` step, after which it shows up here like any other.

### Targets

```
plugin            vercel                       whole plugin (bare name auto-resolves to name@marketplace)
plugin@market     vercel@claude-plugins-...    disambiguate a name shared across marketplaces
plugin:skill      vercel:nextjs                a single skill   (see "v1 limits")
plugin:*          vercel:*                     all skills of a plugin (v1: whole plugin)
```

### Flags (all commands)

| Flag | Meaning |
|---|---|
| `--json` | machine-readable output |
| `--scope project\|local\|user` | which settings file to read/write (default `project`) |
| `--from FILE` / `--stdin` | read targets as a JSON array of strings |
| `--dry-run` | print the change, write nothing |

Scope → file: `project` = `.claude/settings.json`, `local` = `.claude/settings.local.json`, `user` = `~/.claude/settings.json`. Nearer scope wins per key.

## Examples

```bash
ccplug list --json
ccplug disable firebase pyright-lsp                 # trim a frontend project
ccplug enable --stdin <<< '["vercel","ponytail"]'   # agent / bulk path
ccplug disable vercel --dry-run                      # preview (warns it also drops the MCP server)
ccplug status --json
```

Restart the Claude Code session for changes to take effect.

## v1 limits

Claude Code's native on/off switch is **per-plugin** (`enabledPlugins`). There is **no native per-project toggle for an individual plugin-owned skill** — `skillOverrides` only governs loose/project skills, not plugin skills.

So in v1:

- A `plugin:skill` target where the skill is **plugin-owned** returns `{"ok": false, "reason": "unsupported-v1"}` and does **not** fail the rest of the batch. Disable the whole plugin instead, or wait for v2.
- `plugin:*` toggles the whole plugin (with a warning).
- Disabling a plugin also removes any MCP server / LSP / agents it ships in that project; ccplug warns when so.

Per-skill extraction of plugin-owned skills (`--extract` + a `sync` step), an interactive `pick`, and project-type presets are planned for **v2** — see `docs/spec.md` §9.

## Safety

- Only `enabledPlugins` / `skillOverrides` keys are modified; every other settings key (and key order) is preserved — the file is never rewritten wholesale.
- The target file is backed up to `<file>.bak` before the first write.
- `--dry-run` shows the resulting file without writing.

## License

MIT

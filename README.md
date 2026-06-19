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

## Commands

```
ccplug list                  # every global plugin + its skills + enabled state
ccplug status                # what is EFFECTIVELY active in the cwd project after the cascade
ccplug enable  <target>...   # bulk enable
ccplug disable <target>...   # bulk disable
```

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

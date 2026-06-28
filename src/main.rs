mod cli;
mod inventory;
mod output;
mod settings;
mod target;

use anyhow::Result;
use clap::Parser;
use std::io::Read;
use std::path::PathBuf;

use cli::{Cli, Command, CommonFlags, MutateArgs, SkillAction};
use output::TargetResult;
use settings::Change;
use target::{resolve, Kind};

/// The skill shipped with the plugin — embedded so `cargo install` users can drop it in.
const SKILL_MD: &str = include_str!("../skills/ccplug/SKILL.md");

fn main() {
    let code = match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            1
        }
    };
    std::process::exit(code);
}

/// Returns the process exit code (0 ok; 2 = every target failed).
fn run(cli: Cli) -> Result<i32> {
    match cli.command {
        Command::List(f) => cmd_list(f).map(|_| 0),
        Command::Status(f) => cmd_status(f).map(|_| 0),
        Command::Enable(a) => cmd_mutate(a, true),
        Command::Disable(a) => cmd_mutate(a, false),
        Command::Skill {
            action:
                SkillAction::Install {
                    force,
                    dry_run,
                    home_dir,
                },
        } => cmd_skill_install(home_dir, force, dry_run),
    }
}

/// Write the embedded SKILL.md to ~/.claude/skills/ccplug/SKILL.md (a loose user skill).
fn cmd_skill_install(home_dir: Option<String>, force: bool, dry_run: bool) -> Result<i32> {
    let home = match home_dir {
        Some(d) => PathBuf::from(d),
        None => {
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?
        }
    };
    let dest = home.join(".claude/skills/ccplug/SKILL.md");

    if dry_run {
        println!("would write {}", dest.display());
        return Ok(0);
    }
    if dest.exists() && !force {
        println!(
            "ccplug skill already installed at {} (use --force to overwrite)",
            dest.display()
        );
        return Ok(0);
    }
    std::fs::create_dir_all(dest.parent().unwrap())?;
    std::fs::write(&dest, SKILL_MD)?;
    println!("installed ccplug skill → {}", dest.display());
    Ok(0)
}

/// Resolve the home (~/.claude parent) and project dir, honoring hidden test overrides.
fn dirs_from(f: &CommonFlags) -> Result<(PathBuf, PathBuf)> {
    let home = match &f.home_dir {
        Some(d) => PathBuf::from(d),
        None => {
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?
        }
    };
    let project = match &f.project_dir {
        Some(d) => PathBuf::from(d),
        None => std::env::current_dir()?,
    };
    Ok((home, project))
}

fn cmd_list(f: CommonFlags) -> Result<()> {
    let (home, project) = dirs_from(&f)?;
    let mut plugins = inventory::load(&home)?; // already name-sorted
    if f.sort == cli::SortKey::Cost {
        // expensive first; tie-break by name to stay deterministic
        plugins.sort_by(|a, b| {
            b.est_tokens()
                .cmp(&a.est_tokens())
                .then_with(|| a.name.cmp(&b.name))
        });
    }
    let eff = settings::effective(&project, &home)?;
    let is_enabled = |id: &str| eff.plugins.iter().any(|(k, b, _)| k == id && *b);

    if f.json {
        println!("{}", output::list_json(&plugins, &is_enabled)?);
    } else {
        println!("{}", output::list_table(&plugins, &is_enabled));
    }
    Ok(())
}

fn cmd_status(f: CommonFlags) -> Result<()> {
    let (home, project) = dirs_from(&f)?;
    let eff = settings::effective(&project, &home)?;

    // Per-plugin estimated always-on cost, keyed by id (0 if the plugin isn't installed).
    let inv = inventory::load(&home)?;
    let cost_of = |id: &str| -> u32 {
        inv.iter()
            .find(|p| p.id == id)
            .map(|p| p.est_tokens())
            .unwrap_or(0)
    };
    let enabled_est: u32 = eff
        .plugins
        .iter()
        .filter(|(_, on, _)| *on)
        .map(|(id, _, _)| cost_of(id))
        .sum();

    if f.json {
        let plugins: Vec<serde_json::Value> = eff
            .plugins
            .iter()
            .map(|(id, on, src)| {
                serde_json::json!({ "id": id, "enabled": on, "source": src.as_str(), "estTokens": cost_of(id) })
            })
            .collect();
        let overrides: serde_json::Map<String, serde_json::Value> = eff
            .overrides
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "project": project.to_string_lossy(),
                "scopeFiles": {
                    "project": settings::scope_path(cli::Scope::Project, &project, &home).to_string_lossy(),
                    "local": settings::scope_path(cli::Scope::Local, &project, &home).to_string_lossy(),
                    "user": settings::scope_path(cli::Scope::User, &project, &home).to_string_lossy(),
                },
                "effectivePlugins": plugins,
                "skillOverrides": overrides,
                "enabledEst": enabled_est,
            }))?
        );
    } else {
        use comfy_table::presets::UTF8_BORDERS_ONLY;
        use comfy_table::Table;
        let mut t = Table::new();
        t.load_preset(UTF8_BORDERS_ONLY);
        t.set_header(vec!["PLUGIN", "ON", "COST (tok)", "SOURCE"]);
        for (id, on, src) in &eff.plugins {
            t.add_row(vec![
                id.clone(),
                if *on { "✓".into() } else { "·".into() },
                format!("~{}", cost_of(id)),
                src.as_str().to_string(),
            ]);
        }
        println!("project: {}", project.display());
        println!("{t}");
        println!("enabled cost: ~{enabled_est} tok (est. always-on from skill descriptions)");
        if !eff.overrides.is_empty() {
            println!("skillOverrides:");
            for (k, v) in &eff.overrides {
                println!("  {k} = {v}");
            }
        }
    }
    Ok(())
}

/// Collect targets from positional args, then `--from FILE`, then `--stdin` (JSON arrays).
fn gather_targets(a: &MutateArgs) -> Result<Vec<String>> {
    let mut targets = a.targets.clone();
    if let Some(file) = &a.from {
        let text =
            std::fs::read_to_string(file).map_err(|e| anyhow::anyhow!("reading {file}: {e}"))?;
        targets.extend(parse_target_array(&text)?);
    }
    if a.stdin {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text)?;
        targets.extend(parse_target_array(&text)?);
    }
    if targets.is_empty() {
        anyhow::bail!("no targets given (positional, --from FILE, or --stdin)");
    }
    Ok(targets)
}

fn parse_target_array(text: &str) -> Result<Vec<String>> {
    serde_json::from_str::<Vec<String>>(text)
        .map_err(|e| anyhow::anyhow!("expected a JSON array of target strings: {e}"))
}

/// True if a resolved target is ccplug itself — its plugin (`ccplug@<market>`) or a
/// loose skill named `ccplug`. Used to self-protect against disabling the running tool.
fn targets_ccplug(r: &target::Resolved) -> bool {
    r.plugin_id
        .as_deref()
        .is_some_and(|id| id.split('@').next() == Some("ccplug"))
        || r.skill.as_deref() == Some("ccplug")
}

fn cmd_mutate(a: MutateArgs, enable: bool) -> Result<i32> {
    let f = a.common.clone();
    let (home, project) = dirs_from(&f)?;
    let targets = gather_targets(&a)?;

    let plugins = inventory::load(&home)?;
    let loose = inventory::loose_skills(&project, &home);

    let mut change = Change::default();
    let mut results = Vec::new();
    let mut warnings = Vec::new();
    let mut any_ok = false;

    for raw in &targets {
        let r = resolve(raw, &plugins, &loose);
        let kind = match r.kind {
            Kind::Plugin => "plugin",
            Kind::PluginGlob => "plugin-glob",
            Kind::PluginSkill => "plugin-skill",
            Kind::LooseSkill => "loose-skill",
        };

        if !r.ok {
            results.push(TargetResult {
                target: raw.clone(),
                kind: kind.to_string(),
                ok: false,
                action: None,
                reason: r.reason.clone(),
            });
            continue;
        }

        // Self-protect: never let a disable remove ccplug itself — the tool running this.
        if !enable && targets_ccplug(&r) {
            results.push(TargetResult {
                target: raw.clone(),
                kind: kind.to_string(),
                ok: false,
                action: None,
                reason: Some("self-protect: refusing to disable ccplug".to_string()),
            });
            continue;
        }
        any_ok = true;
        let action = if enable { "enabled" } else { "disabled" };

        match r.kind {
            Kind::Plugin | Kind::PluginGlob => {
                let id = r.plugin_id.clone().unwrap();
                change.set_plugin.push((id.clone(), enable));
                if r.kind == Kind::PluginGlob {
                    warnings.push(format!(
                        "{raw}: per-skill plugin control is unavailable in v1; {action} the whole plugin {id}"
                    ));
                }
                if !enable {
                    if let Some(p) = plugins.iter().find(|p| p.id == id) {
                        let extra: Vec<&str> = p
                            .provides
                            .iter()
                            .map(String::as_str)
                            .filter(|x| matches!(*x, "mcp" | "lsp" | "agents"))
                            .collect();
                        if !extra.is_empty() {
                            warnings.push(format!(
                                "{id} also provides {}; disabling removes them here",
                                extra.join(", ")
                            ));
                        }
                    }
                }
                results.push(TargetResult {
                    target: raw.clone(),
                    kind: kind.to_string(),
                    ok: true,
                    action: Some(action.to_string()),
                    reason: None,
                });
            }
            Kind::LooseSkill => {
                let name = r.skill.clone().unwrap();
                let state = if enable { "on" } else { "off" };
                change.set_override.push((name, state.to_string()));
                results.push(TargetResult {
                    target: raw.clone(),
                    kind: kind.to_string(),
                    ok: true,
                    action: Some(action.to_string()),
                    reason: None,
                });
            }
            Kind::PluginSkill => unreachable!("plugin-skill is never ok in v1"),
        }
    }

    let path = settings::scope_path(f.scope, &project, &home);
    let file = path.to_string_lossy().to_string();
    let applied = settings::apply(&path, &change, a.dry_run)?;

    if f.json {
        println!(
            "{}",
            output::mutate_json(&file, f.scope.as_str(), a.dry_run, &results, &warnings)?
        );
    } else {
        let diff = if a.dry_run {
            Some(applied.after.as_str())
        } else {
            None
        };
        let backup = applied
            .backup
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        println!(
            "{}",
            output::mutate_human(
                &file,
                a.dry_run,
                &results,
                &warnings,
                diff,
                applied.wrote,
                backup.as_deref(),
            )
        );
    }

    // Exit non-zero only if EVERY target failed.
    Ok(if any_ok { 0 } else { 2 })
}

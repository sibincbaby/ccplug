mod cli;
mod inventory;
mod output;
mod settings;
mod target;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use cli::{Cli, Command, CommonFlags};

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::List(f) => cmd_list(f),
        Command::Status(f) => cmd_status(f),
        Command::Enable(_) | Command::Disable(_) => {
            anyhow::bail!("enable/disable land after the spec §10 empirical pre-check")
        }
    }
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
    let plugins = inventory::load(&home)?;
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

    if f.json {
        let plugins: Vec<serde_json::Value> = eff
            .plugins
            .iter()
            .map(|(id, on, src)| {
                serde_json::json!({ "id": id, "enabled": on, "source": src.as_str() })
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
            }))?
        );
    } else {
        use comfy_table::presets::UTF8_BORDERS_ONLY;
        use comfy_table::Table;
        let mut t = Table::new();
        t.load_preset(UTF8_BORDERS_ONLY);
        t.set_header(vec!["PLUGIN", "ON", "SOURCE"]);
        for (id, on, src) in &eff.plugins {
            t.add_row(vec![
                id.clone(),
                if *on { "✓".into() } else { "·".into() },
                src.as_str().to_string(),
            ]);
        }
        println!("project: {}", project.display());
        println!("{t}");
        if !eff.overrides.is_empty() {
            println!("skillOverrides:");
            for (k, v) in &eff.overrides {
                println!("  {k} = {v}");
            }
        }
    }
    Ok(())
}

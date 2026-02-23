pub mod config;
pub mod date_parser;
pub mod diff;
pub mod recurrence_parser;
pub mod storage;
pub mod sync;
pub mod types;

use anyhow::{bail, Result};
use config::AppConfig;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let command = args.first().map(String::as_str).unwrap_or("help");

    match command {
        "setup" => {
            let config = AppConfig::load()?;
            let remote_override = args.get(1).map(String::as_str);
            sync::setup(&config, remote_override)?;
            println!("setup complete at {}", config.config_dir.display());
            println!("todo source: {}", config.todo_file.display());
        }
        "sync" => {
            let config = AppConfig::load()?;
            let result = sync::sync(&config)?;
            println!(
                "sync {} | added {} updated {} deleted {} completed {} | {}",
                if result.committed {
                    "committed"
                } else {
                    "no local todo changes"
                },
                result.change_set.added,
                result.change_set.updated,
                result.change_set.deleted,
                result.change_set.completed,
                result.line_summary
            );
            if !result.change_set.changes.is_empty() {
                for change in &result.change_set.changes {
                    println!("- {:?}: {}", change.kind, change.id);
                }
            }
        }
        "where" => {
            let config = AppConfig::load()?;
            println!("config: {}", config.config_dir.display());
            println!("todo: {}", config.todo_file.display());
            println!("env: {}", config.env_file.display());
            println!("branch: {}", config.git_branch);
            if let Some(remote) = config.git_remote {
                println!("remote: {remote}");
            }
            if config.github_token.is_some() {
                println!("github token: set");
            }
        }
        "help" | "-h" | "--help" => {
            print_help();
        }
        _ => bail!("unknown command `{command}`; run `todo_md help`"),
    }

    Ok(())
}

fn print_help() {
    println!("todo_md commands:");
    println!("  setup [remote-url]  Initialize ~/.config/todos and git repo");
    println!("  sync                Pull/rebase, diff todo.md, commit, and push");
    println!("  where               Show resolved config and todo paths");
}

mod command;
mod config;
mod migrate;
mod rules;
mod runner;
mod shells;
mod ui;

use std::env;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::command::Command;
use crate::config::Config;
use crate::shells::Shell;

/// Fixes the command you just got wrong.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Print the shell setup. Add `eval "$(welp init zsh)"` to your shell config.
    Init {
        shell: Shell,
        /// The name you type to fix a command.
        #[arg(long, default_value = "welp")]
        alias: String,
    },
    /// Fix a command (the shell setup calls this for you).
    Fix {
        /// The command to fix. Defaults to the last one in WELP_HISTORY.
        command: Option<String>,
        /// Run the first fix without asking.
        #[arg(short, long)]
        yes: bool,
    },
    /// Move from thefuck: switch your shell config and import its settings.
    Migrate {
        /// Show what would change without changing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// List the rules.
    Rules,
}

fn main() -> ExitCode {
    match Cli::parse().action {
        Action::Init { shell, alias } => {
            print!("{}", shell.script(&alias));
            ExitCode::SUCCESS
        }
        Action::Fix { command, yes } => fix(command, yes),
        Action::Migrate { dry_run } => migrate(dry_run),
        Action::Rules => {
            for rule in rules::all() {
                println!("{:<24}{}", rule.name, rule.description);
            }
            ExitCode::SUCCESS
        }
    }
}

fn fix(command: Option<String>, yes: bool) -> ExitCode {
    let config = Config::load();
    let script = command.or_else(|| {
        let history = env::var("WELP_HISTORY").unwrap_or_default();
        let mut lines: Vec<&str> = history.lines().collect();
        if env::var_os("WELP_NEWEST_FIRST").is_some() {
            lines.reverse();
        }
        let alias = env::var("WELP_ALIAS").unwrap_or_else(|_| "welp".into());
        command::last_command(&lines, &alias).map(str::to_string)
    });
    let Some(script) = script else {
        eprintln!("welp: no command to fix");
        return ExitCode::FAILURE;
    };

    let output = runner::capture_output(&script, &config);
    let fixes = rules::suggest(&Command::new(&script, &output), &config);
    if fixes.is_empty() {
        eprintln!("welp: no fix for `{script}`");
        return ExitCode::FAILURE;
    }
    match ui::pick(&fixes, config.require_confirmation && !yes) {
        Some(fix) => {
            println!("{fix}");
            ExitCode::SUCCESS
        }
        None => ExitCode::FAILURE,
    }
}

fn migrate(dry_run: bool) -> ExitCode {
    let Some(home) = dirs::home_dir() else {
        eprintln!("welp: can't find your home folder");
        return ExitCode::FAILURE;
    };
    let report = match migrate::migrate(&home, &config::config_path(), dry_run) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("welp: migration failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let verb = if dry_run { "Would update" } else { "Updated" };
    if report.shell_files.is_empty() && !report.settings_imported {
        println!("No thefuck setup found. Add welp with: eval \"$(welp init zsh)\"");
        return ExitCode::SUCCESS;
    }
    for path in &report.shell_files {
        println!(
            "{verb} {} (backup: {}.welp-backup)",
            path.display(),
            path.display()
        );
    }
    if let Some(alias) = &report.alias {
        if dry_run {
            println!("`{alias}` would run welp.");
        } else {
            println!("`{alias}` now runs welp. Open a new terminal to use it.");
        }
    }
    if report.settings_imported {
        println!(
            "{verb} {} from your thefuck settings",
            config::config_path().display()
        );
    }
    if !dry_run {
        println!("You can now remove thefuck: pip uninstall thefuck (or brew uninstall thefuck)");
    }
    ExitCode::SUCCESS
}

mod command;
mod config;
mod custom;
mod migrate;
mod rules;
mod runner;
mod shells;
mod stats;
mod style;
mod ui;
mod voice;

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
        /// Run the first fix without asking (thefuck's `--yeah` works too).
        #[arg(short, long, visible_alias = "yeah")]
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
    /// Show what welp remembers about your typing.
    Stats,
}

fn main() -> ExitCode {
    match Cli::parse().action {
        Action::Init { shell, alias } => {
            print!("{}", shell.script(&alias));
            ExitCode::SUCCESS
        }
        Action::Fix { command, yes } => fix(command, yes),
        Action::Migrate { dry_run } => migrate(dry_run),
        Action::Stats => {
            let config = Config::load();
            for (index, line) in stats::Stats::load().card(config.voice()).iter().enumerate() {
                if index == 0 {
                    ui::say(line);
                } else {
                    ui::aside(line);
                }
            }
            ExitCode::SUCCESS
        }
        Action::Rules => {
            let config = Config::load();
            let disabled = rules::disabled_by_default();
            let dir = config::config_dir().join("rules");
            let (custom_rules, problems) = custom::load(&dir);
            let all = rules::all();
            let width = all
                .iter()
                .map(|rule| rule.name.len())
                .chain(custom_rules.iter().map(|rule| rule.name.len()))
                .max()
                .unwrap_or(0)
                + 2;
            for rule in &all {
                let off = disabled.contains(&rule.name)
                    && !config.enable_rules.iter().any(|name| name == rule.name);
                let note = if off {
                    " (off until you add it to enable_rules)"
                } else {
                    ""
                };
                println!("{:<width$}{}{note}", rule.name, rule.description);
            }
            for rule in &custom_rules {
                println!("{:<width$}{} (yours)", rule.name, rule.description);
            }
            for problem in problems {
                ui::say(&format!("skipped {problem}"));
            }
            ExitCode::SUCCESS
        }
    }
}

fn fix(command: Option<String>, yes: bool) -> ExitCode {
    let config = Config::load();
    let history = env::var("WELP_HISTORY").unwrap_or_default();
    let mut lines: Vec<&str> = history.lines().collect();
    if env::var_os("WELP_NEWEST_FIRST").is_some() {
        lines.reverse();
    }
    let alias = env::var("WELP_ALIAS").unwrap_or_else(|_| "welp".into());
    let from_history = command.is_none();
    let script = command.or_else(|| command::last_command(&lines, &alias).map(str::to_string));
    let Some(script) = script else {
        ui::say(voice::NOTHING_TO_FIX);
        return ExitCode::FAILURE;
    };
    // The commands before this one, oldest first, for rules that look back.
    let mut recent = command::recent(&lines, &alias);
    if from_history && recent.last() == Some(&script) {
        recent.pop();
    }

    let output = runner::capture_output(&script, &config);
    let command = Command::new(&script, &output).with_history(recent);
    let (custom_rules, _) = custom::load(&config::config_dir().join("rules"));
    let yours = |first: bool| -> Vec<rules::Fix> {
        custom_rules
            .iter()
            .filter(|rule| rule.first == first && !config.exclude_rules.contains(&rule.name))
            .flat_map(|rule| rule.fixes(&command))
            .collect()
    };
    let mut fixes: Vec<rules::Fix> = Vec::new();
    for fix in yours(true)
        .into_iter()
        .chain(rules::suggest(&command, &config))
        .chain(yours(false))
    {
        if fix.command != script && !fixes.iter().any(|known| known.command == fix.command) {
            fixes.push(fix);
        }
    }
    let voice = config.voice();
    if fixes.is_empty() {
        ui::say(&voice::no_fix(voice, voice::seed()));
        return ExitCode::FAILURE;
    }
    let Some(fix) = ui::pick(&fixes, config.require_confirmation && !yes) else {
        return ExitCode::FAILURE;
    };
    let mut stats = if config.remember {
        stats::Stats::load()
    } else {
        stats::Stats::default()
    };
    let memory = stats.record(&fix.rule, &script, &fix.command, stats::now());
    if config.remember {
        stats.save();
    }
    if let Some(line) = voice::after_fix(&fix.rule, &memory, voice, voice::seed()) {
        ui::aside(&line);
    }
    println!("{}", fix.command);
    ExitCode::SUCCESS
}

fn migrate(dry_run: bool) -> ExitCode {
    let Some(home) = dirs::home_dir() else {
        ui::say("couldn't find your home folder.");
        return ExitCode::FAILURE;
    };
    let report = match migrate::migrate(&home, &config::config_path(), dry_run) {
        Ok(report) => report,
        Err(error) => {
            ui::say(&format!("migration failed: {error}"));
            return ExitCode::FAILURE;
        }
    };
    let short = |path: &std::path::Path| match path.strip_prefix(&home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    };
    if report.shell_files.is_empty() && !report.settings_imported {
        ui::say("no thefuck setup here. to set welp up, add this to your shell config:");
        eprintln!("        eval \"$(welp init zsh)\"");
        return ExitCode::SUCCESS;
    }
    for path in &report.shell_files {
        if dry_run {
            ui::say(&format!("would update {}", short(path)));
        } else {
            ui::say(&format!(
                "updated {} (old one saved as {}.welp-backup)",
                short(path),
                short(path)
            ));
        }
    }
    if report.settings_imported {
        let config = short(&config::config_path());
        if dry_run {
            ui::say(&format!(
                "would bring your thefuck settings over to {config}"
            ));
        } else {
            ui::say(&format!("brought your thefuck settings over to {config}"));
        }
    }
    if let Some(alias) = &report.alias {
        if dry_run {
            ui::say(&format!("`{alias}` would run welp after that."));
        } else {
            ui::say(&format!(
                "moved your thefuck setup over. `{alias}` still works, it just runs welp now."
            ));
            ui::say(
                "open a new terminal to try it. thefuck can go: pip uninstall thefuck (or brew uninstall thefuck)",
            );
        }
    }
    ExitCode::SUCCESS
}

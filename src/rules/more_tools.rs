//! More language and tool rules ported from thefuck: adb, aws, az, cargo,
//! composer, conda, docker, fab, gem, go, gradle, grunt, gulp, heroku, lein,
//! mvn, npm, pip, python, rails, react-native, terraform, tsuru, vagrant,
//! workon and yarn.
//!
//! Where thefuck runs a tool to learn the valid names, the rule here runs the
//! same help or listing command through `listing` (read-only, 3 s timeout),
//! or reads the file the information lives in.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::{Rule, listing, replace_arg, replace_program};
use crate::command::Command;

/// Rules in this file that are off unless the user enables them.
pub const DISABLED: &[&str] = &[
    // Its fix force-removes a container the user never mentioned.
    "docker_image_being_used_by_container",
    // `--delete-ghost-migrations` deletes migration records from the database;
    // South's own message says "this may not be good".
    "django_south_ghost",
];

pub fn rules() -> Vec<Rule> {
    vec![
        Rule {
            name: "adb_unknown_command",
            description: "Fixes a mistyped adb command",
            priority: 1100,
            matches: |c| {
                is_app(c, &["adb"]) && c.output.starts_with("Android Debug Bridge version")
            },
            fixes: adb_unknown_command,
        },
        Rule {
            name: "ag_literal",
            description: "Searches ag for a literal string with -Q after a bad regex",
            priority: 1000,
            matches: |c| is_app(c, &["ag"]) && c.output.trim_end().ends_with("run ag with -Q"),
            fixes: |c| vec![replace_program(&c.script, "ag -Q")],
        },
        Rule {
            name: "aws_cli",
            description: "Takes the aws command the cli suggests",
            priority: 1000,
            matches: |c| {
                is_app(c, &["aws"])
                    && c.output.contains("usage:")
                    && c.output.contains("maybe you meant:")
            },
            fixes: aws_cli,
        },
        Rule {
            name: "az_cli",
            description: "Takes the az command the cli suggests",
            priority: 1000,
            matches: |c| {
                is_app(c, &["az"])
                    && c.output.contains("is not in the")
                    && c.output.contains("command group")
            },
            fixes: az_cli,
        },
        Rule {
            name: "cargo",
            description: "Builds when cargo is run without a command",
            priority: 1000,
            matches: |c| c.script == "cargo",
            fixes: |_| vec!["cargo build".to_string()],
        },
        Rule {
            name: "cargo_no_command",
            description: "Takes the cargo command it suggests",
            priority: 1000,
            matches: |c| {
                let output = c.output_lower();
                is_app(c, &["cargo"])
                    && c.parts.len() > 1
                    && (output.contains("no such subcommand") || output.contains("no such command"))
                    && c.output.contains("Did you mean")
            },
            fixes: cargo_no_command,
        },
        Rule {
            name: "composer_not_command",
            description: "Takes the composer command it suggests",
            priority: 1000,
            matches: composer_not_command_matches,
            fixes: composer_not_command,
        },
        Rule {
            name: "conda_mistype",
            description: "Takes the conda command it suggests",
            priority: 1000,
            matches: |c| is_app(c, &["conda"]) && c.output.contains("Did you mean 'conda"),
            fixes: conda_mistype,
        },
        Rule {
            name: "cpp11",
            description: "Adds -std=c++11 when the compiler needs it",
            priority: 1000,
            matches: |c| {
                is_app(c, &["g++", "clang++"])
                    && (c.output.contains(
                        "This file requires compiler and library support for the ISO C++ 2011 standard.",
                    ) || c.output.contains("-Wc++11-extensions"))
            },
            fixes: |c| vec![format!("{} -std=c++11", c.script)],
        },
        Rule {
            name: "django_south_ghost",
            description: "Deletes ghost migrations from the database with --delete-ghost-migrations",
            priority: 1000,
            matches: |c| {
                c.script.contains("manage.py")
                    && c.script.contains("migrate")
                    && c.output.contains("or pass --delete-ghost-migrations")
            },
            fixes: |c| vec![format!("{} --delete-ghost-migrations", c.script)],
        },
        Rule {
            name: "django_south_merge",
            description: "Adds --merge to a South migration that ran out of order",
            priority: 1000,
            matches: |c| {
                c.script.contains("manage.py")
                    && c.script.contains("migrate")
                    && c.output
                        .contains("--merge: will just attempt the migration")
            },
            fixes: |c| vec![format!("{} --merge", c.script)],
        },
        Rule {
            name: "docker_image_being_used_by_container",
            description: "Force-removes the container that uses the image, then removes it",
            priority: 1000,
            matches: |c| {
                is_app(c, &["docker"])
                    && c.output
                        .contains("image is being used by running container")
            },
            fixes: |c| {
                c.output
                    .split_whitespace()
                    .last()
                    .map(|id| {
                        format!(
                            "docker container rm -f {} && {}",
                            shell_words::quote(id),
                            c.script
                        )
                    })
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "docker_login",
            description: "Logs in to docker first",
            priority: 1000,
            matches: |c| {
                is_app(c, &["docker"])
                    && c.script.contains("docker")
                    && c.output.contains("access denied")
                    && c.output.contains("may require 'docker login'")
            },
            fixes: |c| vec![format!("docker login && {}", c.script)],
        },
        Rule {
            name: "docker_not_command",
            description: "Fixes a mistyped docker command",
            priority: 1100,
            matches: docker_not_command_matches,
            fixes: |c| with_sudo(c, docker_not_command),
        },
        Rule {
            name: "fab_command_not_found",
            description: "Fixes a mistyped fabric task",
            priority: 1100,
            matches: |c| is_app(c, &["fab"]) && c.output.contains("Warning: Command(s) not found:"),
            fixes: fab_command_not_found,
        },
        Rule {
            name: "gem_unknown_command",
            description: "Fixes a mistyped gem command",
            priority: 1100,
            matches: |c| {
                is_app(c, &["gem"])
                    && c.output
                        .contains("ERROR:  While executing gem ... (Gem::CommandLineError)")
                    && c.output.contains("Unknown command")
            },
            fixes: |c| gem_unknown_command(c, listing("gem", &["help", "commands"]).as_deref()),
        },
        Rule {
            name: "go_unknown_command",
            description: "Fixes a mistyped go command",
            priority: 1100,
            matches: |c| {
                is_app(c, &["go"]) && c.parts.len() > 1 && c.output.contains("unknown command")
            },
            fixes: |c| go_unknown_command(c, listing("go", &[]).as_deref()),
        },
        Rule {
            name: "gradle_no_task",
            description: "Fixes a mistyped gradle task",
            priority: 1100,
            matches: |c| is_app(c, &["gradle", "gradlew"]) && GRADLE_TASK.is_match(&c.output),
            fixes: gradle_no_task,
        },
        Rule {
            name: "gradle_wrapper",
            description: "Uses ./gradlew when gradle itself is not installed",
            priority: 1000,
            matches: |c| {
                is_app(c, &["gradle"])
                    && gradle_wrapper_applies(c, on_path("gradle"), Path::new("gradlew").is_file())
            },
            fixes: |c| {
                vec![format!(
                    "./gradlew {}",
                    c.parts
                        .iter()
                        .skip(1)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" ")
                )]
            },
        },
        Rule {
            name: "grunt_task_not_found",
            description: "Fixes a mistyped grunt task",
            priority: 1100,
            matches: |c| is_app(c, &["grunt"]) && GRUNT_TASK.is_match(&c.output),
            fixes: |c| grunt_task_not_found(c, listing("grunt", &["--help"]).as_deref()),
        },
        Rule {
            name: "gulp_not_task",
            description: "Fixes a mistyped gulp task",
            priority: 1100,
            matches: |c| is_app(c, &["gulp"]) && c.output.contains("is not in your gulpfile"),
            fixes: |c| gulp_not_task(c, listing("gulp", &["--tasks-simple"]).as_deref()),
        },
        Rule {
            name: "heroku_multiple_apps",
            description: "Names one of the apps in the git remotes with --app",
            priority: 1000,
            matches: |c| {
                is_app(c, &["heroku"])
                    && c.output
                        .contains("https://devcenter.heroku.com/articles/multiple-environments")
            },
            fixes: |c| {
                HEROKU_APP
                    .captures_iter(&c.output)
                    .map(|found| format!("{} --app {}", c.script, &found[1]))
                    .collect()
            },
        },
        Rule {
            name: "heroku_not_command",
            description: "Runs the command heroku says it meant",
            priority: 1000,
            matches: |c| is_app(c, &["heroku"]) && c.output.contains("Run heroku _ to run"),
            fixes: |c| {
                HEROKU_RUN
                    .captures(&c.output)
                    .map(|found| found[1].to_string())
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "lein_not_task",
            description: "Takes the lein task it suggests",
            priority: 1000,
            matches: |c| is_app(c, &["lein"]) && lein_not_task_matches(&unsudo(c).1),
            fixes: |c| with_sudo(c, lein_not_task),
        },
        Rule {
            name: "mvn_no_command",
            description: "Gives mvn a goal to run",
            priority: 1000,
            matches: |c| {
                is_app(c, &["mvn"])
                    && c.output
                        .contains("No goals have been specified for this build")
            },
            fixes: |c| {
                vec![
                    format!("{} clean package", c.script),
                    format!("{} clean install", c.script),
                ]
            },
        },
        Rule {
            name: "mvn_unknown_lifecycle_phase",
            description: "Fixes a mistyped mvn lifecycle phase",
            priority: 1100,
            matches: |c| {
                is_app(c, &["mvn"])
                    && MVN_FAILED_PHASE.is_match(&c.output)
                    && MVN_PHASES.is_match(&c.output)
            },
            fixes: mvn_unknown_lifecycle_phase,
        },
        Rule {
            name: "npm_missing_script",
            description: "Fixes a mistyped script name for npm run",
            priority: 1100,
            matches: |c| {
                is_app(c, &["npm"])
                    && c.parts.iter().any(|part| part.starts_with("ru"))
                    && NPM_MISSING_SCRIPT.is_match(&c.output)
            },
            fixes: |c| npm_missing_script(c, &package_scripts(&read_package_json(Path::new(".")))),
        },
        Rule {
            name: "npm_wrong_command",
            description: "Fixes a mistyped npm command",
            priority: 1100,
            matches: |c| is_app(c, &["npm"]) && npm_wrong_command_matches(&unsudo(c).1),
            fixes: |c| with_sudo(c, npm_wrong_command),
        },
        Rule {
            name: "php_s",
            description: "Uses -S, the built-in server flag, in place of -s",
            priority: 1000,
            matches: |c| {
                is_app(c, &["php"])
                    && c.parts.len() > 2
                    && c.parts.iter().any(|part| part == "-s")
                    && c.parts.last().is_some_and(|last| last != "-s")
            },
            fixes: |c| vec![replace_arg(&c.script, "-s", "-S")],
        },
        Rule {
            name: "pip_unknown_command",
            description: "Takes the pip command it suggests",
            priority: 1000,
            matches: |c| {
                is_app(c, &["pip", "pip2", "pip3"])
                    && c.output.contains("unknown command")
                    && c.output.contains("maybe you meant")
            },
            fixes: pip_unknown_command,
        },
        Rule {
            name: "python_command",
            description: "Runs a .py script with python",
            priority: 1000,
            matches: |c| c.script.contains(".py") && python_command_matches(&unsudo(c).1),
            fixes: |c| {
                let (sudo, inner) = unsudo(c);
                vec![format!("{sudo}python {}", inner.script)]
            },
        },
        Rule {
            name: "python_module_error",
            description: "Installs the missing Python module with pip, then runs the command",
            priority: 1000,
            matches: |c| c.output.contains("ModuleNotFoundError: No module named '"),
            fixes: |c| {
                MISSING_MODULE
                    .captures(&c.output)
                    .map(|found| {
                        format!(
                            "pip install {} && {}",
                            shell_words::quote(&found[1]),
                            c.script
                        )
                    })
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "rails_migrations_pending",
            description: "Runs the pending migrations rails asks for, then the command",
            priority: 1000,
            matches: |c| {
                c.output
                    .contains("Migrations are pending. To resolve this issue, run:")
            },
            fixes: |c| {
                RAILS_MIGRATE
                    .captures(&c.output)
                    .map(|found| format!("{} && {}", &found[1], c.script))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "react_native_command_unrecognized",
            description: "Fixes a mistyped react-native command",
            priority: 1100,
            matches: |c| is_app(c, &["react-native"]) && REACT_NATIVE_UNKNOWN.is_match(&c.output),
            fixes: |c| {
                react_native_command_unrecognized(
                    c,
                    listing("react-native", &["--help"]).as_deref(),
                )
            },
        },
        Rule {
            name: "terraform_init",
            description: "Runs terraform init first",
            priority: 1000,
            matches: |c| {
                let output = c.output_lower();
                is_app(c, &["terraform"])
                    && (output.contains("this module is not yet installed")
                        || output.contains("initialization required"))
            },
            fixes: |c| vec![format!("terraform init && {}", c.script)],
        },
        Rule {
            name: "terraform_no_command",
            description: "Takes the terraform command it suggests",
            priority: 1000,
            matches: |c| {
                is_app(c, &["terraform"])
                    && TERRAFORM_MISTAKE.is_match(&c.output)
                    && TERRAFORM_FIX.is_match(&c.output)
            },
            fixes: terraform_no_command,
        },
        Rule {
            name: "tsuru_login",
            description: "Logs in to tsuru first",
            priority: 1000,
            matches: |c| {
                is_app(c, &["tsuru"])
                    && c.output.contains("not authenticated")
                    && c.output.contains("session has expired")
            },
            fixes: |c| vec![format!("tsuru login && {}", c.script)],
        },
        Rule {
            name: "tsuru_not_command",
            description: "Takes the tsuru command it suggests",
            priority: 1000,
            matches: |c| {
                is_app(c, &["tsuru"])
                    && c.output
                        .contains(" is not a tsuru command. See \"tsuru help\".")
                    && c.output.contains("\nDid you mean?\n\t")
            },
            fixes: tsuru_not_command,
        },
        Rule {
            name: "vagrant_up",
            description: "Starts the machine with vagrant up first",
            priority: 1000,
            matches: |c| is_app(c, &["vagrant"]) && c.output_lower().contains("run `vagrant up`"),
            fixes: vagrant_up,
        },
        Rule {
            name: "workon_doesnt_exists",
            description: "Fixes a mistyped virtualenv name, or creates it with mkvirtualenv",
            priority: 1100,
            matches: |c| is_app(c, &["workon"]) && workon_applies(c, &workon_envs()),
            fixes: |c| workon_fixes(c, &workon_envs()),
        },
        Rule {
            name: "yarn_alias",
            description: "Takes the yarn command it suggests",
            priority: 1000,
            matches: |c| {
                is_app(c, &["yarn"]) && c.parts.len() > 1 && c.output.contains("Did you mean")
            },
            fixes: yarn_alias,
        },
        Rule {
            name: "yarn_command_replaced",
            description: "Runs the command yarn says replaced the old one",
            priority: 1000,
            matches: |c| {
                is_app(c, &["yarn"]) && c.parts.len() > 1 && YARN_REPLACED.is_match(&c.output)
            },
            fixes: |c| {
                YARN_REPLACED
                    .captures(&c.output)
                    .map(|found| found[1].to_string())
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "yarn_help",
            description: "Opens the documentation page yarn help points to",
            priority: 1000,
            matches: |c| {
                is_app(c, &["yarn"])
                    && c.parts.len() > 2
                    && c.parts[1] == "help"
                    && c.output.contains("for documentation about this command.")
            },
            fixes: |c| {
                YARN_DOCS
                    .captures(&c.output)
                    .map(|found| open_command(&found[1], on_path("xdg-open")))
                    .into_iter()
                    .collect()
            },
        },
    ]
}

// helpers shared with more_pkg

/// thefuck's `is_app`: the program, by its base name, is one of `names`. A
/// leading `sudo` is looked through.
pub(super) fn is_app(command: &Command, names: &[&str]) -> bool {
    let program = command.program();
    names.contains(&program.rsplit('/').next().unwrap_or(program))
}

/// Whether `program` is an executable file in a directory on PATH.
pub(super) fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

/// thefuck's `sudo_support`: a rule sees the command without its leading
/// `sudo`, and what it fixes gets the `sudo` back.
pub(super) fn unsudo(command: &Command) -> (&'static str, Command) {
    match command.script.strip_prefix("sudo ") {
        Some(rest) => ("sudo ", Command::new(rest.trim_start(), &command.output)),
        None => ("", command.clone()),
    }
}

pub(super) fn with_sudo(command: &Command, fixes: fn(&Command) -> Vec<String>) -> Vec<String> {
    let (sudo, inner) = unsudo(command);
    fixes(&inner)
        .into_iter()
        .map(|fix| format!("{sudo}{fix}"))
        .collect()
}

/// Python's `SequenceMatcher(None, a, b).ratio()`, so the order of the
/// suggestions is the one thefuck gives. Python tracks "junk" only for
/// strings of 200 or more characters; nothing here is that long.
fn ratio(a: &[char], b: &[char]) -> f64 {
    let total = a.len() + b.len();
    if total == 0 {
        return 1.0;
    }
    let mut matched = 0;
    let mut pending = vec![(0, a.len(), 0, b.len())];
    while let Some((alo, ahi, blo, bhi)) = pending.pop() {
        let (i, j, size) = longest_match(a, b, alo, ahi, blo, bhi);
        if size > 0 {
            matched += size;
            if alo < i && blo < j {
                pending.push((alo, i, blo, j));
            }
            if i + size < ahi && j + size < bhi {
                pending.push((i + size, ahi, j + size, bhi));
            }
        }
    }
    2.0 * matched as f64 / total as f64
}

/// The longest run that `a[alo..ahi]` and `b[blo..bhi]` share: where it
/// starts in each and how long it is, the earliest one when several tie.
fn longest_match(
    a: &[char],
    b: &[char],
    alo: usize,
    ahi: usize,
    blo: usize,
    bhi: usize,
) -> (usize, usize, usize) {
    let (mut best_i, mut best_j, mut best_size) = (alo, blo, 0);
    let mut lengths: HashMap<usize, usize> = HashMap::new();
    for (i, ai) in a.iter().enumerate().take(ahi).skip(alo) {
        let mut next = HashMap::new();
        for (j, bj) in b.iter().enumerate().take(bhi).skip(blo) {
            if ai != bj {
                continue;
            }
            let before = if j > 0 {
                lengths.get(&(j - 1)).copied().unwrap_or(0)
            } else {
                0
            };
            let size = before + 1;
            next.insert(j, size);
            if size > best_size {
                best_i = i + 1 - size;
                best_j = j + 1 - size;
                best_size = size;
            }
        }
        lengths = next;
    }
    (best_i, best_j, best_size)
}

/// Python's `difflib.get_close_matches`: up to `n` of the `possibilities`
/// that look like `word` (at least `cutoff` out of 1), best first.
fn close_matches<S: AsRef<str>>(
    word: &str,
    possibilities: &[S],
    n: usize,
    cutoff: f64,
) -> Vec<String> {
    let word: Vec<char> = word.chars().collect();
    let mut scored: Vec<(f64, &str)> = possibilities
        .iter()
        .map(AsRef::as_ref)
        .filter_map(|candidate| {
            let chars: Vec<char> = candidate.chars().collect();
            let score = ratio(&chars, &word);
            (score >= cutoff).then_some((score, candidate))
        })
        .collect();
    // Python picks the largest (score, name) pairs.
    scored.sort_by(|x, y| y.0.total_cmp(&x.0).then_with(|| y.1.cmp(x.1)));
    scored
        .into_iter()
        .take(n)
        .map(|(_, name)| name.to_string())
        .collect()
}

/// thefuck's `get_closest`. Unlike thefuck it does not fall back to the
/// first possibility when nothing is close: that offered fixes like
/// `adb backup` for `adb xyzzy`.
fn closest<S: AsRef<str>>(word: &str, possibilities: &[S]) -> Option<String> {
    close_matches(word, possibilities, 1, 0.6)
        .into_iter()
        .next()
}

/// thefuck's `replace_command`: the script once per name that looks like
/// `broken`, with `broken` swapped for it. thefuck shows three.
pub(super) fn replace_command<S: AsRef<str>>(
    script: &str,
    broken: &str,
    matched: &[S],
) -> Vec<String> {
    close_matches(broken, matched, 3, 0.1)
        .iter()
        .map(|new| replace_arg(script, broken, new.trim()))
        .collect()
}

/// thefuck's `get_all_matched_commands`: the non-empty lines after one that
/// holds a separator.
fn lines_after(output: &str, separators: &[&str]) -> Vec<String> {
    let mut found = false;
    let mut lines = Vec::new();
    for line in output.split('\n') {
        if separators.iter().any(|separator| line.contains(separator)) {
            found = true;
        } else if found && !line.trim().is_empty() {
            lines.push(line.trim().to_string());
        }
    }
    lines
}

fn first_word(line: &str) -> Option<String> {
    line.split(' ')
        .next()
        .filter(|word| !word.is_empty())
        .map(String::from)
}

// adb

const ADB_COMMANDS: &[&str] = &[
    "backup",
    "bugreport",
    "connect",
    "devices",
    "disable-verity",
    "disconnect",
    "enable-verity",
    "emu",
    "forward",
    "get-devpath",
    "get-serialno",
    "get-state",
    "install",
    "install-multiple",
    "jdwp",
    "keygen",
    "kill-server",
    "logcat",
    "pull",
    "push",
    "reboot",
    "reconnect",
    "restore",
    "reverse",
    "root",
    "run-as",
    "shell",
    "sideload",
    "start-server",
    "sync",
    "tcpip",
    "uninstall",
    "unroot",
    "usb",
    "wait-for",
];

fn adb_unknown_command(command: &Command) -> Vec<String> {
    for (index, arg) in command.parts.iter().enumerate().skip(1) {
        // -s, -H, -P and -L take a value, which is not the command
        let takes_value = matches!(command.parts[index - 1].as_str(), "-s" | "-H" | "-P" | "-L");
        if !arg.starts_with('-') && !takes_value {
            return closest(arg, ADB_COMMANDS)
                .map(|right| replace_arg(&command.script, arg, &right))
                .into_iter()
                .collect();
        }
    }
    Vec::new()
}

// aws and az

static AWS_INVALID_CHOICE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Invalid choice: '(.*)', maybe you meant:").unwrap());
static AWS_OPTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\s*\*\s(.*)").unwrap());

fn aws_cli(command: &Command) -> Vec<String> {
    let Some(mistake) = AWS_INVALID_CHOICE.captures(&command.output) else {
        return Vec::new();
    };
    AWS_OPTION
        .captures_iter(&command.output)
        .map(|option| replace_arg(&command.script, &mistake[1], option[1].trim()))
        .collect()
}

static AZ_INVALID_CHOICE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"az.*: '(.*)' is not in the '.*' command group\.").unwrap());
static AZ_OPTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^The most similar choice to '.*' is:\n\s*(.*)$").unwrap());

fn az_cli(command: &Command) -> Vec<String> {
    let Some(mistake) = AZ_INVALID_CHOICE.captures(&command.output) else {
        return Vec::new();
    };
    AZ_OPTION
        .captures_iter(&command.output)
        .map(|option| replace_arg(&command.script, &mistake[1], option[1].trim()))
        .collect()
}

// cargo, composer and conda

static CARGO_SUGGESTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Did you mean `([^`]*)`").unwrap());

fn cargo_no_command(command: &Command) -> Vec<String> {
    let (Some(broken), Some(found)) = (
        command.parts.get(1),
        CARGO_SUGGESTION.captures(&command.output),
    ) else {
        return Vec::new();
    };
    vec![replace_arg(&command.script, broken, &found[1])]
}

static COMPOSER_UNDEFINED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"Command "([^"]*)" is not defined"#).unwrap());
static COMPOSER_THIS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Did you mean this\?[^\n]*\n\s*([^\n]*)").unwrap());
static COMPOSER_ONE_OF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Did you mean one of these\?[^\n]*\n\s*([^\n]*)").unwrap());

/// composer says to use `require` when `install` was given a package.
fn composer_wants_require(command: &Command) -> bool {
    command.parts.iter().any(|part| part == "install")
        && command.output_lower().contains("composer require")
}

fn composer_not_command_matches(command: &Command) -> bool {
    let output = command.output_lower();
    is_app(command, &["composer"])
        && (output.contains("did you mean this?")
            || output.contains("did you mean one of these?")
            || composer_wants_require(command))
}

fn composer_not_command(command: &Command) -> Vec<String> {
    if composer_wants_require(command) {
        return vec![replace_arg(&command.script, "install", "require")];
    }
    let Some(broken) = COMPOSER_UNDEFINED.captures(&command.output) else {
        return Vec::new();
    };
    let suggestion = COMPOSER_THIS
        .captures(&command.output)
        .or_else(|| COMPOSER_ONE_OF.captures(&command.output));
    suggestion
        .map(|found| replace_arg(&command.script, &broken[1], found[1].trim()))
        .into_iter()
        .collect()
}

static CONDA_COMMAND: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"'conda ([^']*)'").unwrap());

fn conda_mistype(command: &Command) -> Vec<String> {
    let names: Vec<String> = CONDA_COMMAND
        .captures_iter(&command.output)
        .map(|found| found[1].to_string())
        .collect();
    match names.as_slice() {
        [broken, correct, ..] => {
            replace_command(&command.script, broken, std::slice::from_ref(correct))
        }
        _ => Vec::new(),
    }
}

// docker

static DOCKER_UNKNOWN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"docker: '(\w+)' is not a docker command.").unwrap());

fn docker_not_command_matches(command: &Command) -> bool {
    is_app(command, &["docker"])
        && (command.output.contains("is not a docker command")
            || command.output.contains("Usage:\tdocker"))
}

/// The names listed under a heading such as `Commands:` in docker's help.
fn docker_commands_under(lines: &[&str], heading: &str) -> Vec<String> {
    lines
        .iter()
        .skip_while(|line| !line.starts_with(heading))
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| first_word(line.trim()))
        .collect()
}

/// Every command in `docker`'s own help: management commands first.
fn docker_commands(help: &str) -> Vec<String> {
    let lines: Vec<&str> = help.lines().collect();
    let mut commands = if lines.contains(&"Management Commands:") {
        docker_commands_under(&lines, "Management Commands:")
    } else {
        Vec::new()
    };
    commands.extend(docker_commands_under(&lines, "Commands:"));
    commands
}

fn docker_not_command(command: &Command) -> Vec<String> {
    docker_not_command_with(command, || listing("docker", &[]))
}

/// `help` is asked for only when the command needs docker's own list.
fn docker_not_command_with(
    command: &Command,
    help: impl FnOnce() -> Option<String>,
) -> Vec<String> {
    // `docker image la`: docker printed the help of `docker image`
    if command.output.contains("Usage:") && command.parts.len() > 1 {
        let lines: Vec<&str> = command.output.split('\n').collect();
        let subcommands = docker_commands_under(&lines, "Commands:");
        return match command.parts.get(2) {
            Some(wrong) => replace_command(&command.script, wrong, &subcommands),
            None => Vec::new(),
        };
    }
    let Some(wrong) = DOCKER_UNKNOWN.captures(&command.output) else {
        return Vec::new();
    };
    let help = help().unwrap_or_default();
    replace_command(&command.script, &wrong[1], &docker_commands(&help))
}

// fab

/// Names in fab's output after the line holding `start`, up to the line
/// holding `end` if there is one.
fn fab_between(output: &str, start: &str, end: Option<&str>) -> Vec<String> {
    let mut found = false;
    let mut names = Vec::new();
    for line in output.split('\n') {
        if line.contains(start) {
            found = true;
            continue;
        }
        if end.is_some_and(|end| line.contains(end)) {
            break;
        }
        if found && !line.trim().is_empty() {
            names.extend(first_word(line.trim()));
        }
    }
    names
}

fn fab_command_not_found(command: &Command) -> Vec<String> {
    let missing = fab_between(
        &command.output,
        "Warning: Command(s) not found:",
        Some("Available commands:"),
    );
    let available = fab_between(&command.output, "Available commands:", None);
    let mut script = command.script.clone();
    for name in missing {
        if let Some(right) = closest(&name, &available) {
            script = script.replace(&format!(" {name}"), &format!(" {right}"));
        }
    }
    vec![script]
}

// gem and go

static GEM_UNKNOWN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)Unknown command (.*)$").unwrap());

/// The commands in `gem help commands`: indented four spaces, not more.
fn gem_commands(help: &str) -> Vec<String> {
    help.lines()
        .filter_map(|line| line.strip_prefix("    "))
        .filter(|rest| !rest.starts_with(' '))
        .filter_map(first_word)
        .collect()
}

fn gem_unknown_command(command: &Command, help: Option<&str>) -> Vec<String> {
    let (Some(unknown), Some(help)) = (GEM_UNKNOWN.captures(&command.output), help) else {
        return Vec::new();
    };
    replace_command(&command.script, unknown[1].trim(), &gem_commands(help))
}

/// The commands `go` prints under "The commands are:".
fn go_commands(help: &str) -> Vec<String> {
    help.lines()
        .map(str::trim)
        .skip_while(|line| *line != "The commands are:")
        .skip(2)
        .take_while(|line| !line.is_empty())
        .filter_map(first_word)
        .collect()
}

fn go_unknown_command(command: &Command, help: Option<&str>) -> Vec<String> {
    let (Some(wrong), Some(help)) = (command.parts.get(1), help) else {
        return Vec::new();
    };
    closest(wrong, &go_commands(help))
        .map(|right| replace_arg(&command.script, wrong, &right))
        .into_iter()
        .collect()
}

// gradle

static GRADLE_TASK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Task '(.*)' (is ambiguous|not found)").unwrap());

/// The tasks in `gradle tasks`: the lines under a `----` rule.
fn gradle_tasks(help: &str) -> Vec<String> {
    let mut listed = false;
    let mut tasks = Vec::new();
    for line in help.lines().map(str::trim) {
        if line.starts_with("----") {
            listed = true;
        } else if line.is_empty() {
            listed = false;
        } else if listed
            && !line.starts_with("All tasks runnable from root project")
            && !line.starts_with("Tasks runnable from root project")
        {
            tasks.extend(first_word(line));
        }
    }
    tasks
}

fn gradle_no_task(command: &Command) -> Vec<String> {
    let Some(wrong) = GRADLE_TASK.captures(&command.output) else {
        return Vec::new();
    };
    let program = command.program();
    // listing only finds programs on PATH or given as an absolute path
    let program = if program.contains('/') && !Path::new(program).is_absolute() {
        std::env::current_dir()
            .map(|dir| dir.join(program).to_string_lossy().into_owned())
            .unwrap_or_else(|_| program.to_string())
    } else {
        program.to_string()
    };
    let Some(help) = listing(&program, &["tasks"]) else {
        return Vec::new();
    };
    replace_command(&command.script, &wrong[1], &gradle_tasks(&help))
}

fn gradle_wrapper_applies(command: &Command, gradle_installed: bool, has_wrapper: bool) -> bool {
    is_app(command, &["gradle"])
        && !gradle_installed
        && command.output.contains("not found")
        && has_wrapper
}

// grunt and gulp

static GRUNT_TASK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"Warning: Task "(.*)" not found."#).unwrap());

/// The tasks under "Available tasks" in `grunt --help`.
fn grunt_tasks(help: &str) -> Vec<String> {
    let mut listed = false;
    let mut tasks = Vec::new();
    for line in help.lines().map(str::trim) {
        if line.contains("Available tasks") {
            listed = true;
        } else if listed && line.is_empty() {
            break;
        } else if listed && line.contains("  ") {
            tasks.extend(first_word(line));
        }
    }
    tasks
}

fn grunt_task_not_found(command: &Command, help: Option<&str>) -> Vec<String> {
    let (Some(task), Some(help)) = (GRUNT_TASK.captures(&command.output), help) else {
        return Vec::new();
    };
    // `grunt cmpass:all`: the task is the part before the colon
    let wrong = task[1].split(':').next().unwrap_or_default();
    closest(wrong, &grunt_tasks(help))
        .map(|right| {
            command
                .script
                .replace(&format!(" {wrong}"), &format!(" {right}"))
        })
        .into_iter()
        .collect()
}

static GULP_TASK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Task '(\w+)' is not in your gulpfile").unwrap());

fn gulp_not_task(command: &Command, tasks: Option<&str>) -> Vec<String> {
    let (Some(wrong), Some(tasks)) = (GULP_TASK.captures(&command.output), tasks) else {
        return Vec::new();
    };
    let tasks: Vec<&str> = tasks
        .lines()
        .map(str::trim)
        .filter(|task| !task.is_empty())
        .collect();
    replace_command(&command.script, &wrong[1], &tasks)
}

// heroku

static HEROKU_APP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([^\s]+) \([^)]*\)").unwrap());
static HEROKU_RUN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Run heroku _ to run ([^.]*)").unwrap());

// lein

static LEIN_NOT_TASK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"'([^']*)' is not a task").unwrap());

fn lein_not_task_matches(command: &Command) -> bool {
    is_app(command, &["lein"])
        && command.script.starts_with("lein")
        && command.output.contains("is not a task. See 'lein help'")
        && command.output.contains("Did you mean this?")
}

fn lein_not_task(command: &Command) -> Vec<String> {
    let Some(broken) = LEIN_NOT_TASK.captures(&command.output) else {
        return Vec::new();
    };
    replace_command(
        &command.script,
        &broken[1],
        &lines_after(&command.output, &["Did you mean this?"]),
    )
}

// mvn

static MVN_FAILED_PHASE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\[ERROR\] Unknown lifecycle phase "([^"]+)""#).unwrap());
static MVN_PHASES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Available lifecycle phases are: (.+) -> \[Help 1\]").unwrap());

fn mvn_unknown_lifecycle_phase(command: &Command) -> Vec<String> {
    let (Some(failed), Some(available)) = (
        MVN_FAILED_PHASE.captures(&command.output),
        MVN_PHASES.captures(&command.output),
    ) else {
        return Vec::new();
    };
    let phases: Vec<&str> = available[1].split(", ").collect();
    let selected = close_matches(&failed[1], &phases, 3, 0.6);
    replace_command(&command.script, &failed[1], &selected)
}

// npm

static NPM_MISSING_SCRIPT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)npm (?:ERR!|error) missing script: "?([^"\n]*)"?"#).unwrap()
});

fn npm_missing_script(command: &Command, scripts: &[String]) -> Vec<String> {
    NPM_MISSING_SCRIPT
        .captures(&command.output)
        .map(|missing| replace_command(&command.script, missing[1].trim(), scripts))
        .unwrap_or_default()
}

fn read_package_json(dir: &Path) -> String {
    fs::read_to_string(dir.join("package.json")).unwrap_or_default()
}

static SCRIPTS_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""scripts"\s*:\s*\{"#).unwrap());

/// The names under `"scripts"` in a package.json, which is where `npm run`
/// lists them from. A scripts block is only string pairs, so a small
/// scanner does.
fn package_scripts(json: &str) -> Vec<String> {
    let mut names = Vec::new();
    let Some(start) = SCRIPTS_KEY.find(json) else {
        return names;
    };
    let mut chars = json[start.end()..].chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace() || *c == ',').is_some() {}
        let Some(name) = json_string(&mut chars) else {
            break;
        };
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.next() != Some(':') {
            break;
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if json_string(&mut chars).is_none() {
            break;
        }
        names.push(name);
    }
    names
}

fn json_string(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
    chars.next_if_eq(&'"')?;
    let mut text = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(text),
            '\\' => text.push(chars.next()?),
            c => text.push(c),
        }
    }
}

/// The command npm was given: the first word that is not a flag.
fn npm_wrong_word(command: &Command) -> Option<&String> {
    command
        .parts
        .iter()
        .skip(1)
        .find(|part| !part.starts_with('-'))
}

fn npm_wrong_command_matches(command: &Command) -> bool {
    is_app(command, &["npm"])
        && command.parts.first().is_some_and(|first| first == "npm")
        && command.output.contains("where <command> is one of:")
        && npm_wrong_word(command).is_some()
}

/// The commands npm lists after "where <command> is one of:".
fn npm_commands(output: &str) -> Vec<String> {
    output
        .split('\n')
        .skip_while(|line| !line.starts_with("where <command> is one of:"))
        .skip(1)
        .take_while(|line| !line.is_empty())
        .flat_map(|line| line.split(", "))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(String::from)
        .collect()
}

fn npm_wrong_command(command: &Command) -> Vec<String> {
    let Some(wrong) = npm_wrong_word(command) else {
        return Vec::new();
    };
    closest(wrong, &npm_commands(&command.output))
        .map(|right| replace_arg(&command.script, wrong, &right))
        .into_iter()
        .collect()
}

// pip and python

static PIP_BROKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"ERROR: unknown command "([^"]+)""#).unwrap());
static PIP_SUGGESTED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"maybe you meant "([^"]+)""#).unwrap());

fn pip_unknown_command(command: &Command) -> Vec<String> {
    match (
        PIP_BROKEN.captures(&command.output),
        PIP_SUGGESTED.captures(&command.output),
    ) {
        (Some(broken), Some(suggested)) => {
            vec![replace_arg(&command.script, &broken[1], &suggested[1])]
        }
        _ => Vec::new(),
    }
}

fn python_command_matches(command: &Command) -> bool {
    command
        .parts
        .first()
        .is_some_and(|first| first.ends_with(".py"))
        && (command.output.contains("Permission denied")
            || command.output.contains("command not found"))
}

static MISSING_MODULE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"ModuleNotFoundError: No module named '([^']+)'").unwrap());

// rails and react-native

static RAILS_MIGRATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"To resolve this issue, run:\s+(.*?)\n").unwrap());

static REACT_NATIVE_UNKNOWN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Unrecognized command '(.*)'").unwrap());

/// The commands under "Commands:" in `react-native --help`.
fn react_native_commands(help: &str) -> Vec<String> {
    help.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .skip_while(|line| !line.contains("Commands:"))
        .skip(1)
        .filter_map(first_word)
        .collect()
}

fn react_native_command_unrecognized(command: &Command, help: Option<&str>) -> Vec<String> {
    let (Some(wrong), Some(help)) = (REACT_NATIVE_UNKNOWN.captures(&command.output), help) else {
        return Vec::new();
    };
    replace_command(&command.script, &wrong[1], &react_native_commands(help))
}

// terraform

static TERRAFORM_MISTAKE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"Terraform has no command named "([^"]+)"\."#).unwrap());
static TERRAFORM_FIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"Did you mean "([^"]+)"\?"#).unwrap());

fn terraform_no_command(command: &Command) -> Vec<String> {
    match (
        TERRAFORM_MISTAKE.captures(&command.output),
        TERRAFORM_FIX.captures(&command.output),
    ) {
        (Some(mistake), Some(fix)) => vec![replace_arg(&command.script, &mistake[1], &fix[1])],
        _ => Vec::new(),
    }
}

// tsuru

static TSURU_BROKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"tsuru: "([^"]*)" is not a tsuru command"#).unwrap());

fn tsuru_not_command(command: &Command) -> Vec<String> {
    let Some(broken) = TSURU_BROKEN.captures(&command.output) else {
        return Vec::new();
    };
    replace_command(
        &command.script,
        &broken[1],
        &lines_after(&command.output, &["Did you mean"]),
    )
}

// vagrant and workon

fn vagrant_up(command: &Command) -> Vec<String> {
    let everything = format!("vagrant up && {}", command.script);
    match command.parts.get(2) {
        Some(machine) => vec![
            format!(
                "vagrant up {} && {}",
                shell_words::quote(machine),
                command.script
            ),
            everything,
        ],
        None => vec![everything],
    }
}

/// The virtualenvs in a directory such as ~/.virtualenvs.
fn virtualenvs(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

fn workon_envs() -> Vec<String> {
    dirs::home_dir()
        .map(|home| virtualenvs(&home.join(".virtualenvs")))
        .unwrap_or_default()
}

fn workon_applies(command: &Command, envs: &[String]) -> bool {
    is_app(command, &["workon"])
        && command
            .parts
            .get(1)
            .is_some_and(|name| !envs.contains(name))
}

fn workon_fixes(command: &Command, envs: &[String]) -> Vec<String> {
    let Some(misspelled) = command.parts.get(1) else {
        return Vec::new();
    };
    let mut fixes = replace_command(&command.script, misspelled, envs);
    fixes.push(format!("mkvirtualenv {}", shell_words::quote(misspelled)));
    fixes
}

// yarn

static YARN_SUGGESTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"Did you mean [`"](?:yarn )?([^`"]*)[`"]"#).unwrap());
static YARN_REPLACED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"Run "(.*)" instead"#).unwrap());
static YARN_DOCS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Visit ([^ ]*) for documentation about this command\.").unwrap());

fn yarn_alias(command: &Command) -> Vec<String> {
    let (Some(broken), Some(fix)) = (
        command.parts.get(1),
        YARN_SUGGESTION.captures(&command.output),
    ) else {
        return Vec::new();
    };
    vec![replace_arg(&command.script, broken, &fix[1])]
}

/// thefuck's `open_command`: xdg-open where there is one, `open` (macOS) otherwise.
fn open_command(url: &str, has_xdg_open: bool) -> String {
    let opener = if has_xdg_open { "xdg-open" } else { "open" };
    format!("{opener} {}", shell_words::quote(url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{all, rule_matches};

    const DOCKER_SWARM_HELP: &str = r#"
Usage:	docker swarm COMMAND

Manage Swarm

Commands:
  ca          Display and rotate the root CA
  init        Initialize a swarm
  join        Join a swarm as a node and/or manager
  join-token  Manage join tokens
  leave       Leave the swarm
  unlock      Unlock swarm
  unlock-key  Manage the unlock key
  update      Update the swarm

Run 'docker swarm COMMAND --help' for more information on a command.
"#;
    const DOCKER_IMAGE_HELP: &str = r#"
Usage:	docker image COMMAND

Manage images

Commands:
  build       Build an image from a Dockerfile
  history     Show the history of an image
  import      Import the contents from a tarball to create a filesystem image
  inspect     Display detailed information on one or more images
  load        Load an image from a tar archive or STDIN
  ls          List images
  prune       Remove unused images
  pull        Pull an image or a repository from a registry
  push        Push an image or a repository to a registry
  rm          Remove one or more images
  save        Save one or more images to a tar archive (streamed to STDOUT by default)
  tag         Create a tag TARGET_IMAGE that refers to SOURCE_IMAGE

Run 'docker image COMMAND --help' for more information on a command.
"#;
    const DOCKER_HELP: &str = r#"
Usage:	docker [OPTIONS] COMMAND

A self-sufficient runtime for containers

Options:
      --config string      Location of client config files (default "/Users/ik1ne/.docker")
  -c, --context string     Name of the context to use to connect to the daemon (overrides DOCKER_HOST env var
                           and default context set with "docker context use")
  -D, --debug              Enable debug mode
  -H, --host list          Daemon socket(s) to connect to
  -l, --log-level string   Set the logging level ("debug"|"info"|"warn"|"error"|"fatal") (default "info")
      --tls                Use TLS; implied by --tlsverify
      --tlscacert string   Trust certs signed only by this CA (default "/Users/ik1ne/.docker/ca.pem")
      --tlscert string     Path to TLS certificate file (default "/Users/ik1ne/.docker/cert.pem")
      --tlskey string      Path to TLS key file (default "/Users/ik1ne/.docker/key.pem")
      --tlsverify          Use TLS and verify the remote
  -v, --version            Print version information and quit

Management Commands:
  builder     Manage builds
  config      Manage Docker configs
  container   Manage containers
  context     Manage contexts
  image       Manage images
  network     Manage networks
  node        Manage Swarm nodes
  plugin      Manage plugins
  secret      Manage Docker secrets
  service     Manage services
  stack       Manage Docker stacks
  swarm       Manage Swarm
  system      Manage Docker
  trust       Manage trust on Docker images
  volume      Manage volumes

Commands:
  attach      Attach local standard input, output, and error streams to a running container
  build       Build an image from a Dockerfile
  commit      Create a new image from a container's changes
  cp          Copy files/folders between a container and the local filesystem
  create      Create a new container
  diff        Inspect changes to files or directories on a container's filesystem
  events      Get real time events from the server
  exec        Run a command in a running container
  export      Export a container's filesystem as a tar archive
  history     Show the history of an image
  images      List images
  import      Import the contents from a tarball to create a filesystem image
  info        Display system-wide information
  inspect     Return low-level information on Docker objects
  kill        Kill one or more running containers
  load        Load an image from a tar archive or STDIN
  login       Log in to a Docker registry
  logout      Log out from a Docker registry
  logs        Fetch the logs of a container
  pause       Pause all processes within one or more containers
  port        List port mappings or a specific mapping for the container
  ps          List containers
  pull        Pull an image or a repository from a registry
  push        Push an image or a repository to a registry
  rename      Rename a container
  restart     Restart one or more containers
  rm          Remove one or more containers
  rmi         Remove one or more images
  run         Run a command in a new container
  save        Save one or more images to a tar archive (streamed to STDOUT by default)
  search      Search the Docker Hub for images
  start       Start one or more stopped containers
  stats       Display a live stream of container(s) resource usage statistics
  stop        Stop one or more running containers
  tag         Create a tag TARGET_IMAGE that refers to SOURCE_IMAGE
  top         Display the running processes of a container
  unpause     Unpause all processes within one or more containers
  update      Update configuration of one or more containers
  version     Show the Docker version information
  wait        Block until one or more containers stop, then print their exit codes

Run 'docker COMMAND --help' for more information on a command.
"#;
    const GEM_HELP: &str = r#"
GEM commands are:

    build             Build a gem from a gemspec
    cert              Manage RubyGems certificates and signing settings
    check             Check a gem repository for added or missing files
    cleanup           Clean up old versions of installed gems
    contents          Display the contents of the installed gems
    dependency        Show the dependencies of an installed gem
    environment       Display information about the RubyGems environment
    fetch             Download a gem and place it in the current directory
    generate_index    Generates the index files for a gem server directory
    help              Provide help on the 'gem' command
    install           Install a gem into the local repository
    list              Display local gems whose name matches REGEXP
    lock              Generate a lockdown list of gems
    mirror            Mirror all gem files (requires rubygems-mirror)
    open              Open gem sources in editor
    outdated          Display all gems that need updates
    owner             Manage gem owners of a gem on the push server
    pristine          Restores installed gems to pristine condition from files
                      located in the gem cache
    push              Push a gem up to the gem server
    query             Query gem information in local or remote repositories
    rdoc              Generates RDoc for pre-installed gems
    search            Display remote gems whose name matches REGEXP
    server            Documentation and gem repository HTTP server
    sources           Manage the sources and cache file RubyGems uses to search
                      for gems
    specification     Display gem specification (in yaml)
    stale             List gems along with access times
    uninstall         Uninstall gems from the local repository
    unpack            Unpack an installed gem to the current directory
    update            Update installed gems to the latest version
    which             Find the location of a library file you can require
    yank              Remove a pushed gem from the index

For help on a particular command, use 'gem help COMMAND'.

Commands may be abbreviated, so long as they are unambiguous.
e.g. 'gem i rake' is short for 'gem install rake'.

"#;
    const GO_HELP: &str = r#"Go is a tool for managing Go source code.

Usage:

	go <command> [arguments]

The commands are:

	bug         start a bug report
	build       compile packages and dependencies
	clean       remove object files and cached files
	doc         show documentation for package or symbol
	env         print Go environment information
	fix         update packages to use new APIs
	fmt         gofmt (reformat) package sources
	generate    generate Go files by processing source
	get         add dependencies to current module and install them
	install     compile and install packages and dependencies
	list        list packages or modules
	mod         module maintenance
	run         compile and run Go program
	test        test packages
	tool        run specified go tool
	version     print Go version
	vet         report likely mistakes in packages

Use "go help <command>" for more information about a command.

Additional help topics:

	buildconstraint build constraints
	buildmode       build modes
	c               calling between Go and C
	cache           build and test caching
	environment     environment variables
	filetype        file types
	go.mod          the go.mod file
	gopath          GOPATH environment variable
	gopath-get      legacy GOPATH go get
	goproxy         module proxy protocol
	importpath      import path syntax
	modules         modules, module versions, and more
	module-get      module-aware go get
	module-auth     module authentication using go.sum
	module-private  module configuration for non-public modules
	packages        package lists and patterns
	testflag        testing flags
	testfunc        testing functions

Use "go help <topic>" for more information about that topic.

"#;
    const GRUNT_HELP: &str = r#"
Grunt: The JavaScript Task Runner (v0.4.5)

Usage
 grunt [options] [task [task ...]]

Options
    --help, -h  Display this help text.
        --base  Specify an alternate base path. By default, all file paths are
                relative to the Gruntfile. (grunt.file.setBase) *
    --no-color  Disable colored output.
   --gruntfile  Specify an alternate Gruntfile. By default, grunt looks in the
                current or parent directories for the nearest Gruntfile.js or
                Gruntfile.coffee file.
   --debug, -d  Enable debugging mode for tasks that support it.
       --stack  Print a stack trace when exiting with a warning or fatal error.
   --force, -f  A way to force your way past warnings. Want a suggestion? Don't
                use this option, fix your code.
       --tasks  Additional directory paths to scan for task and "extra" files.
                (grunt.loadTasks) *
         --npm  Npm-installed grunt plugins to scan for task and "extra" files.
                (grunt.loadNpmTasks) *
    --no-write  Disable writing files (dry run).
 --verbose, -v  Verbose mode. A lot more information output.
 --version, -V  Print the grunt version. Combine with --verbose for more info.
  --completion  Output shell auto-completion rules. See the grunt-cli
                documentation for more information.

Options marked with * have methods exposed via the grunt API and should instead
be specified inside the Gruntfile wherever possible.

Available tasks
  autoprefixer  Prefix CSS files. *
    concurrent  Run grunt tasks concurrently *
         clean  Clean files and folders. *
       compass  Compile Sass to CSS using Compass *
        concat  Concatenate files. *
       connect  Start a connect web server. *
          copy  Copy files. *
        cssmin  Minify CSS *
       htmlmin  Minify HTML *
      imagemin  Minify PNG, JPEG, GIF and SVG images *
        jshint  Validate files with JSHint. *
        uglify  Minify files with UglifyJS. *
         watch  Run predefined tasks whenever watched files change.
       filerev  File revisioning based on content hashing *
        cdnify  Replace scripts with refs to the Google CDN *
         karma  run karma. *
         newer  Run a task with only those source files that have been modified
                since the last successful run.
     any-newer  DEPRECATED TASK.  Use the "newer" task instead
 newer-postrun  Internal task.
   newer-clean  Remove cached timestamps.
    ngAnnotate  Add, remove and rebuild AngularJS dependency injection
                annotations *
    ngconstant  Dynamic angular constant generator task. *
        svgmin  Minify SVG *
        usemin  Replaces references to non-minified scripts / stylesheets *
 useminPrepare  Using HTML markup as the primary source of information *
       wiredep  Inject Bower components into your source code. *
         serve  Compile then start a connect web server
        server  DEPRECATED TASK. Use the "serve" task instead
          test  Alias for "clean:server", "ngconstant:test", "wiredep",
                "concurrent:test", "autoprefixer", "connect:test", "karma"
                tasks.
         build  Alias for "ngconstant:production", "clean:dist", "wiredep",
                "useminPrepare", "concurrent:dist", "autoprefixer", "concat",
                "ngAnnotate", "copy:dist", "cdnify", "cssmin", "uglify",
                "filerev", "usemin", "htmlmin" tasks.
       default  Alias for "newer:jshint", "test", "build" tasks.

Tasks run in the order specified. Arguments may be passed to tasks that accept
them by using colons, like "lint:files". Tasks marked with * are "multi tasks"
and will iterate over all sub-targets if no argument is specified.

The list of available tasks may change based on tasks directories or grunt
plugins specified in the Gruntfile or via command-line options.

For more information, see http://gruntjs.com/
"#;
    const REACT_NATIVE_HELP: &str = r#"
Scanning 615 folders for symlinks in /home/nvbn/work/zcho/BookkaWebView/node_modules (6ms)

  Usage: react-native [options] [command]


  Options:

    -V, --version  output the version number
    -h, --help     output usage information


  Commands:

    start [options]                    starts the webserver
    run-ios [options]                  builds your app and starts it on iOS simulator
    run-android [options]              builds your app and starts it on a connected Android emulator or device
    new-library [options]              generates a native library bridge
    bundle [options]                   builds the javascript bundle for offline use
    unbundle [options]                 builds javascript as "unbundle" for offline use
    eject [options]                    Re-create the iOS and Android folders and native code
    link [options] [packageName]       links all native dependencies (updates native build files)
    unlink [options] <packageName>     unlink native dependency
    install [options] <packageName>    install and link native dependencies
    uninstall [options] <packageName>  uninstall and unlink native dependencies
    upgrade [options]                  upgrade your app's template files to the latest version; run this after updating the react-native version in your package.json and running npm install
    log-android [options]              starts adb logcat
    log-ios [options]                  starts iOS device syslog tail
"#;
    const NPM_USAGE: &str = r#"
Usage: npm <command>

where <command> is one of:
    access, add-user, adduser, apihelp, author, bin, bugs, c,
    cache, completion, config, ddp, dedupe, deprecate, dist-tag,
    dist-tags, docs, edit, explore, faq, find, find-dupes, get,
    help, help-search, home, i, info, init, install, issues, la,
    link, list, ll, ln, login, logout, ls, outdated, owner,
    pack, ping, prefix, prune, publish, r, rb, rebuild, remove,
    repo, restart, rm, root, run-script, s, se, search, set,
    show, shrinkwrap, star, stars, start, stop, t, tag, team,
    test, tst, un, uninstall, unlink, unpublish, unstar, up,
    update, upgrade, v, verison, version, view, whoami

npm <cmd> -h     quick help on <cmd>
npm -l           display full usage info
npm faq          commonly asked questions
npm help <term>  search for help on <term>
npm help npm     involved overview

Specify configs in the ini-formatted file:
    /home/nvbn/.npmrc
or on the command line via: npm <command> --key value
Config info can be viewed via: npm help config

npm@2.14.7 /opt/node/lib/node_modules/npm
"#;

    /// What the named rule alone offers: nothing unless it matches. Only for
    /// rules whose fixes don't run a program; the others are tested through
    /// the function that takes the program's output.
    fn fixes_of(name: &str, script: &str, output: &str) -> Vec<String> {
        let rule = all()
            .into_iter()
            .find(|rule| rule.name == name)
            .unwrap_or_else(|| panic!("no rule named {name}"));
        let command = Command::new(script, output);
        if (rule.matches)(&command) {
            (rule.fixes)(&command)
        } else {
            Vec::new()
        }
    }

    #[test]
    fn close_matches_order_like_python() {
        // the example in Python's difflib documentation
        assert_eq!(
            close_matches("appel", &["ape", "apple", "peach", "puppy"], 3, 0.6),
            ["apple", "ape"]
        );
        assert_eq!(
            close_matches(
                "pes",
                &["ps", "push", "pause", "attach", "tag", "stats", "images"],
                3,
                0.1
            ),
            ["ps", "push", "pause"]
        );
        // nothing close: no guess, where thefuck would take the first name
        assert_eq!(closest("xyzzy", ADB_COMMANDS), None);
        assert_eq!(
            close_matches::<&str>("a", &[], 3, 0.1),
            Vec::<String>::new()
        );
    }

    #[test]
    fn the_risky_rules_are_the_ones_that_are_off() {
        assert_eq!(
            DISABLED,
            ["docker_image_being_used_by_container", "django_south_ghost"]
        );
        let output = "south.exceptions.GhostMigrations: \n\n ! These migrations are in the database but not on disk:\n ! with the south_migrationhistory table, or pass --delete-ghost-migrations\n";
        assert!(
            crate::rules::fixes_for("./manage.py migrate", output)
                .iter()
                .all(|fix| !fix.contains("--delete-ghost-migrations"))
        );
        let err = "Error response from daemon: conflict: unable to delete cd809b04b6ff (cannot be forced) - image is being used by running container e5e2591040d1";
        assert!(
            crate::rules::fixes_for("docker image rm -f cd809b04b6ff", err)
                .iter()
                .all(|fix| !fix.contains("container rm"))
        );
    }

    const ADB_USAGE: &str = "Android Debug Bridge version 1.0.31\n\n -d                            - directs command to the only connected USB device\n                                 returns an error if more than one USB device is present.\n -e                            - directs command to the only running emulator.\n                                 returns an error if more than one emulator is running.\n -s <specific device>          - directs command to the device or emulator with the given\n                                 serial number or qualifier. Overrides ANDROID_SERIAL\n                                 environment variable.\n";

    #[test]
    fn adb_fixes_the_command_and_not_a_flag_value() {
        for (script, fix) in [
            (
                "adb puhs test.bin /sdcard/test.bin",
                "adb push test.bin /sdcard/test.bin",
            ),
            ("adb -s 1111 logcta", "adb -s 1111 logcat"),
            (
                "adb -P 666 pulll /sdcard/test.bin",
                "adb -P 666 pull /sdcard/test.bin",
            ),
            ("adb -d logcatt", "adb -d logcat"),
            ("adb -e reboott", "adb -e reboot"),
        ] {
            assert_eq!(
                fixes_of("adb_unknown_command", script, ADB_USAGE),
                [fix],
                "{script}"
            );
        }
        assert!(rule_matches("adb_unknown_command", "adb lgcat", ADB_USAGE));
        assert!(!rule_matches("adb_unknown_command", "git branch foo", ""));
        assert!(!rule_matches("adb_unknown_command", "abd push", ""));
        // no command is close: no guess
        assert!(fixes_of("adb_unknown_command", "adb xyzzy", ADB_USAGE).is_empty());
    }

    #[test]
    fn ag_searches_for_a_literal() {
        let output = "ERR: Bad regex! pcre_compile() failed at position 1: missing )\nIf you meant to search for a literal string, run ag with -Q\n";
        assert_eq!(fixes_of("ag_literal", "ag \\(", output), ["ag -Q \\("]);
        assert!(!rule_matches("ag_literal", "ag foo", ""));
    }

    const AWS_HEAD: &str = "usage: aws [options] <command> <subcommand> [<subcommand> ...] [parameters]\nTo see help text, you can run:\n\n  aws help\n  aws <command> help\n  aws <command> <subcommand> help\n";

    #[test]
    fn aws_takes_every_suggestion() {
        let misspelled_command = format!(
            "{AWS_HEAD}aws: error: argument command: Invalid choice, valid choices are:\n\ndynamodb                                 | dynamodbstreams\nec2                                      | ecr\n\n\nInvalid choice: 'dynamdb', maybe you meant:\n\n  * dynamodb\n"
        );
        let multiple = format!(
            "{AWS_HEAD}aws: error: argument operation: Invalid choice, valid choices are:\n\ndescribe-table                           | get-item\nlist-tables                              | put-item\n\n\nInvalid choice: 't-item', maybe you meant:\n\n  * put-item\n  * get-item\n"
        );
        let no_suggestions = format!(
            "{AWS_HEAD}aws: error: argument command: Invalid choice, valid choices are:\n\ndynamodb                                 | dynamodbstreams\nec2                                      | ecr\n"
        );
        assert_eq!(
            fixes_of("aws_cli", "aws dynamdb scan", &misspelled_command),
            ["aws dynamodb scan"]
        );
        assert_eq!(
            fixes_of("aws_cli", "aws dynamodb t-item", &multiple),
            ["aws dynamodb put-item", "aws dynamodb get-item"]
        );
        assert!(!rule_matches(
            "aws_cli",
            "aws dynamodb invalid",
            &no_suggestions
        ));
    }

    #[test]
    fn az_takes_the_suggestion() {
        let command = "az: 'providers' is not in the 'az' command group. See 'az --help'.\n\nThe most similar choice to 'providers' is:\n    provider\n";
        let subcommand = "az provider: 'lis' is not in the 'az provider' command group. See 'az provider --help'.\n\nThe most similar choice to 'lis' is:\n    list\n";
        assert_eq!(
            fixes_of("az_cli", "az providers list", command),
            ["az provider list"]
        );
        assert_eq!(
            fixes_of("az_cli", "az provider lis", subcommand),
            ["az provider list"]
        );
        let none = "az provider: error: the following arguments are required: _subcommand\nusage: az provider [-h] {list,show,register,unregister,operation} ...\n";
        assert!(!rule_matches("az_cli", "az provider", none));
    }

    #[test]
    fn cargo_alone_builds() {
        assert_eq!(fixes_of("cargo", "cargo", ""), ["cargo build"]);
        assert!(!rule_matches("cargo", "cargo build", ""));
        assert!(!rule_matches("cargo", "cargo test", ""));
    }

    #[test]
    fn cargo_takes_the_suggestion() {
        let old = "No such subcommand\n\n        Did you mean `build`?\n";
        let new = "error: no such subcommand\n\n\tDid you mean `build`?\n";
        // cargo now says "command"
        let newest = "error: no such command: `buils`\n\n\tDid you mean `build`?\n";
        assert_eq!(
            fixes_of("cargo_no_command", "cargo buid", old),
            ["cargo build"]
        );
        assert_eq!(
            fixes_of("cargo_no_command", "cargo buils", new),
            ["cargo build"]
        );
        assert_eq!(
            fixes_of("cargo_no_command", "cargo buils --release", newest),
            ["cargo build --release"]
        );
        assert!(!rule_matches("cargo_no_command", "cargo build", ""));
        assert!(!rule_matches("cargo_no_command", "npm buid", old));
    }

    #[test]
    fn composer_takes_the_suggestion() {
        let this = "\n\n                                    \n  [InvalidArgumentException]        \n  Command \"udpate\" is not defined.  \n  Did you mean this?                \n      update                        \n                                    \n\n\n";
        let one_of = "\n\n                                   \n  [InvalidArgumentException]       \n  Command \"pdate\" is not defined.  \n  Did you mean one of these?       \n      selfupdate                   \n      self-update                  \n      update                       \n                                   \n\n\n";
        let require = "Invalid argument package. Use \"composer require package\" instead to add packages to your composer.json.";
        assert_eq!(
            fixes_of("composer_not_command", "composer udpate", this),
            ["composer update"]
        );
        assert_eq!(
            fixes_of("composer_not_command", "composer pdate", one_of),
            ["composer selfupdate"]
        );
        assert_eq!(
            fixes_of("composer_not_command", "composer install package", require),
            ["composer require package"]
        );
        assert!(!rule_matches("composer_not_command", "ls update", this));
    }

    #[test]
    fn conda_takes_the_suggestion() {
        let output =
            "\n\nCommandNotFoundError: No command 'conda lst'.\nDid you mean 'conda list'?\n\n    ";
        assert_eq!(
            fixes_of("conda_mistype", "conda lst", output),
            ["conda list"]
        );
        assert!(!rule_matches(
            "conda_mistype",
            "codna list",
            "bash: codna: command not found"
        ));
    }

    #[test]
    fn cpp11_adds_the_standard() {
        // thefuck has no test for this rule: these are the messages it looks for
        let gcc = "In file included from foo.cpp:1:\n/usr/include/c++/4.6/bits/c++0x_warning.h:31:2: error: #error This file requires compiler and library support for the ISO C++ 2011 standard. This support is currently experimental, and must be enabled with the -std=c++11 or -std=gnu++11 compiler options.\n";
        let clang = "foo.cpp:3:5: warning: 'auto' type specifier is a C++11 extension [-Wc++11-extensions]\n";
        assert_eq!(
            fixes_of("cpp11", "g++ foo.cpp", gcc),
            ["g++ foo.cpp -std=c++11"]
        );
        assert_eq!(
            fixes_of("cpp11", "clang++ foo.cpp -o foo", clang),
            ["clang++ foo.cpp -o foo -std=c++11"]
        );
        assert!(!rule_matches("cpp11", "g++ foo.cpp", ""));
        assert!(!rule_matches("cpp11", "gcc foo.c", gcc));
    }

    #[test]
    fn django_south_flags() {
        let ghost = "south.exceptions.GhostMigrations: \n\n ! These migrations are in the database but not on disk:\n    <app1: 0033_auto__...>\n ! I'm not trusting myself; either fix this yourself by fiddling\n ! with the south_migrationhistory table, or pass --delete-ghost-migrations\n ! to South to have it delete ALL of these records (this may not be good).\n";
        let merge = "Running migrations for app:\n ! Migration app:0003_auto... should not have been applied before app:0002_auto__add_field_query_due_date_ but was.\nsouth.exceptions.InconsistentMigrationHistory: Inconsistent migration history\nThe following options are available:\n    --merge: will just attempt the migration ignoring any potential dependency conflicts.\n";
        assert_eq!(
            fixes_of("django_south_ghost", "./manage.py migrate auth", ghost),
            ["./manage.py migrate auth --delete-ghost-migrations"]
        );
        assert_eq!(
            fixes_of("django_south_merge", "./manage.py migrate auth", merge),
            ["./manage.py migrate auth --merge"]
        );
        assert!(rule_matches(
            "django_south_ghost",
            "python manage.py migrate",
            ghost
        ));
        for name in ["django_south_ghost", "django_south_merge"] {
            let output = if name == "django_south_ghost" {
                ghost
            } else {
                merge
            };
            assert!(!rule_matches(name, "./manage.py migrate", ""), "{name}");
            assert!(!rule_matches(name, "app migrate", output), "{name}");
            assert!(!rule_matches(name, "./manage.py test", output), "{name}");
        }
    }

    #[test]
    fn docker_removes_the_container_first() {
        let output = "Error response from daemon: conflict: unable to delete cd809b04b6ff (cannot be forced) - image is being used by running container e5e2591040d1";
        assert_eq!(
            fixes_of(
                "docker_image_being_used_by_container",
                "docker image rm -f cd809b04b6ff",
                output
            ),
            ["docker container rm -f e5e2591040d1 && docker image rm -f cd809b04b6ff"]
        );
        // the id is the last word, wherever the line was wrapped
        let wrapped = "\n        Error response from daemon: conflict: unable to delete cd809b04b6ff (cannot be forced) - image\n        is being used by running container e5e2591040d1\n        ";
        assert!(
            (all()
                .into_iter()
                .find(|rule| rule.name == "docker_image_being_used_by_container")
                .unwrap()
                .fixes)(&Command::new("docker image rm -f cd809b04b6ff", wrapped))
            .iter()
            .all(|fix| fix.starts_with("docker container rm -f e5e2591040d1 && "))
        );
        let name = "docker_image_being_used_by_container";
        assert!(!rule_matches(
            name,
            "docker image rm -f cd809b04b6ff",
            "bash: docker: command not found"
        ));
        assert!(!rule_matches(name, "git image rm -f cd809b04b6ff", output));
    }

    #[test]
    fn docker_logs_in_first() {
        let pull = "Sending build context to Docker daemon  118.8kB\nStep 1/6 : FROM foo/bar:fdb7c6d\npull access denied for foo/bar, repository does not exist or may require 'docker login'\n";
        let push = "The push refers to repository [artifactory:9090/foo/bar]\npush access denied for foo/bar, repository does not exist or may require 'docker login'\n";
        assert_eq!(
            fixes_of(
                "docker_login",
                "docker build -t artifactory:9090/foo/bar:fdb7c6d .",
                pull
            ),
            ["docker login && docker build -t artifactory:9090/foo/bar:fdb7c6d ."]
        );
        assert_eq!(
            fixes_of(
                "docker_login",
                "docker push artifactory:9090/foo/bar:fdb7c6d",
                push
            ),
            ["docker login && docker push artifactory:9090/foo/bar:fdb7c6d"]
        );
        let pushed = "The push refers to repository [artifactory:9090/foo/bar]\n9c29c7ad209d: Preparing\n71f3ad53dfe0: Pushed\ncb95fa0faeb1: Layer already exists\n";
        assert!(!rule_matches(
            "docker_login",
            "docker push artifactory:9090/foo/bar:fdb7c6d",
            pushed
        ));
    }

    fn docker_unknown(command: &str) -> String {
        format!("docker: '{command}' is not a docker command.\nSee 'docker --help'.")
    }

    #[test]
    fn docker_fixes_a_mistyped_command() {
        let fixed = |script: &str, output: &str| {
            docker_not_command_with(&Command::new(script, output), || {
                Some(DOCKER_HELP.to_string())
            })
        };
        assert_eq!(
            fixed("docker swarn", &docker_unknown("swarn")),
            ["docker swarm", "docker start", "docker search"]
        );
        assert_eq!(
            fixed("docker inage", &docker_unknown("inage")),
            ["docker image", "docker images", "docker rename"]
        );
        // `docker swarm int`: docker printed the help of `docker swarm`
        assert_eq!(
            fixed("docker swarm int", DOCKER_SWARM_HELP),
            [
                "docker swarm init",
                "docker swarm join",
                "docker swarm join-token"
            ]
        );
        assert_eq!(
            fixed("docker image la", DOCKER_IMAGE_HELP),
            ["docker image load", "docker image ls", "docker image tag"]
        );
        // `docker swarm` alone has no mistyped subcommand to fix
        assert!(fixed("docker swarm", DOCKER_SWARM_HELP).is_empty());
        // and a plain docker help is not a mistake
        assert!(!rule_matches("docker_not_command", "docker ps", ""));
        assert!(!rule_matches(
            "docker_not_command",
            "cat pes",
            &docker_unknown("pes")
        ));
        assert!(rule_matches(
            "docker_not_command",
            "sudo docker pes",
            &docker_unknown("pes")
        ));
        assert!(rule_matches(
            "docker_not_command",
            "docker swarn",
            &docker_unknown("swarn")
        ));
        assert!(rule_matches(
            "docker_not_command",
            "docker swarm int",
            DOCKER_SWARM_HELP
        ));
    }

    #[test]
    fn docker_help_lists_management_commands_then_the_rest() {
        let commands = docker_commands(DOCKER_HELP);
        assert_eq!(commands[0], "builder");
        assert!(commands.contains(&"swarm".to_string()));
        assert!(commands.contains(&"attach".to_string()));
        assert!(commands.contains(&"wait".to_string()));
        assert!(!commands.contains(&"Options:".to_string()));
    }

    #[test]
    fn fab_fixes_every_task_it_does_not_know() {
        let output = "\nWarning: Command(s) not found:\n    extenson\n    deloyp\n\nAvailable commands:\n\n    update_config\n    prepare_extension\n    Template               A string class for supporting $-substitutions.\n    deploy\n    glob                   Return a list of paths matching a pathname pattern.\n    install_web\n    set_version\n";
        for (script, fix) in [
            ("fab extenson", "fab prepare_extension"),
            (
                "fab extenson:version=2016",
                "fab prepare_extension:version=2016",
            ),
            (
                "fab extenson:version=2016 install_web set_version:val=0.5.0",
                "fab prepare_extension:version=2016 install_web set_version:val=0.5.0",
            ),
            (
                "fab extenson:version=2016 deloyp:beta=true -H the.fuck",
                "fab prepare_extension:version=2016 deploy:beta=true -H the.fuck",
            ),
        ] {
            assert_eq!(fixes_of("fab_command_not_found", script, output), [fix]);
        }
        assert!(rule_matches(
            "fab_command_not_found",
            "fab extenson deloyp",
            output
        ));
        assert!(!rule_matches(
            "fab_command_not_found",
            "gulp extenson",
            output
        ));
        assert!(!rule_matches("fab_command_not_found", "fab deloyp", ""));
    }

    #[test]
    fn gem_fixes_a_mistyped_command() {
        let output = |command: &str| {
            format!(
                "\nERROR:  While executing gem ... (Gem::CommandLineError)\n    Unknown command {command}\n"
            )
        };
        let first = |script: &str, command: &str| {
            gem_unknown_command(&Command::new(script, &output(command)), Some(GEM_HELP))
        };
        assert_eq!(
            first("gem isntall jekyll", "isntall")[0],
            "gem install jekyll"
        );
        assert_eq!(first("gem last --local", "last")[0], "gem list --local");
        // a wrapped description line is not a command
        assert!(!gem_commands(GEM_HELP).contains(&"located".to_string()));
        assert!(gem_commands(GEM_HELP).contains(&"pristine".to_string()));
        assert!(rule_matches(
            "gem_unknown_command",
            "gem isntall jekyll",
            &output("isntall")
        ));
        assert!(!rule_matches(
            "gem_unknown_command",
            "gem install jekyll",
            ""
        ));
        assert!(!rule_matches(
            "gem_unknown_command",
            "git log",
            &output("log")
        ));
        // without the list of commands there is nothing to fix with
        assert!(
            gem_unknown_command(&Command::new("gem isntall x", &output("isntall")), None)
                .is_empty()
        );
    }

    #[test]
    fn go_fixes_a_mistyped_command() {
        let output = "go bulid: unknown command\nRun 'go help' for usage.";
        let fixes = go_unknown_command(&Command::new("go bulid", output), Some(GO_HELP));
        assert_eq!(fixes, ["go build"]);
        let commands = go_commands(GO_HELP);
        assert_eq!(commands.first().map(String::as_str), Some("bug"));
        assert_eq!(commands.last().map(String::as_str), Some("vet"));
        assert!(rule_matches("go_unknown_command", "go bulid", output));
        assert!(!rule_matches(
            "go_unknown_command",
            "go run",
            "go run: no go files listed"
        ));
    }

    #[test]
    fn gradle_fixes_a_mistyped_task() {
        // thefuck has no test for these two rules; this is what gradle prints
        let tasks = "\n> Task :tasks\n\n------------------------------------------------------------\nTasks runnable from root project 'demo'\n------------------------------------------------------------\n\nBuild tasks\n-----------\nassemble - Assembles the outputs of this project.\nbuild - Assembles and tests this project.\nclean - Deletes the build directory.\n\nVerification tasks\n------------------\ncheck - Runs all checks.\ntest - Runs the test suite.\n\nBUILD SUCCESSFUL in 1s\n";
        assert_eq!(
            gradle_tasks(tasks),
            ["assemble", "build", "clean", "check", "test"]
        );
        let output = "FAILURE: Build failed with an exception.\n\n* What went wrong:\nTask 'tets' not found in root project 'demo'.\n";
        let wrong = GRADLE_TASK.captures(output).unwrap();
        assert_eq!(&wrong[1], "tets");
        assert_eq!(
            replace_command("gradle tets", &wrong[1], &gradle_tasks(tasks))[0],
            "gradle test"
        );
        assert!(rule_matches("gradle_no_task", "gradle tets", output));
        assert!(rule_matches("gradle_no_task", "./gradlew tets", output));
        assert!(!rule_matches(
            "gradle_no_task",
            "gradle test",
            "BUILD SUCCESSFUL"
        ));
        assert!(!rule_matches("gradle_no_task", "make tets", output));
    }

    #[test]
    fn gradle_wrapper_stands_in_for_a_missing_gradle() {
        let not_found = Command::new("gradle tasks", "gradle: not found");
        assert!(gradle_wrapper_applies(&not_found, false, true));
        assert!(!gradle_wrapper_applies(&not_found, false, false));
        assert!(!gradle_wrapper_applies(&not_found, true, true));
        assert!(!gradle_wrapper_applies(
            &Command::new("npm tasks", "npm: not found"),
            false,
            true
        ));
        assert!(!gradle_wrapper_applies(
            &Command::new("gradle tasks", "command not found"),
            true,
            true
        ));
    }

    #[test]
    fn grunt_fixes_a_mistyped_task() {
        let output = |task: &str| {
            format!(
                "\nWarning: Task \"{task}\" not found. Use --force to continue.\n\nAborted due to warnings.\n"
            )
        };
        let first = |script: &str, task: &str| {
            grunt_task_not_found(&Command::new(script, &output(task)), Some(GRUNT_HELP))
        };
        assert_eq!(first("grunt defualt", "defualt"), ["grunt default"]);
        assert_eq!(
            first("grunt cmpass:all", "cmpass:all"),
            ["grunt compass:all"]
        );
        assert_eq!(
            first("grunt cmpass:all --color", "cmpass:all"),
            ["grunt compass:all --color"]
        );
        assert!(grunt_tasks(GRUNT_HELP).contains(&"autoprefixer".to_string()));
        assert!(grunt_tasks(GRUNT_HELP).contains(&"default".to_string()));
        // the options listed before the tasks are not tasks
        assert!(!grunt_tasks(GRUNT_HELP).contains(&"--help,".to_string()));
        assert!(rule_matches(
            "grunt_task_not_found",
            "grunt defualt",
            &output("defualt")
        ));
        assert!(rule_matches(
            "grunt_task_not_found",
            "grunt buld:css",
            &output("buld:css")
        ));
        assert!(!rule_matches(
            "grunt_task_not_found",
            "npm nuild",
            &output("nuild")
        ));
        assert!(!rule_matches("grunt_task_not_found", "grunt rm", ""));
    }

    #[test]
    fn gulp_fixes_a_mistyped_task() {
        let output = "[00:41:11] Using gulpfile gulpfile.js\n[00:41:11] Task 'srve' is not in your gulpfile\n[00:41:11] Please check the documentation for proper gulpfile formatting\n";
        assert_eq!(
            gulp_not_task(
                &Command::new("gulp srve", output),
                Some("serve \nbuild \ndefault \n")
            ),
            ["gulp serve", "gulp default"]
        );
        assert!(rule_matches("gulp_not_task", "gulp srve", output));
        assert!(!rule_matches("gulp_not_task", "gulp serve", ""));
        assert!(!rule_matches("gulp_not_task", "cat srve", output));
    }

    #[test]
    fn heroku_names_the_app() {
        let output = "\n ▸    Multiple apps in git remotes\n ▸    Usage: --remote heroku-dev\n ▸    or: --app myapp-dev\n ▸    Your local git repository has more than 1 app referenced in git remotes.\n ▸    Because of this, we can't determine which app you want to run this command against.\n ▸    Specify the app you want with --app or --remote.\n ▸    Heroku remotes in repo:\n ▸    myapp (heroku)\n ▸    myapp-dev (heroku-dev)\n ▸\n ▸    https://devcenter.heroku.com/articles/multiple-environments\n";
        assert_eq!(
            fixes_of("heroku_multiple_apps", "heroku pg", output),
            ["heroku pg --app myapp", "heroku pg --app myapp-dev"]
        );
        let pg = "\n=== HEROKU_POSTGRESQL_TEAL_URL, DATABASE_URL\nPlan:                  Hobby-basic\nStatus:                Available\nRows:                  12345/10000000 (In compliance)\n";
        assert!(!rule_matches("heroku_multiple_apps", "heroku pg", pg));
    }

    #[test]
    fn heroku_runs_the_command_it_meant() {
        let output = "\n ▸    log is not a heroku command.\n ▸    Perhaps you meant logs?\n ▸    Run heroku _ to run heroku logs.\n ▸    Run heroku help for a list of available commands.";
        assert_eq!(
            fixes_of("heroku_not_command", "heroku log", output),
            ["heroku logs"]
        );
        assert!(!rule_matches("heroku_not_command", "cat log", output));
    }

    #[test]
    fn lein_takes_the_suggestions() {
        let output = "'rpl' is not a task. See 'lein help'.\n\nDid you mean this?\n         repl\n         jar\n";
        assert_eq!(
            fixes_of("lein_not_task", "lein rpl --help", output),
            ["lein repl --help", "lein jar --help"]
        );
        assert_eq!(
            fixes_of("lein_not_task", "sudo lein rpl", output),
            ["sudo lein repl", "sudo lein jar"]
        );
        assert!(!rule_matches("lein_not_task", "ls", output));
    }

    const MVN_PHASE_LIST: &str = "Available lifecycle phases are: validate, initialize, generate-sources, process-sources, generate-resources, process-resources, compile, process-classes, generate-test-sources, process-test-sources, generate-test-resources, process-test-resources, test-compile, process-test-classes, test, prepare-package, package, pre-integration-test, integration-test, post-integration-test, verify, install, deploy, pre-clean, clean, post-clean, pre-site, site, post-site, site-deploy. -> [Help 1]";
    const MVN_FORMAT: &str = "You must specify a valid lifecycle phase or a goal in the format <plugin-prefix>:<goal> or <plugin-group-id>:<plugin-artifact-id>[:<plugin-version>]:<goal>.";

    #[test]
    fn mvn_gives_a_goal_to_a_bare_command() {
        let output = format!(
            "[ERROR] No goals have been specified for this build. {MVN_FORMAT} {MVN_PHASE_LIST}"
        );
        assert_eq!(
            fixes_of("mvn_no_command", "mvn", &output),
            ["mvn clean package", "mvn clean install"]
        );
        assert_eq!(
            fixes_of("mvn_no_command", "mvn -N", &output),
            ["mvn -N clean package", "mvn -N clean install"]
        );
        let built =
            "[INFO] Scanning for projects...\n[INFO] BUILD SUCCESS\n[INFO] Total time: 0.477s\n";
        assert!(!rule_matches("mvn_no_command", "mvn clean", built));
        assert!(!rule_matches("mvn_no_command", "mvn --help", ""));
        assert!(!rule_matches("mvn_no_command", "mvn -v", ""));
    }

    #[test]
    fn mvn_fixes_a_mistyped_phase() {
        let output = |phase: &str| {
            format!("[ERROR] Unknown lifecycle phase \"{phase}\". {MVN_FORMAT} {MVN_PHASE_LIST}")
        };
        assert_eq!(
            fixes_of("mvn_unknown_lifecycle_phase", "mvn cle", &output("cle")),
            ["mvn clean", "mvn compile"]
        );
        assert_eq!(
            fixes_of(
                "mvn_unknown_lifecycle_phase",
                "mvn claen package",
                &output("claen")
            ),
            ["mvn clean package"]
        );
        assert!(!rule_matches(
            "mvn_unknown_lifecycle_phase",
            "mvn clean",
            "[INFO] BUILD SUCCESS"
        ));
    }

    #[test]
    fn npm_fixes_a_mistyped_script_name() {
        let output = |script: &str| {
            format!(
                "\nnpm ERR! Linux 4.4.0-31-generic\nnpm ERR! argv \"/opt/node/bin/node\" \"/opt/node/bin/npm\" \"run\" \"dvelop\"\nnpm ERR! node v4.4.7\nnpm ERR! npm  v2.15.8\n\nnpm ERR! missing script: {script}\nnpm ERR!\nnpm ERR! If you need help, you may report this error at:\n"
            )
        };
        let json = "{\n  \"name\": \"code-view-web\",\n  \"scripts\": {\n    \"test\": \"jest\",\n    \"build\": \"cp node_modules/ace-builds/src-min/ -a resources/ace/ && webpack\",\n    \"develop\": \"webpack-dev-server --progress --colors\",\n    \"watch-test\": \"jest --verbose --watch\"\n  }\n}";
        let scripts = package_scripts(json);
        assert_eq!(scripts, ["test", "build", "develop", "watch-test"]);
        let first = |script: &str, wrong: &str| {
            npm_missing_script(&Command::new(script, &output(wrong)), &scripts)[0].clone()
        };
        assert_eq!(
            first("npm ru wach-tests", "wach-tests"),
            "npm ru watch-test"
        );
        assert_eq!(
            first("npm -i run-script dvelop", "dvelop"),
            "npm -i run-script develop"
        );
        assert_eq!(
            first("npm -i run-script buld -X POST", "buld"),
            "npm -i run-script build -X POST"
        );
        // npm 7 and later
        let modern = "npm error Missing script: \"dvelop\"\nnpm error\nnpm error To see a list of scripts, run:\nnpm error   npm run\n";
        assert_eq!(
            npm_missing_script(&Command::new("npm run dvelop", modern), &scripts)[0],
            "npm run develop"
        );
        assert!(rule_matches(
            "npm_missing_script",
            "npm ru wach",
            &output("wach")
        ));
        assert!(rule_matches(
            "npm_missing_script",
            "npm run live-tes",
            &output("live-tes")
        ));
        assert!(rule_matches(
            "npm_missing_script",
            "npm run-script sahare",
            &output("sahare")
        ));
        assert!(!rule_matches(
            "npm_missing_script",
            "npm wach",
            &output("wach")
        ));
        assert!(!rule_matches(
            "npm_missing_script",
            "vim live-tes",
            &output("live-tes")
        ));
        assert!(!rule_matches(
            "npm_missing_script",
            "npm run-script sahare",
            ""
        ));
    }

    #[test]
    fn npm_fixes_a_mistyped_command() {
        for (script, fix) in [
            ("npm urgrade", "npm upgrade"),
            ("npm -g isntall gulp", "npm -g install gulp"),
            ("npm isntall -g gulp", "npm install -g gulp"),
            ("sudo npm isntall -g gulp", "sudo npm install -g gulp"),
        ] {
            assert_eq!(
                fixes_of("npm_wrong_command", script, NPM_USAGE),
                [fix],
                "{script}"
            );
        }
        for script in [
            "npm urgrdae",
            "npm urgrade -g",
            "npm -f urgrade -g",
            "npm urg",
        ] {
            assert!(
                rule_matches("npm_wrong_command", script, NPM_USAGE),
                "{script}"
            );
        }
        for (script, output) in [
            ("npm urgrade", ""),
            ("npm", NPM_USAGE),
            ("test urgrade", NPM_USAGE),
            ("npm -e", NPM_USAGE),
        ] {
            assert!(
                !rule_matches("npm_wrong_command", script, output),
                "{script}"
            );
        }
        // nothing is close to it: no guess
        assert!(fixes_of("npm_wrong_command", "npm xyzzyx", NPM_USAGE).is_empty());
    }

    #[test]
    fn php_serves_with_a_capital_s() {
        assert_eq!(
            fixes_of("php_s", "php -s localhost:8000", ""),
            ["php -S localhost:8000"]
        );
        assert_eq!(
            fixes_of("php_s", "php -t pub -s 0.0.0.0:8080", ""),
            ["php -t pub -S 0.0.0.0:8080"]
        );
        assert!(!rule_matches("php_s", "php -S localhost:8000", ""));
        assert!(!rule_matches("php_s", "vim php -s", ""));
    }

    #[test]
    fn pip_takes_the_suggestion() {
        let output = "ERROR: unknown command \"instatl\" - maybe you meant \"install\"";
        assert_eq!(
            fixes_of("pip_unknown_command", "pip instatl", output),
            ["pip install"]
        );
        let plus = "ERROR: unknown command \"un+install\" - maybe you meant \"uninstall\"";
        assert_eq!(
            fixes_of("pip_unknown_command", "pip un+install thefuck", plus),
            ["pip uninstall thefuck"]
        );
        assert!(!rule_matches(
            "pip_unknown_command",
            "pip i",
            "ERROR: unknown command \"i\""
        ));
    }

    #[test]
    fn python_runs_a_script_with_python() {
        assert_eq!(
            fixes_of("python_command", "./test_sudo.py", "Permission denied"),
            ["python ./test_sudo.py"]
        );
        assert_eq!(
            fixes_of(
                "python_command",
                "sudo temp.py",
                "sh: temp.py: command not found"
            ),
            ["sudo python temp.py"]
        );
        assert!(rule_matches(
            "python_command",
            "temp.py",
            "Permission denied"
        ));
        assert!(!rule_matches("python_command", "temp.py", ""));
        assert!(!rule_matches("python_command", "ls", "Permission denied"));
    }

    #[test]
    fn python_installs_a_missing_module() {
        let output = |file: &str, module: &str| {
            format!(
                "Traceback (most recent call last):\n  File \"{file}\", line 1, in <module>\n    import {module}\nModuleNotFoundError: No module named '{module}'"
            )
        };
        assert_eq!(
            fixes_of(
                "python_module_error",
                "python some_script.py",
                &output("some_script.py", "more_itertools")
            ),
            ["pip install more_itertools && python some_script.py"]
        );
        assert_eq!(
            fixes_of(
                "python_module_error",
                "./some_other_script.py",
                &output("some_other_script.py", "a_module")
            ),
            ["pip install a_module && ./some_other_script.py"]
        );
        assert!(!rule_matches(
            "python_module_error",
            "python hello_world.py",
            "Hello World"
        ));
        let name_error = "Traceback (most recent call last):\n  File \"hello_world.py\", line 1, in <module>\n    pritn(\"Hello World\")\nNameError: name 'pritn' is not defined";
        assert!(!rule_matches(
            "python_module_error",
            "./hello_world.py",
            name_error
        ));
    }

    #[test]
    fn rails_runs_the_pending_migrations() {
        let development = "\nMigrations are pending. To resolve this issue, run:\n\n        rails db:migrate RAILS_ENV=development\n";
        let test = "\nMigrations are pending. To resolve this issue, run:\n\n        bin/rails db:migrate RAILS_ENV=test\n";
        assert_eq!(
            fixes_of("rails_migrations_pending", "bin/rspec", development),
            ["rails db:migrate RAILS_ENV=development && bin/rspec"]
        );
        assert_eq!(
            fixes_of("rails_migrations_pending", "bin/rspec", test),
            ["bin/rails db:migrate RAILS_ENV=test && bin/rspec"]
        );
        assert!(!rule_matches(
            "rails_migrations_pending",
            "bin/rspec",
            "Environment data not found in the schema. To resolve this issue, run: \n\n"
        ));
    }

    #[test]
    fn react_native_fixes_a_mistyped_command() {
        let output = |command: &str| format!("Unrecognized command '{command}'");
        let first = |script: &str, wrong: &str| {
            react_native_command_unrecognized(
                &Command::new(script, &output(wrong)),
                Some(REACT_NATIVE_HELP),
            )[0]
            .clone()
        };
        assert_eq!(first("react-native star", "star"), "react-native start");
        assert_eq!(
            first("react-native logsandroid -f", "logsandroid"),
            "react-native log-android -f"
        );
        assert!(react_native_commands(REACT_NATIVE_HELP).contains(&"run-ios".to_string()));
        assert!(rule_matches(
            "react_native_command_unrecognized",
            "react-native star",
            &output("star")
        ));
        assert!(rule_matches(
            "react_native_command_unrecognized",
            "react-native android-logs",
            &output("android-logs")
        ));
        assert!(!rule_matches(
            "react_native_command_unrecognized",
            "gradle star",
            &output("star")
        ));
        assert!(!rule_matches(
            "react_native_command_unrecognized",
            "react-native start",
            ""
        ));
    }

    #[test]
    fn terraform_inits_first() {
        for (script, output) in [
            (
                "terraform plan",
                "Error: Initialization required. Please see the error message above.",
            ),
            (
                "terraform plan",
                "This module is not yet installed. Run \"terraform init\" to install all modules required by this configuration.",
            ),
            (
                "terraform apply",
                "Error: Initialization required. Please see the error message above.",
            ),
        ] {
            assert_eq!(
                fixes_of("terraform_init", script, output),
                [format!("terraform init && {script}")]
            );
        }
        for (script, output) in [
            ("terraform --version", "Terraform v0.12.2"),
            (
                "terraform plan",
                "No changes. Infrastructure is up-to-date.",
            ),
            (
                "terraform apply",
                "Apply complete! Resources: 0 added, 0 changed, 0 destroyed.",
            ),
        ] {
            assert!(!rule_matches("terraform_init", script, output));
            assert!(!rule_matches("terraform_no_command", script, output));
        }
    }

    #[test]
    fn terraform_takes_the_suggestion() {
        assert_eq!(
            fixes_of(
                "terraform_no_command",
                "terraform appyl",
                "Terraform has no command named \"appyl\". Did you mean \"apply\"?"
            ),
            ["terraform apply"]
        );
        assert_eq!(
            fixes_of(
                "terraform_no_command",
                "terraform destory --some-other-option",
                "Terraform has no command named \"destory\". Did you mean \"destroy\"?"
            ),
            ["terraform destroy --some-other-option"]
        );
    }

    #[test]
    fn tsuru_logs_in_first() {
        let expired = "Error: you're not authenticated or your session has expired.";
        let again = "You're not authenticated or your session has expired. Please use \"login\" command for authentication.";
        assert_eq!(
            fixes_of("tsuru_login", "tsuru app-shell", expired),
            ["tsuru login && tsuru app-shell"]
        );
        assert_eq!(
            fixes_of("tsuru_login", "tsuru app-log -f", again),
            ["tsuru login && tsuru app-log -f"]
        );
        assert!(!rule_matches("tsuru_login", "tsuru", ""));
        assert!(!rule_matches(
            "tsuru_login",
            "tsuru app-restart",
            "Error: unauthorized"
        ));
        assert!(!rule_matches(
            "tsuru_login",
            "tsuru app-log -f",
            "Error: unparseable data"
        ));
    }

    #[test]
    fn tsuru_takes_the_suggestions() {
        let output = |command: &str, names: &[&str]| {
            let mut text = format!(
                "tsuru: \"{command}\" is not a tsuru command. See \"tsuru help\".\n\nDid you mean?\n"
            );
            for name in names {
                text.push_str(&format!("\t{name}\n"));
            }
            text
        };
        assert_eq!(
            fixes_of(
                "tsuru_not_command",
                "tsuru log",
                &output("log", &["app-log", "login", "logout"])
            ),
            ["tsuru login", "tsuru logout", "tsuru app-log"]
        );
        assert_eq!(
            fixes_of(
                "tsuru_not_command",
                "tsuru app-l",
                &output("app-l", &["app-list", "app-log"])
            ),
            ["tsuru app-log", "tsuru app-list"]
        );
        assert_eq!(
            fixes_of(
                "tsuru_not_command",
                "tsuru user-list",
                &output("user-list", &["team-user-list"])
            ),
            ["tsuru team-user-list"]
        );
        // no suggestions, or not a tsuru command at all
        assert!(!rule_matches(
            "tsuru_not_command",
            "tsuru tchururu",
            &output("tchururu", &[])
        ));
        assert!(!rule_matches(
            "tsuru_not_command",
            "tsuru version",
            "tsuru version 0.16.0."
        ));
        assert!(!rule_matches(
            "tsuru_not_command",
            "tsuru env-get",
            "Error: App thefuck not found."
        ));
    }

    #[test]
    fn vagrant_starts_the_machine_first() {
        let ssh = "VM must be running to open SSH connection. Run `vagrant up`\nto start the virtual machine.";
        let rdp = "VM must be created before running this command. Run `vagrant up` first.";
        assert_eq!(
            fixes_of("vagrant_up", "vagrant ssh", ssh),
            ["vagrant up && vagrant ssh"]
        );
        assert_eq!(
            fixes_of("vagrant_up", "vagrant ssh devbox", ssh),
            [
                "vagrant up devbox && vagrant ssh devbox",
                "vagrant up && vagrant ssh devbox"
            ]
        );
        assert_eq!(
            fixes_of("vagrant_up", "vagrant rdp", rdp),
            ["vagrant up && vagrant rdp"]
        );
        assert!(!rule_matches("vagrant_up", "vagrant ssh", ""));
        assert!(!rule_matches(
            "vagrant_up",
            "vagrant ssh jeff",
            "The machine with the name 'jeff' was not found configured for this Vagrant environment."
        ));
    }

    #[test]
    fn workon_fixes_a_mistyped_environment_or_creates_it() {
        let envs = vec!["thefuck".to_string(), "code_view".to_string()];
        let first = |script: &str| workon_fixes(&Command::new(script, ""), &envs)[0].clone();
        assert_eq!(first("workon tehfuck"), "workon thefuck");
        assert_eq!(first("workon code-view"), "workon code_view");
        assert_eq!(first("workon zzzz"), "mkvirtualenv zzzz");
        // with no environments at all the only offer is to create it
        assert_eq!(
            workon_fixes(&Command::new("workon web", ""), &[]),
            ["mkvirtualenv web"]
        );
        for script in ["workon tehfuck", "workon code-view", "workon new-env"] {
            assert!(workon_applies(&Command::new(script, ""), &envs), "{script}");
        }
        for script in [
            "workon thefuck",
            "workon code_view",
            "work on tehfuck",
            "workon",
        ] {
            assert!(
                !workon_applies(&Command::new(script, ""), &envs),
                "{script}"
            );
        }
    }

    #[test]
    fn virtualenvs_are_the_directories() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("web")).unwrap();
        std::fs::create_dir(dir.path().join("api")).unwrap();
        std::fs::write(dir.path().join("postactivate"), "").unwrap();
        assert_eq!(virtualenvs(dir.path()), ["api", "web"]);
        assert!(virtualenvs(&dir.path().join("missing")).is_empty());
    }

    #[test]
    fn yarn_takes_the_suggestion() {
        for (script, output, fix) in [
            (
                "yarn rm",
                "error Did you mean `yarn remove`?",
                "yarn remove",
            ),
            (
                "yarn etil",
                "error Command \"etil\" not found. Did you mean \"etl\"?",
                "yarn etl",
            ),
            ("yarn ls", "error Did you mean `yarn list`?", "yarn list"),
        ] {
            assert_eq!(fixes_of("yarn_alias", script, output), [fix], "{script}");
        }
        assert!(!rule_matches(
            "yarn_alias",
            "yarn",
            "error Did you mean `yarn list`?"
        ));
        assert!(!rule_matches("yarn_alias", "yarn list", ""));
    }

    #[test]
    fn yarn_runs_the_command_that_replaced_the_old_one() {
        let output = |package: &str| {
            format!(
                "error `install` has been replaced with `add` to add new dependencies. Run \"yarn add {package}\" instead."
            )
        };
        assert_eq!(
            fixes_of(
                "yarn_command_replaced",
                "yarn install redux",
                &output("redux")
            ),
            ["yarn add redux"]
        );
        assert_eq!(
            fixes_of(
                "yarn_command_replaced",
                "yarn install lodash",
                &output("lodash")
            ),
            ["yarn add lodash"]
        );
        assert!(!rule_matches("yarn_command_replaced", "yarn install", ""));
    }

    #[test]
    fn yarn_help_opens_the_docs() {
        let output = "\n  Usage: yarn [command] [flags]\n\n  Options:\n\n    -h, --help                      output usage information\n    --network-concurrency <number>  maximum number of concurrent network requests\n\n  Visit https://yarnpkg.com/en/docs/cli/clean for documentation about this command.\n";
        let url = "https://yarnpkg.com/en/docs/cli/clean";
        let fixes = fixes_of("yarn_help", "yarn help clean", output);
        assert!(fixes == [format!("xdg-open {url}")] || fixes == [format!("open {url}")]);
        assert_eq!(open_command(url, false), format!("open {url}"));
        assert_eq!(open_command(url, true), format!("xdg-open {url}"));
        assert!(!rule_matches("yarn_help", "yarn help", output));
        assert!(!rule_matches("yarn_help", "yarn help clean", ""));
    }
}

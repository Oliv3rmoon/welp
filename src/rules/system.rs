use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::{Rule, replace_arg};
use crate::command::Command;

pub fn rules() -> Vec<Rule> {
    vec![
        Rule {
            name: "chmod_x",
            description: "Makes a script executable before running it",
            priority: 800,
            matches: |c| {
                c.parts.first().is_some_and(|p| p.starts_with("./"))
                    && c.output_lower().contains("permission denied")
                    && Path::new(&c.parts[0]).is_file()
            },
            fixes: |c| {
                vec![format!(
                    "chmod +x {} && {}",
                    shell_words::quote(&c.parts[0]),
                    c.script
                )]
            },
        },
        Rule {
            name: "sudo",
            description: "Runs the command again with sudo",
            priority: 1000,
            matches: |c| {
                let output = c.output_lower();
                c.parts.first().is_some_and(|p| p != "sudo")
                    && NEEDS_ROOT.iter().any(|text| output.contains(text))
            },
            fixes: |c| {
                if c.script.contains("&&") || c.script.contains("||") || c.script.contains('>') {
                    vec![format!("sudo sh -c {}", shell_words::quote(&c.script))]
                } else {
                    vec![format!("sudo {}", c.script)]
                }
            },
        },
        Rule {
            name: "apt_get",
            description: "Installs the missing program the system names, then runs it",
            priority: 950,
            matches: |c| APT_INSTALL.is_match(&c.output),
            fixes: |c| {
                APT_INSTALL
                    .captures(&c.output)
                    .map(|found| format!("sudo apt install {} && {}", &found[1], c.script))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "no_command",
            description: "Fixes a mistyped program name",
            priority: 3000,
            matches: |c| {
                let program = c.program();
                // With output, trust only a "not found"; without it (the
                // command was not run again), fall back to looking at PATH.
                // The message has to name the program itself: `pip` saying
                // "unknown command instal" is about pip's argument.
                let output = c.output_lower();
                let says_missing = output.is_empty()
                    || output.lines().any(|line| {
                        line.contains(&program.to_lowercase())
                            && ["not found", "unknown command", "no such file or directory"]
                                .iter()
                                .any(|text| line.contains(text))
                    });
                !program.is_empty()
                    && !program.contains('/')
                    && !BUILTINS.contains(&program)
                    && says_missing
                    && find_in_path(program).is_none()
            },
            fixes: |c| {
                let program = c.program();
                closest_programs(program, &path_programs())
                    .into_iter()
                    .map(|right| c.replace_word(program, &right))
                    .collect()
            },
        },
        Rule {
            name: "man_no_space",
            description: "Adds the missing space after man",
            priority: 2000,
            matches: |c| {
                c.output_lower().contains("command not found")
                    && c.script.strip_prefix("man").is_some_and(|rest| {
                        !rest.is_empty() && !rest.starts_with(char::is_whitespace)
                    })
            },
            fixes: |c| {
                c.script
                    .strip_prefix("man")
                    .map(|rest| format!("man {rest}"))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "missing_space_before_subcommand",
            description: "Adds the missing space after a program name: gitbranch to git branch",
            priority: 2900,
            matches: |c| unspaced(c).is_some(),
            fixes: |c| {
                unspaced(c)
                    .map(|(program, _)| format!("{program} {}", &c.script[program.len()..]))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "quotation_marks",
            description: "Makes mismatched quotes match: 'text\" to \"text\"",
            priority: 1000,
            // Both kinds in one command is often fine (`echo "it's"`), so
            // this waits for quotes that do not pair up.
            matches: |c| {
                c.script.contains('\'')
                    && c.script.contains('"')
                    && shell_words::split(&c.script).is_err()
            },
            fixes: |c| vec![c.script.replace('\'', "\"")],
        },
        Rule {
            name: "sudo_command_from_user_path",
            description: "Keeps your PATH under sudo for a program only you can find",
            priority: 1000,
            matches: |c| {
                c.parts.first().is_some_and(|p| p == "sudo") && user_path_command(c).is_some()
            },
            fixes: |c| {
                user_path_command(c)
                    .map(|name| replace_arg(&c.script, name, &format!("env \"PATH=$PATH\" {name}")))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "unsudo",
            description: "Drops sudo from a command that refuses to run as root",
            priority: 1000,
            matches: |c| {
                c.parts.first().is_some_and(|p| p == "sudo")
                    && c.output_lower()
                        .contains("you cannot perform this operation as root")
            },
            fixes: |c| {
                c.script
                    .strip_prefix("sudo")
                    .map(str::trim_start)
                    // `sudo -u somebody ...` is deliberate; leave it.
                    .filter(|rest| !rest.is_empty() && !rest.starts_with('-'))
                    .map(str::to_string)
                    .into_iter()
                    .collect()
            },
        },
    ]
}

/// Messages that mean the command needs root, from thefuck's sudo rule.
const NEEDS_ROOT: &[&str] = &[
    "permission denied",
    "eacces",
    "operation not permitted",
    "must be run as root",
    "must run as root",
    "must be superuser",
    "must be root",
    "need to be root",
    "needs to be run as root",
    "only root can ",
    "are you root",
    "requires root",
    "superuser privilege",
    "root privilege",
    "insufficient privileges",
    "authentication is required",
    "you don't have write permissions",
    "use `sudo`",
];

static APT_INSTALL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*(?:sudo )?apt(?:-get)? install ([A-Za-z0-9.+-]+)\s*$").unwrap()
});

/// Shell keywords and builtins, which are never on PATH.
const BUILTINS: &[&str] = &[
    "cd",
    "echo",
    "export",
    "source",
    ".",
    "alias",
    "unalias",
    "set",
    "unset",
    "exit",
    "history",
    "type",
    "eval",
    "exec",
    "fg",
    "bg",
    "jobs",
    "pushd",
    "popd",
    "read",
    "shift",
    "trap",
    "ulimit",
    "umask",
    "wait",
    "builtin",
    "command",
    "declare",
    "local",
    "function",
    "if",
    "for",
    "while",
    "case",
    "then",
    "do",
    "done",
    "fi",
    "esac",
    "true",
    "false",
    "test",
    "[",
    "[[",
    "printf",
    "pwd",
    "kill",
    "fc",
    "hash",
    "let",
    "return",
    "setopt",
    "bindkey",
    "abbr",
    "functions",
    "end",
];

static SUDO_NOT_FOUND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"sudo: (.*): command not found").unwrap());

/// The program sudo could not find although it is on the user's PATH.
fn user_path_command(command: &Command) -> Option<&str> {
    let found = SUDO_NOT_FOUND.captures(&command.output)?;
    let name = found.get(1)?.as_str();
    find_in_path(name).is_some().then_some(name)
}

/// The program and what follows it when the first word is a known program
/// with something stuck on the end: `gitbranch`, `ls-la`.
fn unspaced(command: &Command) -> Option<(String, String)> {
    let word = command.parts.first()?;
    let output = command.output_lower();
    if word.contains('/') || !(output.is_empty() || output.contains("not found")) {
        return None;
    }
    split_program(word, &path_programs())
}

/// The longest known program (of 2+ letters) the word starts with.
fn split_program(word: &str, known: &BTreeSet<String>) -> Option<(String, String)> {
    if known.contains(word) {
        return None;
    }
    known
        .iter()
        .filter(|name| name.len() > 1 && word.len() > name.len() && word.starts_with(name.as_str()))
        .max_by_key(|name| name.len())
        .map(|name| (name.clone(), word[name.len()..].to_string()))
}

fn find_in_path(program: &str) -> Option<std::path::PathBuf> {
    crate::runner::find_in_path(program)
}

fn path_programs() -> BTreeSet<String> {
    let Some(path) = env::var_os("PATH") else {
        return BTreeSet::new();
    };
    env::split_paths(&path)
        .filter_map(|dir| fs::read_dir(dir).ok())
        .flatten()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        // On Windows `git.exe` is typed as `git`.
        .map(|name| match name.rsplit_once('.') {
            Some((stem, ext))
                if cfg!(windows)
                    && ["exe", "cmd", "bat", "com"].contains(&ext.to_lowercase().as_str()) =>
            {
                stem.to_string()
            }
            _ => name,
        })
        .chain(BUILTINS.iter().map(|name| name.to_string()))
        .collect()
}

/// Up to three names close to `wrong`, closest first.
pub fn closest_programs(wrong: &str, known: &BTreeSet<String>) -> Vec<String> {
    let mut scored: Vec<(f64, &String)> = known
        .iter()
        .filter(|name| name.len().abs_diff(wrong.len()) <= 2)
        .map(|name| (strsim::normalized_damerau_levenshtein(wrong, name), name))
        .filter(|(score, _)| *score >= 0.6)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    scored
        .into_iter()
        .take(3)
        .map(|(_, name)| name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{fixes_for, offers, rule_matches};

    #[test]
    fn sudo_after_permission_denied() {
        assert_eq!(
            fixes_for(
                "apt install vim",
                "E: Could not open lock file - open (13: Permission denied)"
            ),
            ["sudo apt install vim"]
        );
        assert_eq!(
            fixes_for("echo hi > /etc/x", "sh: /etc/x: Permission denied"),
            ["sudo sh -c 'echo hi > /etc/x'"]
        );
        assert!(fixes_for("sudo ls", "permission denied").is_empty());
    }

    #[test]
    fn apt_install_suggestion() {
        let output =
            "Command 'htop' not found, but can be installed with:\n\nsudo apt install htop\n";
        assert_eq!(
            fixes_for("htop", output)[0],
            "sudo apt install htop && htop"
        );
    }

    #[test]
    fn closest_program_names() {
        let known: BTreeSet<String> = ["git", "gitk", "grep", "python3"].map(String::from).into();
        assert_eq!(closest_programs("gti", &known)[0], "git");
        assert_eq!(closest_programs("pyhton3", &known), ["python3"]);
        assert!(closest_programs("zzzz", &known).is_empty());
    }

    #[test]
    fn mistyped_program() {
        let fixes = fixes_for("welp-not-a-real-program-x", "");
        assert!(fixes.is_empty());
        assert_eq!(fixes_for("sl", "")[0], "ls");
    }

    #[test]
    fn man_missing_a_space() {
        assert_eq!(
            fixes_for("mandiff", "sh: mandiff: command not found")[0],
            "man diff"
        );
        assert!(offers(
            "mandiff",
            "zsh: command not found: mandiff",
            "man diff"
        ));
        assert!(!rule_matches("man_no_space", "mandiff", ""));
        assert!(!rule_matches("man_no_space", "", ""));
        assert!(!rule_matches(
            "man_no_space",
            "man",
            "sh: man: command not found"
        ));
        assert!(!rule_matches(
            "man_no_space",
            "man ls",
            "sh: man: command not found"
        ));
    }

    #[test]
    fn space_before_the_subcommand() {
        let known: BTreeSet<String> = ["git", "ls", "npm", "w", "watch"].map(String::from).into();
        for (word, program, rest) in [
            ("gitbranch", "git", "branch"),
            ("ls-la", "ls", "-la"),
            ("npminstall", "npm", "install"),
            ("watchls", "watch", "ls"),
        ] {
            assert_eq!(
                split_program(word, &known),
                Some((program.to_string(), rest.to_string())),
                "{word}"
            );
        }
        assert_eq!(split_program("git", &known), None);
        assert_eq!(split_program("vimfile", &known), None);
        // one-letter programs do not count
        assert_eq!(split_program("wx", &known), None);
    }

    #[test]
    fn space_before_the_subcommand_on_this_machine() {
        assert!(offers("ls-la", "sh: ls-la: command not found", "ls -la"));
        assert!(offers("ls-la /tmp", "", "ls -la /tmp"));
        // something is wrong that a space would not fix
        assert!(!rule_matches(
            "missing_space_before_subcommand",
            "ls-la",
            "ls: no such file"
        ));
        assert!(!rule_matches(
            "missing_space_before_subcommand",
            "ls -la",
            "sh: ls: command not found"
        ));
        assert!(!rule_matches(
            "missing_space_before_subcommand",
            "./ls-la",
            ""
        ));
    }

    #[test]
    fn mismatched_quotes() {
        for (script, fix) in [
            (
                "git commit -m 'My Message\"",
                "git commit -m \"My Message\"",
            ),
            (
                "git commit -am \"Mismatched Quotation Marks'",
                "git commit -am \"Mismatched Quotation Marks\"",
            ),
            ("echo \"hello'", "echo \"hello\""),
        ] {
            assert!(offers(script, "", fix), "{script}");
        }
        // quotes that pair up are not a typo
        for script in [
            "echo \"it's\"",
            "echo 'say \"hi\"'",
            "echo 'a' \"b\"",
            "echo 'a'",
            "echo \"b\"",
        ] {
            assert!(!rule_matches("quotation_marks", script, ""), "{script}");
        }
    }

    #[test]
    fn sudo_with_your_path() {
        // sh is on every PATH
        let output = "sudo: sh: command not found";
        assert!(offers(
            "sudo sh -c 'echo hi'",
            output,
            "sudo env \"PATH=$PATH\" sh -c 'echo hi'"
        ));
        assert!(offers(
            "sudo -u app sh run.sh",
            output,
            "sudo -u app env \"PATH=$PATH\" sh run.sh"
        ));
        assert!(!rule_matches(
            "sudo_command_from_user_path",
            "sh --version",
            output
        ));
        assert!(!rule_matches(
            "sudo_command_from_user_path",
            "sudo sh --version",
            ""
        ));
        // not on PATH either: nothing to carry over
        assert!(!rule_matches(
            "sudo_command_from_user_path",
            "sudo welp-nope-x",
            "sudo: welp-nope-x: command not found"
        ));
    }

    #[test]
    fn unsudo() {
        let root = "you cannot perform this operation as root";
        assert_eq!(fixes_for("sudo ls", root)[0], "ls");
        assert!(offers(
            "sudo pacaur -S helloworld",
            root,
            "pacaur -S helloworld"
        ));
        assert!(offers(
            "sudo brew install x",
            "Running Homebrew as root is extremely dangerous. You cannot perform this operation as root",
            "brew install x"
        ));
        assert!(!rule_matches("unsudo", "", ""));
        assert!(!rule_matches("unsudo", "sudo ls", "Permission denied"));
        assert!(!rule_matches("unsudo", "ls", root));
        assert!(
            fixes_for("sudo -u somebody ls", root)
                .iter()
                .all(|fix| fix.starts_with("sudo"))
        );
    }
}

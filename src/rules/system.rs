use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::Rule;

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

fn find_in_path(program: &str) -> Option<std::path::PathBuf> {
    env::split_paths(&env::var_os("PATH")?)
        .map(|dir| dir.join(program))
        .find(|path| path.is_file())
}

fn path_programs() -> BTreeSet<String> {
    let Some(path) = env::var_os("PATH") else {
        return BTreeSet::new();
    };
    env::split_paths(&path)
        .filter_map(|dir| fs::read_dir(dir).ok())
        .flatten()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
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
    use crate::rules::fixes_for;

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
}

use std::fs;
use std::path::{Path, PathBuf};

use super::{Rule, replace_program};
use crate::command::Command;

pub fn rules() -> Vec<Rule> {
    vec![
        Rule {
            name: "cd_parent",
            description: "cd.. to cd ..",
            priority: 900,
            matches: |c| c.script == "cd..",
            fixes: |_| vec!["cd ..".into()],
        },
        Rule {
            name: "cd_mkdir",
            description: "Creates the folder you tried to cd into",
            priority: 1000,
            matches: |c| c.is("cd") && target(c).is_some_and(|dir| !Path::new(dir).exists()),
            fixes: |c| {
                let dir = shell_words::quote(target(c).unwrap_or_default()).into_owned();
                vec![format!("mkdir -p {dir} && cd {dir}")]
            },
        },
        Rule {
            name: "mkdir_p",
            description: "Adds -p when parent folders are missing",
            priority: 1000,
            matches: |c| {
                c.is("mkdir")
                    && !has_flag(c, 'p')
                    && c.output_lower().contains("no such file or directory")
            },
            fixes: |c| vec![c.replace_word("mkdir", "mkdir -p")],
        },
        Rule {
            name: "cp_omitting_directory",
            description: "Adds -r to copy a folder",
            priority: 1000,
            matches: |c| {
                let output = c.output_lower();
                c.is("cp")
                    && !has_flag(c, 'r')
                    && !has_flag(c, 'R')
                    && (output.contains("omitting directory") || output.contains("is a directory"))
            },
            fixes: |c| vec![c.replace_word("cp", "cp -r")],
        },
        Rule {
            name: "rm_dir",
            description: "Adds -r to remove a folder",
            priority: 1000,
            // rm is never run again, so this looks at the files instead.
            matches: |c| {
                c.is("rm")
                    && !has_flag(c, 'r')
                    && !has_flag(c, 'R')
                    && (c.output_lower().contains("is a directory")
                        || operands(c).any(|path| Path::new(path).is_dir()))
            },
            fixes: |c| vec![c.replace_word("rm", "rm -r")],
        },
        Rule {
            name: "grep_recursive",
            description: "Adds -r to search a folder",
            priority: 1000,
            matches: |c| {
                c.is("grep") && !has_flag(c, 'r') && c.output_lower().contains("is a directory")
            },
            fixes: |c| vec![c.replace_word("grep", "grep -r")],
        },
        Rule {
            name: "touch",
            description: "Creates the missing folders, then the file",
            priority: 1000,
            matches: |c| c.is("touch") && c.output_lower().contains("no such file or directory"),
            fixes: |c| {
                operands(c)
                    .filter_map(|path| Path::new(path).parent())
                    .filter(|parent| !parent.as_os_str().is_empty() && !parent.exists())
                    .map(|parent| {
                        let parent = shell_words::quote(&parent.to_string_lossy()).into_owned();
                        format!("mkdir -p {parent} && {}", c.script)
                    })
                    .take(1)
                    .collect()
            },
        },
        Rule {
            name: "cat_dir",
            description: "Lists a folder instead of trying to cat it",
            priority: 1000,
            matches: |c| {
                c.is("cat")
                    && c.output_lower().contains("is a directory")
                    && operands(c).any(|path| Path::new(path).is_dir())
            },
            fixes: |c| vec![replace_program(&c.script, "ls")],
        },
        Rule {
            name: "cd_correction",
            description: "Fixes a misspelled folder in a cd path",
            // Ahead of cd_mkdir: a folder that is nearly there beats a new one.
            priority: 950,
            // It rebuilds `/`-rooted paths, so it is for Unix paths only.
            matches: |c| cfg!(unix) && cd_missing(c).is_some(),
            fixes: |c| cd_missing(c).and_then(corrected_cd).into_iter().collect(),
        },
        Rule {
            name: "has_exists_script",
            description: "Runs a script in this folder with ./",
            priority: 1000,
            matches: |c| local_script(c).is_some(),
            fixes: |c| {
                local_script(c)
                    .map(|script| {
                        replace_program(&c.script, &format!("./{}", shell_words::quote(script)))
                    })
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "ln_no_hard_link",
            description: "Adds -s when a hard link to a folder is not allowed",
            priority: 1000,
            matches: |c| {
                let output = c.output_lower();
                c.is("ln")
                    && !has_flag(c, 's')
                    && (output
                        .trim_end()
                        .ends_with("hard link not allowed for directory")
                        || output.lines().any(|line| {
                            line.starts_with("ln: ") && line.ends_with("is a directory")
                        }))
            },
            fixes: |c| vec![replace_program(&c.script, "ln -s")],
        },
        Rule {
            name: "sl_ls",
            description: "sl to ls",
            priority: 900,
            matches: |c| c.script == "sl",
            fixes: |_| vec!["ls".into()],
        },
    ]
}

/// Arguments after the program name that are not flags.
fn operands(command: &Command) -> impl Iterator<Item = &str> {
    let skip = if command.parts.first().is_some_and(|p| p == "sudo") {
        2
    } else {
        1
    };
    command
        .parts
        .iter()
        .skip(skip)
        .map(String::as_str)
        .filter(|part| !part.starts_with('-'))
}

fn target(command: &Command) -> Option<&str> {
    operands(command).next()
}

/// A short flag, alone (`-r`) or grouped (`-rf`).
fn has_flag(command: &Command, flag: char) -> bool {
    command
        .parts
        .iter()
        .any(|part| part.starts_with('-') && !part.starts_with("--") && part.contains(flag))
}

const CD_ERRORS: [&str; 3] = ["no such file or directory", "can't cd to", "does not exist"];

/// The folder `cd` could not enter: it is not there, and the shell said so
/// (or said nothing, as cd_mkdir assumes).
fn cd_missing(command: &Command) -> Option<&str> {
    if !command.is("cd") {
        return None;
    }
    let dir = target(command)?;
    let output = command.output_lower();
    let says_so = output.is_empty() || CD_ERRORS.iter().any(|text| output.contains(text));
    (says_so && !expand_home(dir).exists()).then_some(dir)
}

fn expand_home(path: &str) -> PathBuf {
    let home = dirs::home_dir();
    match (path, path.strip_prefix("~/"), home) {
        ("~", _, Some(home)) => home,
        (_, Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

/// `cd` into the path with each misspelled folder swapped for the closest
/// folder that exists. None when some part has no close match.
fn corrected_cd(target: &str) -> Option<String> {
    let (mut here, prefix, rest) = if let Some(rest) = target.strip_prefix('/') {
        (PathBuf::from("/"), "/", rest)
    } else if let Some(rest) = target.strip_prefix("~/") {
        (dirs::home_dir()?, "~/", rest)
    } else {
        (PathBuf::from("."), "", target)
    };
    let mut shown = Vec::new();
    for name in rest.split('/').filter(|name| !name.is_empty()) {
        match name {
            "." => shown.push(".".to_string()),
            ".." => {
                here.push("..");
                shown.push("..".to_string());
            }
            _ => {
                let found = closest_dir(&here, name)?;
                here.push(&found);
                shown.push(shell_words::quote(&found).into_owned());
            }
        }
    }
    (!shown.is_empty()).then(|| format!("cd {prefix}{}", shown.join("/")))
}

/// The folder in `parent` called `wanted`, or else the one spelled most like it.
fn closest_dir(parent: &Path, wanted: &str) -> Option<String> {
    let mut names: Vec<String> = fs::read_dir(parent)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    if names.iter().any(|name| name == wanted) {
        return Some(wanted.to_string());
    }
    names.sort();
    let wanted = wanted.to_lowercase();
    names
        .into_iter()
        .map(|name| {
            (
                strsim::normalized_damerau_levenshtein(&wanted, &name.to_lowercase()),
                name,
            )
        })
        .filter(|(score, _)| *score >= 0.6)
        .reduce(|best, next| if next.0 > best.0 { next } else { best })
        .map(|(_, name)| name)
}

/// The program, when it is a file in this folder that the shell could not find.
fn local_script(command: &Command) -> Option<&str> {
    let program = command.program();
    (!program.is_empty()
        && !program.contains('/')
        && command.output_lower().contains("command not found")
        && Path::new(program).is_file())
    .then_some(program)
}

#[cfg(test)]
mod tests {
    use crate::rules::{fixes_for, offers, rule_matches};

    #[test]
    fn cd_parent() {
        assert_eq!(fixes_for("cd..", ""), ["cd .."]);
    }

    #[test]
    fn cd_into_missing_folder() {
        assert_eq!(
            fixes_for("cd welp-missing/sub", ""),
            ["mkdir -p welp-missing/sub && cd welp-missing/sub"]
        );
        assert!(fixes_for("cd src", "").is_empty());
    }

    #[test]
    fn mkdir_parents() {
        let output = "mkdir: a/b: No such file or directory";
        assert_eq!(fixes_for("mkdir a/b", output), ["mkdir -p a/b"]);
    }

    #[test]
    fn copy_folder() {
        assert_eq!(
            fixes_for("cp src dst", "cp: src is a directory (not copied)."),
            ["cp -r src dst"]
        );
        assert_eq!(
            fixes_for(
                "cp src dst",
                "cp: -r not specified; omitting directory 'src'"
            ),
            ["cp -r src dst"]
        );
    }

    #[test]
    fn remove_folder_without_rerunning() {
        assert_eq!(fixes_for("rm src", ""), ["rm -r src"]);
        assert!(fixes_for("rm -rf src", "").is_empty());
    }

    #[test]
    fn touch_makes_parents() {
        let output = "touch: welp-missing/a.txt: No such file or directory";
        assert_eq!(
            fixes_for("touch welp-missing/a.txt", output),
            ["mkdir -p welp-missing && touch welp-missing/a.txt"]
        );
    }

    #[test]
    fn cat_on_a_folder() {
        assert_eq!(
            fixes_for("cat src", "cat: src: Is a directory\n"),
            ["ls src"]
        );
        assert!(offers("cat src/", "cat: src/: Is a directory\n", "ls src/"));
        assert!(offers(
            "sudo cat src",
            "cat: src: Is a directory\n",
            "sudo ls src"
        ));
        assert!(!rule_matches(
            "cat_dir",
            "cat welp-missing",
            "cat: welp-missing: No such file or directory\n"
        ));
        assert!(!rule_matches(
            "cat_dir",
            "cat Cargo.toml",
            "[package]\nname = \"welp\""
        ));
        assert!(!rule_matches("cat_dir", "notcat src", "some output"));
    }

    /// The path as the rule writes it: a quoted piece per folder.
    fn written(path: &std::path::Path) -> String {
        path.components()
            .filter_map(|part| match part {
                std::path::Component::Normal(name) => {
                    Some(shell_words::quote(&name.to_string_lossy()).into_owned())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/")
    }

    #[cfg(unix)]
    #[test]
    fn cd_to_the_closest_folders() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("projects/docs")).unwrap();
        std::fs::create_dir_all(tmp.path().join("projects/src")).unwrap();
        std::fs::write(tmp.path().join("projects/readme"), "").unwrap();
        let base = tmp.path().to_str().unwrap();
        let output = "sh: line 0: cd: nope: No such file or directory";
        let want = format!("cd /{}/projects/docs", written(tmp.path()));

        let fixes = fixes_for(&format!("cd {base}/projcts/dcos"), output);
        assert_eq!(fixes[0], want);
        // a trailing slash and a capital letter both still land on the folder
        assert_eq!(
            fixes_for(&format!("cd {base}/Projcts/Dcos/"), output)[0],
            want
        );
        // only a folder counts, not the file called readme
        assert!(
            !fixes_for(&format!("cd {base}/projects/readmee"), output)
                .iter()
                .any(|f| f.ends_with("readme"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn cd_correction_comes_before_cd_mkdir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("projects")).unwrap();
        let script = format!("cd {}/projcts", tmp.path().to_str().unwrap());
        let fixes = fixes_for(&script, "cd: no such file or directory");
        assert_eq!(fixes[0], format!("cd /{}/projects", written(tmp.path())));
        assert!(fixes[1].starts_with("mkdir -p "));
    }

    #[test]
    fn cd_correction_matches_what_thefuck_matches() {
        for (script, output) in [
            ("cd welp-nope", "cd: welp-nope: No such file or directory"),
            (
                "cd welp-nope/bar/baz",
                "cd: welp-nope: No such file or directory",
            ),
            ("cd welp-nope/bar/baz", "cd: can't cd to welp-nope/bar/baz"),
            (
                "cd /welp-nope/bar/",
                "cd: The directory \"/welp-nope/bar/\" does not exist",
            ),
        ] {
            assert!(rule_matches("cd_correction", script, output), "{script}");
        }
        assert!(!rule_matches("cd_correction", "cd src", ""));
        assert!(!rule_matches(
            "cd_correction",
            "cd src",
            "cd: src: No such file or directory"
        ));
        assert!(!rule_matches(
            "cd_correction",
            "cd welp-nope",
            "cd: too many arguments"
        ));
        assert!(!rule_matches("cd_correction", "", ""));
        // nothing close enough: leave it to cd_mkdir
        assert!(
            fixes_for("cd zzzzqqqq", "")
                .iter()
                .all(|f| f.starts_with("mkdir -p"))
        );
    }

    #[test]
    fn script_in_this_folder() {
        assert!(offers(
            "Cargo.toml --help",
            "sh: Cargo.toml: command not found",
            "./Cargo.toml --help"
        ));
        assert!(offers(
            "sudo Cargo.toml",
            "sudo: Cargo.toml: command not found",
            "sudo ./Cargo.toml"
        ));
        assert!(!rule_matches("has_exists_script", "Cargo.toml", ""));
        assert!(!rule_matches(
            "has_exists_script",
            "welp-missing",
            "sh: welp-missing: command not found"
        ));
        // a folder is not a script
        assert!(!rule_matches(
            "has_exists_script",
            "src",
            "sh: src: command not found"
        ));
    }

    #[test]
    fn ln_on_a_folder_makes_a_symlink() {
        for (script, output, fix) in [
            (
                "ln barDir barLink",
                "ln: \u{2018}barDir\u{2019}: hard link not allowed for directory",
                "ln -s barDir barLink",
            ),
            (
                "sudo ln a b",
                "ln: \u{2018}a\u{2019}: hard link not allowed for directory",
                "sudo ln -s a b",
            ),
            (
                "sudo ln -nbi a b",
                "ln: a: hard link not allowed for directory\n",
                "sudo ln -s -nbi a b",
            ),
            (
                "ln -nbi a b && ls",
                "ln: a: hard link not allowed for directory",
                "ln -s -nbi a b && ls",
            ),
            (
                "ln a ln",
                "ln: a: hard link not allowed for directory",
                "ln -s a ln",
            ),
            ("ln src link", "ln: src: Is a directory\n", "ln -s src link"),
        ] {
            assert!(offers(script, output, fix), "{script}");
        }
        for (script, output) in [
            ("", ""),
            ("ln a b", "... hard link"),
            ("sudo ln a b", "... hard link"),
            ("a b", "hard link not allowed for directory"),
            ("ln -s src link", "ln: src: Is a directory"),
        ] {
            assert!(!rule_matches("ln_no_hard_link", script, output), "{script}");
        }
    }
}

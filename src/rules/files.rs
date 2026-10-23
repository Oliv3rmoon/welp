use std::path::Path;

use super::Rule;
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

#[cfg(test)]
mod tests {
    use crate::rules::fixes_for;

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
}

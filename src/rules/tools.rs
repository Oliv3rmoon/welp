//! Rules for package managers and language tools, ported from thefuck:
//! brew, go, java, npm, pip, python and yarn.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::system::closest_programs;
use super::{Rule, replace_arg};
use crate::command::Command;

pub fn rules() -> Vec<Rule> {
    vec![
        Rule {
            name: "brew_link",
            description: "Previews what `brew link --overwrite` would replace",
            priority: 1000,
            matches: |c| {
                c.is("brew")
                    && args(c)
                        .first()
                        .is_some_and(|sub| matches!(sub.as_str(), "ln" | "link"))
                    && c.output.contains("brew link --overwrite --dry-run")
            },
            fixes: |c| vec![with_args(c, &["link", "--overwrite", "--dry-run"], 1)],
        },
        Rule {
            name: "brew_reinstall",
            description: "Reinstalls a formula that is already installed and up to date",
            priority: 1000,
            matches: |c| {
                c.is("brew")
                    && args(c).iter().any(|arg| arg == "install")
                    && ALREADY_INSTALLED.is_match(&c.output)
                    && HOW_TO_REINSTALL.is_match(&c.output)
            },
            fixes: |c| vec![c.replace_word("install", "reinstall")],
        },
        Rule {
            name: "brew_uninstall",
            description: "Adds --force to remove every installed version",
            priority: 1000,
            matches: |c| {
                c.is("brew")
                    && args(c)
                        .first()
                        .is_some_and(|sub| matches!(sub.as_str(), "uninstall" | "rm" | "remove"))
                    && c.output.contains("brew uninstall --force")
            },
            fixes: |c| vec![with_args(c, &["uninstall", "--force"], 1)],
        },
        Rule {
            name: "go_run",
            description: "Adds .go to the file `go run` was given",
            priority: 1000,
            matches: |c| c.is("go") && go_file(c).is_some(),
            fixes: |c| {
                go_file(c)
                    .map(|file| c.replace_word(file, &crate::command::quote(&format!("{file}.go"))))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "java",
            description: "Drops .java when running a class",
            priority: 1000,
            matches: |c| {
                c.is("java")
                    && c.script.ends_with(".java")
                    && c.output_lower()
                        .contains("could not find or load main class")
            },
            fixes: |c| {
                c.script
                    .strip_suffix(".java")
                    .map(str::to_string)
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "javac",
            description: "Adds .java to the file javac was given",
            priority: 1000,
            matches: |c| {
                let output = c.output_lower();
                c.is("javac")
                    && !c.script.ends_with(".java")
                    && ((output.contains("class names") && output.contains("only accepted if"))
                        || output.contains("file not found"))
            },
            fixes: |c| vec![format!("{}.java", c.script)],
        },
        Rule {
            name: "npm_run_script",
            description: "Uses npm run-script for a script from package.json",
            priority: 1000,
            matches: |c| {
                npm_run_script_applies(c, &package_scripts(&read_package_json(Path::new("."))))
            },
            fixes: |c| vec![with_args(c, &["run-script"], 0)],
        },
        Rule {
            name: "pip_install",
            description: "Installs with --user (then sudo) after a permission error",
            priority: 1000,
            matches: |c| {
                (c.is("pip") || c.is("pip3"))
                    && c.parts.first().is_some_and(|first| first != "sudo")
                    && args(c).iter().any(|arg| arg == "install")
                    && c.output_lower().contains("permission denied")
            },
            fixes: |c| {
                if c.script.contains("--user") {
                    vec![format!("sudo {}", c.script.replace(" --user", ""))]
                } else {
                    vec![replace_arg(&c.script, "install", "install --user")]
                }
            },
        },
        Rule {
            name: "python_execute",
            description: "Adds .py to the script python was given",
            priority: 1000,
            matches: |c| python_file(c).is_some(),
            fixes: |c| {
                python_file(c)
                    .map(|file| c.replace_word(file, &crate::command::quote(&format!("{file}.py"))))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "yarn_command_not_found",
            description: "Fixes a mistyped yarn command",
            priority: 1000,
            matches: |c| c.is("yarn") && YARN_NOT_FOUND.is_match(&c.output),
            fixes: yarn_command_not_found,
        },
    ]
}

/// Arguments after the program name (and a leading `sudo`).
fn args(command: &Command) -> &[String] {
    let skip = if command.parts.first().is_some_and(|p| p == "sudo") {
        2
    } else {
        1
    };
    command.parts.get(skip..).unwrap_or(&[])
}

/// The command with the arguments from `from` on kept, and `put` inserted
/// before them (replacing the ones before `from`).
fn with_args(command: &Command, put: &[&str], from: usize) -> String {
    let skip = command.parts.len() - args(command).len();
    command.parts[..skip]
        .iter()
        .map(String::as_str)
        .chain(put.iter().copied())
        .chain(args(command).iter().skip(from).map(String::as_str))
        .map(crate::command::quote)
        .collect::<Vec<_>>()
        .join(" ")
}

// brew

static ALREADY_INSTALLED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Warning: [^\n]+ is already installed and up-to-date").unwrap());
// Older brew says: run `brew reinstall x`. Newer: run:\n  brew reinstall x
static HOW_TO_REINSTALL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"To reinstall [^\n]+?, run:?\s+`?brew reinstall").unwrap());

// go and python

/// The file operand of `go run <file>` when it is missing its `.go` and the
/// file with the extension is there.
fn go_file(command: &Command) -> Option<&str> {
    let args = args(command);
    if args.first()? != "run" {
        return None;
    }
    let output = command.output_lower();
    let says_so = [
        "no go files",
        "is not in std",
        "cannot find package",
        "no required module provides package",
    ]
    .iter()
    .any(|text| output.contains(text));
    let file = args[1..].iter().find(|arg| !arg.starts_with('-'))?;
    (says_so && !file.ends_with(".go") && Path::new(&format!("{file}.go")).is_file())
        .then_some(file.as_str())
}

/// The script operand of `python <script>` when it is missing its `.py` and
/// the file with the extension is there.
fn python_file(command: &Command) -> Option<&str> {
    if !(command.is("python") || command.is("python2") || command.is("python3")) {
        return None;
    }
    let output = command.output_lower();
    if !(output.contains("can't open file") && output.contains("no such file")) {
        return None;
    }
    let file = args(command).iter().find(|arg| !arg.starts_with('-'))?;
    (!file.ends_with(".py") && Path::new(&format!("{file}.py")).is_file()).then_some(file.as_str())
}

// npm

/// npm said it did not know the command, and the command is a script in
/// package.json that needs `run-script`.
fn npm_run_script_applies(command: &Command, scripts: &[String]) -> bool {
    let output = command.output_lower();
    command.is("npm")
        && (command.output.contains("Usage: npm <command>") || output.contains("unknown command"))
        && !command.parts.iter().any(|part| part.starts_with("ru"))
        && args(command)
            .iter()
            .find(|arg| !arg.starts_with('-'))
            .is_some_and(|script| scripts.contains(script))
}

fn read_package_json(dir: &Path) -> String {
    fs::read_to_string(dir.join("package.json")).unwrap_or_default()
}

static SCRIPTS_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""scripts"\s*:\s*\{"#).unwrap());

/// The names under `"scripts"` in a package.json. package.json is plain
/// JSON, and a scripts block is only string pairs, so a small scanner does.
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

// yarn

static YARN_NOT_FOUND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"error Command "([^"]*)" not found\."#).unwrap());

/// yarn's own commands, from `yarn --help`.
const YARN_COMMANDS: &[&str] = &[
    "access",
    "add",
    "audit",
    "autoclean",
    "bin",
    "cache",
    "check",
    "clean",
    "config",
    "create",
    "exec",
    "generate-lock-entry",
    "global",
    "help",
    "import",
    "info",
    "init",
    "install",
    "licenses",
    "link",
    "list",
    "login",
    "logout",
    "node",
    "outdated",
    "owner",
    "pack",
    "policies",
    "publish",
    "remove",
    "run",
    "tag",
    "team",
    "unlink",
    "unplug",
    "upgrade",
    "upgrade-interactive",
    "version",
    "versions",
    "why",
    "workspace",
    "workspaces",
];

fn yarn_command_not_found(command: &Command) -> Vec<String> {
    let Some(found) = YARN_NOT_FOUND.captures(&command.output) else {
        return Vec::new();
    };
    let wrong = &found[1];
    // npm's `require` is yarn's `add`.
    if wrong == "require" {
        return vec![replace_arg(&command.script, wrong, "add")];
    }
    let known: BTreeSet<String> = YARN_COMMANDS.iter().map(|name| name.to_string()).collect();
    closest_programs(wrong, &known)
        .into_iter()
        .map(|right| replace_arg(&command.script, wrong, &right))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{fixes_for, offers, rule_matches};

    #[test]
    fn brew_link_previews_first() {
        let output = "Error: Could not symlink bin/gcp\nTarget /usr/local/bin/gcp\nalready exists. You may want to remove it:\n  rm '/usr/local/bin/gcp'\n\nTo force the link and overwrite all conflicting files:\n  brew link --overwrite coreutils\n\nTo list all files that would be deleted:\n  brew link --overwrite --dry-run coreutils\n";
        for script in ["brew link coreutils", "brew ln coreutils"] {
            assert!(
                offers(script, output, "brew link --overwrite --dry-run coreutils"),
                "{script}"
            );
        }
        assert!(!rule_matches("brew_link", "brew link coreutils", ""));
    }

    #[test]
    fn brew_reinstall_when_up_to_date() {
        let old = "Warning: thefuck 9.9 is already installed and up-to-date\nTo reinstall 9.9, run `brew reinstall thefuck`";
        let new = "Warning: thefuck 9.9 is already installed and up-to-date.\nTo reinstall 9.9, run:\n  brew reinstall thefuck\n";
        for output in [old, new] {
            assert!(offers(
                "brew install thefuck",
                output,
                "brew reinstall thefuck"
            ));
        }
        assert!(offers(
            "brew install bar zap",
            old,
            "brew reinstall bar zap"
        ));
        assert!(!rule_matches(
            "brew_reinstall",
            "brew reinstall thefuck",
            ""
        ));
        assert!(!rule_matches("brew_reinstall", "brew install foo", ""));
    }

    #[test]
    fn brew_uninstall_every_version() {
        let output = "Uninstalling /usr/local/Cellar/tbb/4.4-20160916... (118 files, 1.9M)\ntbb 4.4-20160526, 4.4-20160722 are still installed.\nRemove all versions with `brew uninstall --force tbb`.\n";
        for script in ["brew uninstall tbb", "brew rm tbb", "brew remove tbb"] {
            assert!(
                offers(script, output, "brew uninstall --force tbb"),
                "{script}"
            );
        }
        let done = "Uninstalling /usr/local/Cellar/gnuplot/5.0.4_1... (44 files, 2.3M)\n";
        assert!(!rule_matches("brew_uninstall", "brew remove gnuplot", done));
    }

    #[test]
    fn go_run_adds_the_extension() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("foo");
        std::fs::write(dir.path().join("foo.go"), "package main").unwrap();
        let file = file.to_str().unwrap();
        let output = format!("package {file} is not in std (/usr/lib/go/src/{file})");
        assert!(offers(
            &format!("go run {file}"),
            &output,
            &format!("go run {file}.go")
        ));
        assert!(offers(
            &format!("go run {file} --flag"),
            "error: go run: no go files listed",
            &format!("go run {file}.go --flag")
        ));
        // nothing to add the extension to, or it already worked
        assert!(!rule_matches(
            "go_run",
            "go run welp-missing",
            "error: go run: no go files listed"
        ));
        assert!(!rule_matches("go_run", &format!("go run {file}"), ""));
        assert!(!rule_matches(
            "go_run",
            &format!("go run {file}.go"),
            &output
        ));
    }

    #[test]
    fn java_class_not_source_file() {
        let output = "Error: Could not find or load main class foo.java\nCaused by: java.lang.ClassNotFoundException: foo.java";
        assert_eq!(fixes_for("java foo.java", output), ["java foo"]);
        assert!(!rule_matches("java", "java foo.java", ""));
        assert!(!rule_matches("java", "java foo", output));
    }

    #[test]
    fn javac_adds_the_extension() {
        let output = "error: Class names, 'foo', are only accepted if annotation processing is explicitly requested\n1 error";
        assert_eq!(fixes_for("javac foo", output), ["javac foo.java"]);
        assert_eq!(
            fixes_for("javac foo", "javac: file not found: foo"),
            ["javac foo.java"]
        );
        assert!(!rule_matches("javac", "javac foo.java", output));
        assert!(!rule_matches("javac", "javac foo", ""));
    }

    #[test]
    fn npm_script_needs_run_script() {
        let usage = "\nUsage: npm <command>\n\nwhere <command> is one of:\n    access, add-user, adduser, apihelp, author, bin, bugs, c,\n    cache, completion, config, ddp, dedupe, deprecate, dist-tag,\n    update, upgrade, v, version, view, whoami\n";
        let modern = "npm error Unknown command: \"develop\"\nnpm error\nnpm error To see a list of supported npm commands, run:\nnpm error   npm help";
        let json = "{\n  \"name\": \"code-view-web\",\n  \"scripts\": {\n    \"build\": \"cp node_modules/ace-builds/src-min/ -a resources/ace/ && webpack --progress\",\n    \"develop\": \"webpack-dev-server --progress --colors\",\n    \"watch-test\": \"jest --verbose --watch\"\n  },\n  \"dependencies\": { \"jest\": \"1\" }\n}";
        let scripts = package_scripts(json);
        assert_eq!(scripts, ["build", "develop", "watch-test"]);

        let yes = |script: &str, output: &str| {
            let command = Command::new(script, output);
            npm_run_script_applies(&command, &scripts)
                .then(|| with_args(&command, &["run-script"], 0))
        };
        assert_eq!(
            yes("npm watch-test", usage).unwrap(),
            "npm run-script watch-test"
        );
        assert_eq!(
            yes("npm -i develop", usage).unwrap(),
            "npm run-script -i develop"
        );
        assert_eq!(
            yes("npm -i watch-test --path ..", usage).unwrap(),
            "npm run-script -i watch-test --path ..",
        );
        assert!(yes("npm develop", modern).is_some());
        assert!(yes("npm test", "TEST FAIL").is_none());
        assert!(yes("npm watch-test", "TEST FAIL").is_none());
        assert!(yes("npm run watch-test", usage).is_none());
        assert!(yes("npm test", usage).is_none());
        assert!(yes("vim watch-test", usage).is_none());
    }

    #[test]
    fn package_scripts_edge_cases() {
        assert!(package_scripts("{}").is_empty());
        assert!(package_scripts("{\"scripts\": {}}").is_empty());
        assert_eq!(
            package_scripts("{\"scripts\":{\"a\":\"echo \\\"}\\\"\",\"b\":\"x\"}}"),
            ["a", "b"]
        );
    }

    #[test]
    fn npm_script_without_a_package_json_is_left_alone() {
        // the tests run in the crate root, which has no package.json
        let usage = "Usage: npm <command>\n";
        assert!(!rule_matches("npm_run_script", "npm watch-test", usage));
    }

    #[test]
    fn pip_install_tries_user_then_sudo() {
        let denied = "    Could not install packages due to an EnvironmentError: [Errno 13] Permission denied: '/Library/Python/2.7/site-packages/entrypoints.pyc'\nConsider using the `--user` option or check the permissions.\n";
        assert!(offers(
            "pip install -r requirements.txt",
            denied,
            "pip install --user -r requirements.txt"
        ));
        assert!(offers(
            "pip install bacon",
            denied,
            "pip install --user bacon"
        ));
        assert!(offers(
            "pip install --user -r requirements.txt",
            denied,
            "sudo pip install -r requirements.txt"
        ));
        let installed = "Collecting bacon\n  Downloading bacon-0.3.1.zip (11.0MB)\nInstalling collected packages: bacon\n  Running setup.py install for bacon ... done\nSuccessfully installed bacon-0.3.1\n";
        assert!(!rule_matches("pip_install", "pip install bacon", installed));
        // already root: --user would be the wrong fix
        assert!(!offers(
            "sudo pip install bacon",
            denied,
            "sudo pip install --user bacon"
        ));
    }

    #[test]
    fn python_script_without_extension() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("foo.py"), "print(1)").unwrap();
        let file = dir.path().join("foo");
        let file = file.to_str().unwrap();
        let output =
            format!("python3: can't open file '{file}': [Errno 2] No such file or directory");
        assert_eq!(
            fixes_for(&format!("python3 {file}"), &output)[0],
            format!("python3 {file}.py")
        );
        assert!(offers(
            &format!("python {file} --verbose"),
            &output,
            &format!("python {file}.py --verbose")
        ));
        assert!(!rule_matches(
            "python_execute",
            &format!("python {file}"),
            ""
        ));
        assert!(!rule_matches(
            "python_execute",
            &format!("python {file}.py"),
            &output
        ));
        assert!(
            fixes_for(
                "python welp-missing",
                "python: can't open file 'welp-missing': [Errno 2] No such file or directory"
            )
            .iter()
            .all(|fix| !fix.ends_with(".py"))
        );
    }

    #[test]
    fn yarn_mistyped_command() {
        let output = |command: &str| format!("\nerror Command \"{command}\" not found.\n");
        assert_eq!(
            fixes_for("yarn whyy webpack", &output("whyy"))[0],
            "yarn why webpack"
        );
        assert_eq!(
            fixes_for("yarn require lodash", &output("require"))[0],
            "yarn add lodash"
        );
        assert!(!rule_matches("yarn_command_not_found", "yarn install", ""));
        assert!(!rule_matches(
            "yarn_command_not_found",
            "npm nuild",
            &output("nuild")
        ));
    }
}

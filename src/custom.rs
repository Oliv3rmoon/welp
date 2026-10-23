//! Your own rules, as TOML files in `~/.config/welp/rules/`. For example:
//!
//! ```toml
//! name = "kubectl_typo"
//! program = "kubectl"
//! output = 'unknown command "(?P<wrong>[a-z]+)"'
//! fix = "kubectl get pods"
//! ```
//!
//! A rule matches when everything it names matches: the program, a regex on
//! the command (`script`) and a regex on what it printed (`output`). Its fix
//! can use `{script}` and any named group from those regexes.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use crate::command::Command;
use crate::rules::Fix;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleFile {
    name: String,
    #[serde(default)]
    description: String,
    program: Option<String>,
    script: Option<String>,
    output: Option<String>,
    fix: Option<String>,
    #[serde(default)]
    fixes: Vec<String>,
    /// Offer these fixes before the built-in ones (the default) or after.
    #[serde(default = "yes")]
    first: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug)]
pub struct CustomRule {
    pub name: String,
    pub description: String,
    pub first: bool,
    program: Option<String>,
    script: Option<Regex>,
    output: Option<Regex>,
    fixes: Vec<String>,
}

/// Loads every `*.toml` in `dir`. Files that don't make sense come back as
/// messages instead of rules.
pub fn load(dir: &Path) -> (Vec<CustomRule>, Vec<String>) {
    let mut rules = Vec::new();
    let mut problems = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return (rules, problems);
    };
    let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
    {
        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        match fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|text| parse(&text))
        {
            Ok(rule) => rules.push(rule),
            Err(problem) => problems.push(format!("{file_name}: {problem}")),
        }
    }
    (rules, problems)
}

fn parse(text: &str) -> Result<CustomRule, String> {
    let file: RuleFile = toml::from_str(text).map_err(|error| error.message().to_string())?;
    if file.program.is_none() && file.script.is_none() && file.output.is_none() {
        return Err("needs at least one of program, script or output".into());
    }
    let regex = |pattern: Option<String>| {
        pattern
            .map(|p| Regex::new(&p).map_err(|error| error.to_string()))
            .transpose()
    };
    let mut fixes = file.fixes;
    fixes.extend(file.fix);
    if fixes.is_empty() {
        return Err("needs a fix (or fixes)".into());
    }
    Ok(CustomRule {
        name: file.name,
        description: file.description,
        first: file.first,
        program: file.program,
        script: regex(file.script)?,
        output: regex(file.output)?,
        fixes,
    })
}

impl CustomRule {
    pub fn fixes(&self, command: &Command) -> Vec<Fix> {
        if self
            .program
            .as_deref()
            .is_some_and(|program| !command.is(program))
        {
            return Vec::new();
        }
        let mut values = vec![("script".to_string(), command.script.clone())];
        for (regex, text) in [
            (&self.script, &command.script),
            (&self.output, &command.output),
        ] {
            let Some(regex) = regex else { continue };
            let Some(found) = regex.captures(text) else {
                return Vec::new();
            };
            for name in regex.capture_names().flatten() {
                if let Some(value) = found.name(name) {
                    values.push((name.to_string(), value.as_str().to_string()));
                }
            }
        }
        self.fixes
            .iter()
            .filter_map(|template| fill(template, &values))
            .map(|command| Fix {
                command,
                rule: self.name.clone(),
            })
            .collect()
    }
}

static PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{[A-Za-z_][A-Za-z0-9_]*\}").unwrap());

/// Fills `{name}` placeholders; `None` if one has no value.
fn fill(template: &str, values: &[(String, String)]) -> Option<String> {
    let mut out = template.to_string();
    for (name, value) in values {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    (!PLACEHOLDER.is_match(&out)).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(text: &str) -> CustomRule {
        parse(text).unwrap()
    }

    #[test]
    fn fills_captures_and_script() {
        let rule = rule(
            r#"
            name = "kubectl_typo"
            program = "kubectl"
            output = 'unknown command "(?P<wrong>[a-z]+)"'
            fixes = ["kubectl get {wrong}", "echo {script}"]
            "#,
        );
        let command = Command::new(
            "kubectl gt pods",
            "error: unknown command \"gt\" for \"kubectl\"",
        );
        let fixes: Vec<_> = rule
            .fixes(&command)
            .into_iter()
            .map(|fix| fix.command)
            .collect();
        assert_eq!(fixes, ["kubectl get gt", "echo kubectl gt pods"]);
        assert!(
            rule.fixes(&Command::new("kubectl get pods", "fine"))
                .is_empty()
        );
        assert!(
            rule.fixes(&Command::new("helm gt", "unknown command \"gt\""))
                .is_empty()
        );
    }

    #[test]
    fn script_regex_alone() {
        let rule = rule("name = 'please'\nscript = '^please (?P<rest>.+)$'\nfix = 'sudo {rest}'");
        let fixes = rule.fixes(&Command::new("please make install", ""));
        assert_eq!(fixes[0].command, "sudo make install");
        assert_eq!(fixes[0].rule, "please");
    }

    #[test]
    fn missing_values_drop_the_fix() {
        let rule = rule("name = 'x'\nprogram = 'x'\nfix = 'x {nope}'");
        assert!(rule.fixes(&Command::new("x", "")).is_empty());
    }

    #[test]
    fn bad_files_are_reported_not_loaded() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("good.toml"),
            "name = 'a'\nprogram = 'a'\nfix = 'b'",
        )
        .unwrap();
        fs::write(dir.path().join("nothing.toml"), "name = 'b'\nfix = 'c'").unwrap();
        fs::write(
            dir.path().join("regex.toml"),
            "name = 'c'\noutput = '('\nfix = 'd'",
        )
        .unwrap();
        fs::write(dir.path().join("notes.txt"), "ignored").unwrap();
        let (rules, problems) = load(dir.path());
        assert_eq!(rules.len(), 1);
        assert_eq!(problems.len(), 2);
        assert!(
            problems
                .iter()
                .any(|p| p.starts_with("nothing.toml: needs at least one"))
        );
    }
}

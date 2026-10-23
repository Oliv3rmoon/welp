use std::sync::LazyLock;

use regex::Regex;

use super::Rule;
use crate::command::Command;

pub fn rules() -> Vec<Rule> {
    vec![Rule {
        name: "did_you_mean",
        description: "Uses the fix a tool suggests (cargo, npm, pip, brew, gh and more)",
        priority: 1100,
        matches: |c| !suggestions(&c.output).is_empty(),
        fixes: did_you_mean,
    }]
}

/// How tools phrase a suggestion. Each captures the suggested word.
static PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // gh, go and older cargo: Did you mean `build`?
        r"(?i)did you mean[^\n`'\x22]*[`'\x22]([^`'\x22\s]+)[`'\x22]",
        // cargo and other clap tools: a command with a similar name exists: `build`
        r"(?i)similar (?:name|(?:sub)?command) exists:\s*[`'\x22]([^`'\x22\s]+)[`'\x22]",
        // pip: maybe you meant "install"
        r#"(?i)maybe you meant "([^"]+)""#,
        // npm: Did you mean this?\n    npm install # ...
        r"(?i)did you mean (?:this|one of these)\?\s*\n\s*(?:\S+ )?(\S+)",
        // brew: Did you mean python@3.13 or python@3.12?
        r"(?i)did you mean ([A-Za-z0-9@._+-]+)",
    ]
    .iter()
    .map(|pattern| Regex::new(pattern).unwrap())
    .collect()
});

fn suggestions(output: &str) -> Vec<String> {
    let mut found = Vec::new();
    for pattern in PATTERNS.iter() {
        for capture in pattern.captures_iter(output) {
            let word = capture[1].trim_end_matches(['?', ',', '.']).to_string();
            let filler = matches!(word.to_lowercase().as_str(), "this" | "one" | "these");
            if !word.is_empty() && !filler && !found.contains(&word) {
                found.push(word);
            }
        }
    }
    found
}

/// Swaps the word the tool did not recognize for each suggestion. The word to
/// swap is the argument that looks most like the suggestion.
fn did_you_mean(command: &Command) -> Vec<String> {
    suggestions(&command.output)
        .into_iter()
        .filter_map(|right| {
            let wrong = command
                .parts
                .iter()
                .skip(1)
                .filter(|part| !part.starts_with('-'))
                .max_by(|a, b| similarity(a, &right).total_cmp(&similarity(b, &right)))?;
            (similarity(wrong, &right) >= 0.4 && *wrong != right)
                .then(|| command.replace_word(wrong, &right))
        })
        .collect()
}

fn similarity(a: &str, b: &str) -> f64 {
    strsim::normalized_damerau_levenshtein(a, b)
}

#[cfg(test)]
mod tests {
    use crate::rules::fixes_for;

    #[test]
    fn cargo() {
        let output = "error: no such command: `buidl`\n\n\tDid you mean `build`?\n";
        assert_eq!(
            fixes_for("cargo buidl --release", output),
            ["cargo build --release"]
        );
    }

    #[test]
    fn cargo_current_wording() {
        let output = "error: no such command: `buidl`\n\nhelp: a command with a similar name exists: `build`\n";
        assert_eq!(fixes_for("cargo buidl", output), ["cargo build"]);
    }

    #[test]
    fn pip() {
        let output = "ERROR: unknown command \"instal\" - maybe you meant \"install\"";
        assert_eq!(
            fixes_for("pip instal requests", output),
            ["pip install requests"]
        );
    }

    #[test]
    fn npm() {
        let output = "Unknown command: \"isntall\"\n\nDid you mean this?\n    npm install # Install a package\n";
        assert_eq!(
            fixes_for("npm isntall react", output),
            ["npm install react"]
        );
    }

    #[test]
    fn brew_formula() {
        let output = "Warning: No available formula with the name \"pythn\". Did you mean python@3.13 or python@3.12?";
        assert_eq!(
            fixes_for("brew install pythn", output)[0],
            "brew install python@3.13"
        );
    }
}

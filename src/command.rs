/// A command the user ran, with the output it printed.
#[derive(Debug, Clone)]
pub struct Command {
    pub script: String,
    pub parts: Vec<String>,
    pub output: String,
}

impl Command {
    pub fn new(script: &str, output: &str) -> Self {
        let script = script.trim().to_string();
        let parts = shell_words::split(&script)
            .unwrap_or_else(|_| script.split_whitespace().map(String::from).collect());
        Self {
            script,
            parts,
            output: output.to_string(),
        }
    }

    /// The program name, skipping a leading `sudo`.
    pub fn program(&self) -> &str {
        let mut parts = self.parts.iter();
        match parts.next().map(String::as_str) {
            Some("sudo") => parts.next().map(String::as_str).unwrap_or(""),
            Some(first) => first,
            None => "",
        }
    }

    pub fn is(&self, program: &str) -> bool {
        self.program() == program
    }

    pub fn output_lower(&self) -> String {
        self.output.to_lowercase()
    }

    /// The script with the first `old` word swapped for `new`. `new` goes in
    /// as written, so it may hold several words or flags.
    pub fn replace_word(&self, old: &str, new: &str) -> String {
        let mut done = false;
        let parts: Vec<String> = self
            .parts
            .iter()
            .map(|part| {
                if !done && part == old {
                    done = true;
                    new.to_string()
                } else {
                    shell_words::quote(part).into_owned()
                }
            })
            .collect();
        parts.join(" ")
    }
}

/// Picks the command to fix out of recent history: the newest entry that
/// is not a call to the alias itself.
pub fn last_command<'a>(history: &'a [&'a str], alias: &str) -> Option<&'a str> {
    history.iter().rev().map(|line| line.trim()).find(|line| {
        if line.is_empty() {
            return false;
        }
        let first = line.split_whitespace().next().unwrap_or("");
        first != alias && first != "welp"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_skips_sudo() {
        assert_eq!(Command::new("sudo apt install x", "").program(), "apt");
        assert_eq!(Command::new("git push", "").program(), "git");
    }

    #[test]
    fn last_command_skips_the_alias() {
        let history = ["ls", "gti status", "fuck"];
        assert_eq!(last_command(&history, "fuck"), Some("gti status"));
    }

    #[test]
    fn replace_word_keeps_quoting() {
        let cmd = Command::new("git comit -m 'two words'", "");
        assert_eq!(
            cmd.replace_word("comit", "commit"),
            "git commit -m 'two words'"
        );
    }
}

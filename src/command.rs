/// A command the user ran, with the output it printed.
#[derive(Debug, Clone)]
pub struct Command {
    pub script: String,
    pub parts: Vec<String>,
    pub output: String,
    /// Recent commands, oldest first, without the one being fixed. Empty
    /// unless the shell passed history in.
    pub history: Vec<String>,
}

impl Command {
    pub fn new(script: &str, output: &str) -> Self {
        let script = script.trim().to_string();
        let parts = if windows_style() {
            split_windows(&script)
        } else {
            shell_words::split(&script)
                .unwrap_or_else(|_| script.split_whitespace().map(String::from).collect())
        };
        Self {
            script,
            parts,
            output: output.to_string(),
            history: Vec::new(),
        }
    }

    pub fn with_history(mut self, history: Vec<String>) -> Self {
        self.history = history;
        self
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
                    quote(part)
                }
            })
            .collect();
        parts.join(" ")
    }
}

/// Quotes a word for the user's shell when it needs it. Windows paths and
/// PowerShell keep their backslashes and only get quotes around spaces.
pub fn quote(word: &str) -> String {
    if windows_style() {
        if word.contains(char::is_whitespace) || word.is_empty() {
            format!("'{}'", word.replace('\'', "''"))
        } else {
            word.to_string()
        }
    } else {
        shell_words::quote(word).into_owned()
    }
}

/// PowerShell and Windows paths use `\\` as a separator, not an escape.
fn windows_style() -> bool {
    cfg!(windows) || std::env::var("WELP_SHELL").is_ok_and(|shell| shell == "powershell")
}

/// Splits on spaces outside quotes, keeping backslashes as they are.
fn split_windows(script: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in script.chars() {
        match (quote, c) {
            (Some(open), _) if c == open => quote = None,
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    parts.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            _ => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        parts.push(current);
    }
    parts
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

/// History lines, oldest first, without blank lines or calls to welp.
pub fn recent(history: &[&str], alias: &str) -> Vec<String> {
    history
        .iter()
        .map(|line| line.trim())
        .filter(|line| {
            let first = line.split_whitespace().next().unwrap_or("");
            !line.is_empty() && first != alias && first != "welp"
        })
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_split_keeps_backslashes() {
        assert_eq!(
            split_windows(r#"python C:\Users\me\foo "My Docs\a b.txt" ''"#),
            ["python", r"C:\Users\me\foo", r"My Docs\a b.txt", ""]
        );
    }

    #[test]
    fn program_skips_sudo() {
        assert_eq!(Command::new("sudo apt install x", "").program(), "apt");
        assert_eq!(Command::new("git push", "").program(), "git");
    }

    #[test]
    fn recent_drops_welp_calls() {
        assert_eq!(
            recent(&["ls", " ", "fuck", "gti status"], "fuck"),
            ["ls", "gti status"]
        );
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

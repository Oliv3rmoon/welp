//! What welp remembers: how many fixes, which rules and which typos. Only
//! rule names and the mistyped words are kept, never your commands.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::voice::{Memory, Voice};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    pub total: u64,
    /// When the first fix happened, in seconds since 1970.
    pub since: u64,
    pub rules: BTreeMap<String, u64>,
    /// `"gti → git"` and how often.
    pub typos: BTreeMap<String, u64>,
}

/// Rules whose fix swaps one mistyped word.
const TYPO_RULES: &[&str] = &["no_command", "git_not_command", "did_you_mean", "sl_ls"];
const ARROW: &str = " → ";

/// `$XDG_STATE_HOME/welp/stats.toml`, or `~/.local/state/welp/stats.toml`.
pub fn path() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("welp/stats.toml")
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .unwrap_or(0)
}

impl Stats {
    pub fn load() -> Self {
        fs::read_to_string(path())
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(text) = toml::to_string(self) {
            let _ = fs::write(path, text);
        }
    }

    /// Counts a fix and returns what the voice needs to know about it.
    pub fn record(&mut self, rule: &str, original: &str, fixed: &str, now: u64) -> Memory {
        if self.total == 0 {
            self.since = now;
        }
        self.total += 1;
        *self.rules.entry(rule.to_string()).or_default() += 1;
        let typo = TYPO_RULES
            .contains(&rule)
            .then(|| typo(original, fixed))
            .flatten()
            .map(|(wrong, right)| {
                let count = self
                    .typos
                    .entry(format!("{wrong}{ARROW}{right}"))
                    .or_default();
                *count += 1;
                (wrong, right, *count)
            });
        Memory {
            total: self.total,
            typo,
        }
    }

    /// The `welp stats` card, one line per entry.
    pub fn card(&self, voice: Voice) -> Vec<String> {
        if self.total == 0 {
            return vec!["no fixes yet. suspicious.".into()];
        }
        let mut lines = vec![
            "your mistakes, lovingly kept".to_string(),
            String::new(),
            format!(
                "{:<14}{} since {}",
                "fixes",
                self.total,
                local_date(self.since)
            ),
        ];
        if let Some((rule, count)) = top(&self.rules, 1).first() {
            lines.push(format!("{:<14}{rule} ({count})", "favorite rule"));
        }
        let typos: Vec<String> = top(&self.typos, 3)
            .iter()
            .map(|(typo, count)| format!("{typo} ({count})"))
            .collect();
        if !typos.is_empty() {
            lines.push(format!("{:<14}{}", "top typos", typos.join(" · ")));
        }
        lines.push(format!("{:<14}{}", "verdict", self.verdict(voice)));
        lines
    }

    fn verdict(&self, voice: Voice) -> &'static str {
        let rule = top(&self.rules, 1)
            .first()
            .map(|(rule, _)| rule.clone())
            .unwrap_or_default();
        let spicy = voice == Voice::Spicy;
        match rule.as_str() {
            rule if rule.starts_with("git") && spicy => "git is a cry for help at this point.",
            rule if rule.starts_with("git") => "you and git have a thing going on.",
            "no_command" | "did_you_mean" | "sl_ls" if spicy => "typing is not your strong suit.",
            "no_command" | "did_you_mean" | "sl_ls" => {
                "your fingers are faster than your brain. respect."
            }
            "sudo" | "unsudo" => "you keep forgetting you're not root.",
            rule if rule.starts_with("cd") || rule.starts_with("mkdir") => "you and folders, huh.",
            _ if spicy => "could be worse. not much, but could be.",
            _ => "honestly? could be worse.",
        }
    }
}

/// The one word that differs between the two commands, if only one does.
fn typo(original: &str, fixed: &str) -> Option<(String, String)> {
    let before: Vec<&str> = original.split_whitespace().collect();
    let after: Vec<&str> = fixed.split_whitespace().collect();
    if before.len() != after.len() {
        return None;
    }
    let mut changed = before.iter().zip(&after).filter(|(a, b)| a != b);
    let (wrong, right) = changed.next()?;
    let wordlike = |word: &str| {
        !word.is_empty() && word.len() <= 24 && !word.contains(['/', '.', '=', '\'', '"'])
    };
    (changed.next().is_none() && wordlike(wrong) && wordlike(right))
        .then(|| (wrong.to_string(), right.to_string()))
}

fn top(counts: &BTreeMap<String, u64>, n: usize) -> Vec<(String, u64)> {
    let mut sorted: Vec<(String, u64)> = counts
        .iter()
        .map(|(key, count)| (key.clone(), *count))
        .collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    sorted.truncate(n);
    sorted
}

/// The date in your time zone, from the system's `date`, falling back to UTC.
fn local_date(seconds: u64) -> String {
    let formatted = [
        vec!["-r".to_string(), seconds.to_string()],
        vec!["-d".to_string(), format!("@{seconds}")],
    ]
    .into_iter()
    .find_map(|args| {
        let output = std::process::Command::new("date")
            .args(&args)
            .arg("+%b %-d, %Y")
            .output()
            .ok()?;
        output.status.success().then(|| {
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_lowercase()
        })
    });
    formatted
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| date(seconds))
}

/// `oct 22, 2026`, from seconds since 1970 (UTC).
fn date(seconds: u64) -> String {
    // Howard Hinnant's days-to-civil algorithm.
    let days = (seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    format!("{} {day}, {year}", MONTHS[(month - 1) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_typos_and_rules() {
        let mut stats = Stats::default();
        stats.record("no_command", "gti status", "git status", 100);
        let memory = stats.record("no_command", "gti push", "git push", 200);
        assert_eq!(memory.total, 2);
        assert_eq!(memory.typo, Some(("gti".into(), "git".into(), 2)));
        assert_eq!(stats.since, 100);
        assert_eq!(stats.rules["no_command"], 2);
        assert_eq!(stats.typos["gti → git"], 2);
    }

    #[test]
    fn keeps_no_paths_or_whole_commands() {
        let mut stats = Stats::default();
        stats.record("sudo", "apt install vim", "sudo apt install vim", 1);
        stats.record("no_command", "cat ~/secret.txt", "cat ~/secret.md", 1);
        assert!(stats.typos.is_empty());
        let saved = toml::to_string(&stats).unwrap();
        assert!(!saved.contains("vim") && !saved.contains("secret"));
    }

    #[test]
    fn card_has_the_highlights() {
        let mut stats = Stats::default();
        for _ in 0..3 {
            stats.record("no_command", "gti status", "git status", 1_792_627_200);
        }
        stats.record("sudo", "apt update", "sudo apt update", 1_792_627_200);
        let card = stats.card(Voice::Chill).join("\n");
        assert!(card.contains("4 since "), "{card}");
        assert!(card.contains("no_command (3)"));
        assert!(card.contains("gti → git (3)"));
        assert!(card.contains("faster than your brain"));
    }

    #[test]
    fn empty_card() {
        assert_eq!(
            Stats::default().card(Voice::Chill),
            ["no fixes yet. suspicious."]
        );
    }

    #[test]
    fn dates() {
        assert_eq!(date(0), "jan 1, 1970");
        assert_eq!(date(1_792_627_200), "oct 22, 2026");
    }
}

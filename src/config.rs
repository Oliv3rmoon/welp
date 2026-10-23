use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Ask before running the fix.
    pub require_confirmation: bool,
    /// Seconds to wait when running a command again to read its output.
    pub wait_command: u64,
    /// The wait for commands listed in `slow_commands`.
    pub wait_slow_command: u64,
    pub slow_commands: Vec<String>,
    /// Commands welp never runs a second time.
    pub no_rerun: Vec<String>,
    /// Rules that are turned off, by name.
    pub exclude_rules: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            require_confirmation: true,
            wait_command: 3,
            wait_slow_command: 15,
            slow_commands: ["lein", "react-native", "gradle", "./gradlew", "vagrant"]
                .map(String::from)
                .to_vec(),
            no_rerun: [
                "rm", "rmdir", "mv", "dd", "mkfs", "shred", "kill", "killall", "pkill", "shutdown",
                "reboot", "halt", "poweroff", "vim", "vi", "nvim", "nano", "emacs", "less", "more",
                "top", "htop", "ssh",
            ]
            .map(String::from)
            .to_vec(),
            exclude_rules: Vec::new(),
        }
    }
}

/// `$XDG_CONFIG_HOME/welp`, or `~/.config/welp` on every platform, where
/// command-line tools (and thefuck) keep their settings.
pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("welp")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

impl Config {
    pub fn load() -> Self {
        fs::read_to_string(config_path())
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }
}

use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Command as Process, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::Config;

/// Runs the command again to read what it printed, unless it is one that
/// should never run twice. Gives up after the configured wait.
pub fn capture_output(script: &str, config: &Config) -> String {
    let program = script
        .split_whitespace()
        .find(|word| *word != "sudo")
        .unwrap_or("");
    if script.trim_start().starts_with("sudo") || config.no_rerun.iter().any(|p| p == program) {
        return String::new();
    }
    let wait = if config.slow_commands.iter().any(|p| p == program) {
        config.wait_slow_command
    } else {
        config.wait_command
    };

    // Its own process group, so a timeout stops everything it started.
    let Ok(mut child) = Process::new("sh")
        .arg("-c")
        .arg(format!("exec 2>&1\n{script}"))
        .process_group(0)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return String::new();
    };

    let mut stdout = child.stdout.take().expect("stdout is piped");
    let reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });

    let deadline = Instant::now() + Duration::from_secs(wait);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            _ => {
                // `--` so Linux's kill reads the negative number as a process group.
                let _ = Process::new("kill")
                    .args(["-KILL", "--", &format!("-{}", child.id())])
                    .status();
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    String::from_utf8_lossy(&reader.join().unwrap_or_default()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_stderr() {
        let output = capture_output("echo oops >&2", &Config::default());
        assert_eq!(output.trim(), "oops");
    }

    #[test]
    fn never_reruns_listed_commands() {
        assert_eq!(
            capture_output("rm -r /tmp/welp-should-not-run", &Config::default()),
            ""
        );
    }

    #[test]
    fn stops_at_the_wait() {
        let config = Config {
            wait_command: 1,
            ..Config::default()
        };
        let start = Instant::now();
        capture_output("sleep 5", &config);
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}

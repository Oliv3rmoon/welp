use std::io::{self, Write};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::Stylize;
use crossterm::terminal;

/// Shows the fixes on the terminal and returns the one the user picks.
/// Everything is drawn on stderr: stdout carries the answer to the shell.
pub fn pick(fixes: &[String], confirm: bool) -> Option<String> {
    let first = fixes.first()?;
    let mut err = io::stderr();
    if !confirm {
        let _ = writeln!(err, "{}", first.as_str().bold().green());
        return Some(first.clone());
    }

    if terminal::enable_raw_mode().is_err() {
        let _ = writeln!(
            err,
            "welp: {first} (no terminal to ask in; `welp fix --yes` runs it)"
        );
        return None;
    }
    let mut index = 0;
    let choice = loop {
        let hint = if fixes.len() > 1 {
            "[enter/↑/↓/ctrl+c]"
        } else {
            "[enter/ctrl+c]"
        };
        let _ = write!(
            err,
            "\r\x1b[2K{} {}",
            fixes[index].as_str().bold().green(),
            hint.dark_grey()
        );
        let _ = err.flush();
        match event::read() {
            Ok(Event::Key(KeyEvent {
                code,
                modifiers,
                kind: KeyEventKind::Press,
                ..
            })) => match code {
                KeyCode::Enter => break Some(fixes[index].clone()),
                KeyCode::Up => index = (index + fixes.len() - 1) % fixes.len(),
                KeyCode::Down | KeyCode::Tab => index = (index + 1) % fixes.len(),
                KeyCode::Esc => break None,
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => break None,
                _ => {}
            },
            Ok(_) => {}
            Err(_) => break None,
        }
    };
    let _ = terminal::disable_raw_mode();
    match &choice {
        Some(fix) => {
            let _ = writeln!(err, "\r\x1b[2K{}", fix.as_str().bold().green());
        }
        None => {
            let _ = writeln!(err, "\r\x1b[2K{}", "Aborted".dark_grey());
        }
    }
    choice
}

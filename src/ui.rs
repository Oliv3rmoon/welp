use std::io::{self, Write};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal;

use crate::rules::Fix;
use crate::style::Paint;
use crate::voice;

/// Shows the fixes on the terminal and returns the one the user picks.
/// Everything is drawn on stderr: stdout carries the answer to the shell.
pub fn pick(fixes: &[Fix], confirm: bool, quiet: bool) -> Option<Fix> {
    let first = fixes.first()?;
    let paint = Paint::from_env();
    let mut err = io::stderr();
    if !confirm {
        chosen(&mut err, paint, &first.command, quiet);
        return Some(first.clone());
    }

    if terminal::enable_raw_mode().is_err() {
        let line = format!(
            "would run: {} (no terminal to ask in. `welp fix --yes` runs it)",
            first.command
        );
        let _ = writeln!(err, "{}", paint.line(&line));
        return None;
    }
    let mut index = 0;
    let choice = loop {
        let _ = write!(
            err,
            "\r\x1b[2K{}",
            paint.line(&format!(
                "{}  {}",
                paint.fix(&fixes[index].command),
                hints(paint, index, fixes.len())
            ))
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
    let _ = write!(err, "\r\x1b[2K");
    match &choice {
        Some(fix) => chosen(&mut err, paint, &fix.command, quiet),
        None => {
            let _ = writeln!(err, "{}", paint.line(&paint.dim(voice::ABORTED)));
        }
    }
    choice
}

/// Says something on its own line, after the badge.
pub fn say(text: &str) {
    let paint = Paint::from_env();
    eprintln!("{}", paint.line(text));
}

fn chosen(err: &mut io::Stderr, paint: Paint, fix: &str, quiet: bool) {
    let _ = writeln!(err, "{}", paint.line(&paint.fix(fix)));
    if let Some(aside) = voice::after_fix(voice::seed(), quiet) {
        let _ = writeln!(err, "{}", paint.aside(aside));
    }
}

fn hints(paint: Paint, index: usize, count: usize) -> String {
    let text = if count > 1 {
        format!("↵ run · ↑↓ more ({}/{count}) · esc nah", index + 1)
    } else {
        "↵ run · esc nah".to_string()
    };
    paint.dim(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_shows_position_only_when_there_is_a_choice() {
        let paint = Paint { color: false };
        assert_eq!(hints(paint, 0, 1), "↵ run · esc nah");
        assert_eq!(hints(paint, 1, 3), "↵ run · ↑↓ more (2/3) · esc nah");
    }
}

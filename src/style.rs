//! How welp looks: a small yellow badge in front of everything it says, the
//! fix in bold green and the rest dimmed. `NO_COLOR` keeps the words and
//! drops the colours.

use crossterm::style::{Color, Stylize};

#[derive(Clone, Copy)]
pub struct Paint {
    pub color: bool,
}

impl Paint {
    /// Colours on unless `NO_COLOR` is set (https://no-color.org).
    pub fn from_env() -> Self {
        let color = std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty());
        Self { color }
    }

    pub fn badge(self) -> String {
        if self.color {
            " welp "
                .with(Color::Black)
                .on(Color::Rgb {
                    r: 255,
                    g: 204,
                    b: 77,
                })
                .bold()
                .to_string()
        } else {
            "welp:".to_string()
        }
    }

    pub fn fix(self, text: &str) -> String {
        if self.color {
            text.bold().green().to_string()
        } else {
            text.to_string()
        }
    }

    pub fn dim(self, text: &str) -> String {
        if self.color {
            text.dark_grey().to_string()
        } else {
            text.to_string()
        }
    }

    /// A line welp says, after its badge.
    pub fn line(self, text: &str) -> String {
        format!("{}  {text}", self.badge())
    }

    /// A second line that sits under the first one's text.
    pub fn aside(self, text: &str) -> String {
        let indent = if self.color { 8 } else { 7 };
        format!("{}{}", " ".repeat(indent), self.dim(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_color_is_plain_text() {
        let paint = Paint { color: false };
        assert_eq!(paint.line(&paint.fix("git status")), "welp:  git status");
        assert_eq!(paint.aside("classic."), "       classic.");
    }

    #[test]
    fn color_has_the_badge() {
        let paint = Paint { color: true };
        let line = paint.line("hi");
        assert!(line.contains(" welp ") && line.contains('\x1b') && line.ends_with("  hi"));
    }
}

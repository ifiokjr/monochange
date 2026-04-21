use anstyle::AnsiColor;
use anstyle::Color;
use anstyle::Style;

/// Cyan bold for headers, command names, and accent text.
pub(crate) fn header() -> Style {
	Style::new()
		.bold()
		.fg_color(Some(Color::Ansi(AnsiColor::BrightCyan)))
}

/// White bold for usage lines and bordered header text.
pub(crate) fn usage() -> Style {
	Style::new()
		.bold()
		.fg_color(Some(Color::Ansi(AnsiColor::BrightWhite)))
}

/// Yellow for CLI flags and literals.
pub(crate) fn literal() -> Style {
	Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightYellow)))
}

/// Magenta for placeholders and values.
pub(crate) fn placeholder() -> Style {
	Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightMagenta)))
}

/// Red bold for errors.
pub(crate) fn error() -> Style {
	Style::new()
		.bold()
		.fg_color(Some(Color::Ansi(AnsiColor::BrightRed)))
}

/// Green for valid states and code snippets.
pub(crate) fn valid() -> Style {
	Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightGreen)))
}

/// Bright black (gray) for muted secondary text.
pub(crate) fn muted() -> Style {
	Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)))
}

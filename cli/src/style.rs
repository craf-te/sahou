//! Shared terminal styling for CLI reports (presentation only — no judgement
//! lives here). Styles are always embedded; `anstream` strips them on non-TTY
//! output, when piped, and under NO_COLOR. Renderers never branch on TTY.

use anstyle::{AnsiColor, Style};

/// state: OK / present / accepted
pub const OK: Style = AnsiColor::Green.on_default();
/// state: failed / missing / blocked / rejecting diagnostics
pub const BAD: Style = AnsiColor::Red.on_default();
/// state: uncertain or degraded (drift, unreachable, timeout)
pub const WARN: Style = AnsiColor::Yellow.on_default();
/// the next action (fix / next / hint labels)
pub const ACTION: Style = AnsiColor::Cyan.on_default();
/// metadata (vantage, seq numbers, captured logs, disclaimers)
pub const META: Style = Style::new().dimmed();
/// headings and verdict titles
pub const HEAD: Style = Style::new().bold();

/// Wrap `text` in `style`'s escape codes (with a reset at the end).
pub fn paint(style: Style, text: impl std::fmt::Display) -> String {
    format!("{style}{text}{style:#}")
}

/// One-column verdict glyph at the start of list-like lines.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Ok,
    Bad,
    Skip,
    Warn,
}

impl Status {
    pub fn glyph(self) -> String {
        match self {
            Status::Ok => paint(OK, "✓"),
            Status::Bad => paint(BAD, "✗"),
            Status::Skip => paint(META, "–"),
            Status::Warn => paint(WARN, "!"),
        }
    }
}

/// `{bold title} {dim meta}` — callers include the "· " separators in `meta`.
pub fn heading(title: &str, meta: &str) -> String {
    format!("{} {}", paint(HEAD, title), paint(META, meta))
}

/// A hanging-indent block:
/// `  <label padded to 9> first-line` + continuation lines aligned to the
/// text column (12). The label column fits the longest label ("captured").
/// The returned string ends with '\n'.
pub fn labeled_block(label: &str, style: Style, lines: &[String]) -> String {
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            out.push_str(&format!(
                "  {} {line}\n",
                paint(style, format!("{label:<9}"))
            ));
        } else {
            out.push_str(&format!("  {:<9} {line}\n", ""));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(s: &str) -> String {
        anstream::adapter::strip_str(s).to_string()
    }

    #[test]
    fn paint_embeds_codes_and_strips_back_to_plain() {
        let s = paint(OK, "hello");
        assert_ne!(s, "hello"); // escape codes are embedded unconditionally
        assert_eq!(plain(&s), "hello");
    }

    #[test]
    fn status_glyphs_render_one_column() {
        assert_eq!(plain(&Status::Ok.glyph()), "✓");
        assert_eq!(plain(&Status::Bad.glyph()), "✗");
        assert_eq!(plain(&Status::Skip.glyph()), "–");
        assert_eq!(plain(&Status::Warn.glyph()), "!");
    }

    #[test]
    fn heading_joins_title_and_meta() {
        assert_eq!(plain(&heading("tap", "· watching 3")), "tap · watching 3");
    }

    #[test]
    fn labeled_block_hangs_continuation_lines() {
        let b = labeled_block("fix", ACTION, &["1. first".into(), "2. second".into()]);
        assert_eq!(plain(&b), "  fix       1. first\n            2. second\n");
    }

    #[test]
    fn labeled_block_fits_the_longest_label() {
        let b = labeled_block("captured", META, &["evidence".into()]);
        assert_eq!(plain(&b), "  captured  evidence\n");
    }
}

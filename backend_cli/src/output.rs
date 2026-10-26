//! Printing: `--json` documents on stdout, and search-result HTML turned into
//! terminal text. Progress, spinners and log lines go to stderr, never here.

use std::io::{IsTerminal, Write};

use serde::Serialize;

use crate::exit::CliResult;

/// Print one JSON document on stdout, compact, as the API's `Json<T>`
/// responses serialize it, so the same request gives the same bytes from
/// `simsapa … --json` and from `simsapa serve`.
pub fn print_json<T: Serialize + ?Sized>(value: &T) -> CliResult {
    let s = serde_json::to_string(value)?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "{}", s)?;
    Ok(())
}

pub fn stdout_is_tty() -> bool {
    std::io::stdout().is_terminal()
}

const BOLD_ON: &str = "\x1b[1m";
const BOLD_OFF: &str = "\x1b[0m";

/// Turn a result snippet's HTML into plain terminal text. The highlighted
/// matches (`<span class='match'>…</span>`) become ANSI bold on a terminal and
/// `**…**` in a pipe; every other tag is dropped, entities are decoded and
/// whitespace runs are collapsed to one space.
pub fn snippet_to_terminal(html: &str, is_tty: bool) -> String {
    let (on, off) = if is_tty { (BOLD_ON, BOLD_OFF) } else { ("**", "**") };

    let mut out = String::with_capacity(html.len());
    // One entry per open <span>: whether it is a match highlight, so the right
    // </span> closes the highlight even if another span sits inside it.
    let mut spans: Vec<bool> = Vec::new();
    let mut rest = html;

    while let Some(lt) = rest.find('<') {
        push_text(&mut out, &rest[..lt]);
        let after = &rest[lt..];
        let Some(gt) = after.find('>') else {
            // An unclosed '<' is text, not a tag.
            push_text(&mut out, after);
            rest = "";
            break;
        };
        let tag = &after[..=gt];
        let lower = tag.to_ascii_lowercase();

        if lower.starts_with("<span") {
            let is_match = is_match_span(&lower);
            if is_match {
                out.push_str(on);
            }
            spans.push(is_match);
        } else if lower.starts_with("</span") {
            if spans.pop() == Some(true) {
                out.push_str(off);
            }
        } else if lower.starts_with("<br") || lower.starts_with("<p") || lower.starts_with("</p") {
            push_text(&mut out, " ");
        }

        rest = &after[gt + 1..];
    }
    push_text(&mut out, rest);

    // A snippet cut off inside a highlight still ends the bold.
    for is_match in spans.into_iter().rev() {
        if is_match {
            out.push_str(off);
        }
    }

    out.trim().to_string()
}

fn is_match_span(lower_tag: &str) -> bool {
    lower_tag.contains("class='match'") || lower_tag.contains("class=\"match\"")
}

/// Append decoded text, collapsing whitespace runs (including one spanning a
/// dropped tag) to a single space.
fn push_text(out: &mut String, raw: &str) {
    if raw.is_empty() {
        return;
    }
    let decoded = html_escape::decode_html_entities(raw);
    for c in decoded.chars() {
        if c.is_whitespace() {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_span_to_markdown_bold_in_a_pipe() {
        let html = "the <span class='match'>anicca</span> perception";
        assert_eq!(snippet_to_terminal(html, false), "the **anicca** perception");
    }

    #[test]
    fn test_match_span_to_ansi_bold_on_a_tty() {
        let html = "the <span class=\"match\">anicca</span> perception";
        assert_eq!(snippet_to_terminal(html, true), "the \x1b[1manicca\x1b[0m perception");
    }

    #[test]
    fn test_other_tags_are_stripped_and_entities_decoded() {
        let html = "<p><b>Evaṁ</b> me sutaṁ &mdash; <i>ekaṁ</i>&nbsp;samayaṁ &amp; <a href='x'>link</a></p>";
        assert_eq!(snippet_to_terminal(html, false), "Evaṁ me sutaṁ — ekaṁ samayaṁ & link");
    }

    #[test]
    fn test_inner_span_does_not_close_the_highlight() {
        let html = "<span class='match'>a <span class='x'>b</span> c</span> d";
        assert_eq!(snippet_to_terminal(html, false), "**a b c** d");
    }

    #[test]
    fn test_unclosed_highlight_is_closed() {
        assert_eq!(snippet_to_terminal("x <span class='match'>cut", false), "x **cut**");
    }

    #[test]
    fn test_whitespace_is_collapsed() {
        assert_eq!(snippet_to_terminal("  a\n\n  b<br/>c  ", false), "a b c");
    }

    #[test]
    fn test_lone_less_than_is_text() {
        assert_eq!(snippet_to_terminal("1 < 2", false), "1 < 2");
    }

    #[test]
    fn test_no_span_markup_survives() {
        let out = snippet_to_terminal("<span class='match'>x</span><span>y</span>", false);
        assert!(!out.contains("<span"));
    }
}

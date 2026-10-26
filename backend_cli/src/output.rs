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

/// Turn a rendered text's HTML (a sutta content block, a dictionary
/// definition) into plain text with its paragraphs: block-level tags start a
/// new line, the end of a paragraph-like block leaves a blank line, every
/// other tag is dropped, entities are decoded, whitespace runs
/// inside a paragraph collapse to one space, and blank-line runs to one blank
/// line. `<script>` / `<style>` bodies are dropped.
pub fn html_to_text(html: &str) -> String {
    const BLOCK_TAGS: [&str; 22] = [
        "p", "div", "br", "h1", "h2", "h3", "h4", "h5", "h6", "li", "ul", "ol",
        "blockquote", "header", "footer", "section", "article", "table", "tr", "dt", "dd", "hr",
    ];
    const PARAGRAPH_TAGS: [&str; 12] = [
        "p", "h1", "h2", "h3", "h4", "h5", "h6", "blockquote", "header", "section", "ul", "ol",
    ];

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut rest = html;
    let mut skip_until: Option<&str> = None;

    let end_line = |current: &mut String, lines: &mut Vec<String>| {
        let line = current.trim().to_string();
        current.clear();
        if line.is_empty() {
            if lines.last().is_some_and(|l| !l.is_empty()) {
                lines.push(String::new());
            }
        } else {
            lines.push(line);
        }
    };

    while let Some(lt) = rest.find('<') {
        if skip_until.is_none() {
            push_text(&mut current, &rest[..lt]);
        }
        let after = &rest[lt..];
        let Some(gt) = after.find('>') else {
            if skip_until.is_none() {
                push_text(&mut current, after);
            }
            rest = "";
            break;
        };
        let lower = after[..=gt].to_ascii_lowercase();
        rest = &after[gt + 1..];

        if let Some(closing) = skip_until {
            if lower.starts_with(closing) {
                skip_until = None;
            }
            continue;
        }
        if lower.starts_with("<script") {
            skip_until = Some("</script");
            continue;
        }
        if lower.starts_with("<style") {
            skip_until = Some("</style");
            continue;
        }

        let name: String = lower
            .trim_start_matches('<')
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if BLOCK_TAGS.contains(&name.as_str()) {
            end_line(&mut current, &mut lines);
            if lower.starts_with("</") && PARAGRAPH_TAGS.contains(&name.as_str()) {
                end_line(&mut current, &mut lines);
            }
        }
    }
    if skip_until.is_none() {
        push_text(&mut current, rest);
    }
    end_line(&mut current, &mut lines);

    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
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
    fn test_html_to_text_keeps_paragraphs() {
        let html = "<div><h1>Title</h1><p>One <b>two</b>\n three.</p><p>Four&nbsp;five</p></div>";
        assert_eq!(html_to_text(html), "Title\n\nOne two three.\n\nFour five");
    }

    #[test]
    fn test_html_to_text_collapses_blank_lines_and_drops_scripts() {
        let html = "<p>a</p><br><br><br><p>b</p><script>var x = '<p>';</script><style>p { }</style>c";
        assert_eq!(html_to_text(html), "a\n\nb\n\nc");
    }

    #[test]
    fn test_html_to_text_inline_tags_join_words() {
        assert_eq!(html_to_text("dhamm<b>āya</b> <span class='x'>ca</span>"), "dhammāya ca");
    }

    #[test]
    fn test_no_span_markup_survives() {
        let out = snippet_to_terminal("<span class='match'>x</span><span>y</span>", false);
        assert!(!out.contains("<span"));
    }
}

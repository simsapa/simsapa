// Shared Markdown-to-export conversion core.
//
// AI/assistant responses arrive as Markdown. The UI renders them via
// `prompt_utils::markdown_to_html`; the Org-Mode and DOCX export paths parse
// the same Markdown once into an mdast AST here and drive their emitters from
// it, so their feature coverage cannot drift apart. See
// tasks/2026-07-19-072729-prd---markdown-conversion-in-exports.md for the
// construct mapping.

use markdown::mdast::{Code, List, Node, Table};
use markdown::ParseOptions;

use crate::prompt_utils::unwrap_fenced_tables;

/// Parse an AI/assistant response into an mdast AST, applying the same
/// fenced-table pre-processing as the UI's `markdown_to_html`. Returns `None`
/// on parse error; callers then emit the raw response text as-is.
pub fn parse_response(text: &str) -> Option<Node> {
    let processed = unwrap_fenced_tables(text);
    markdown::to_mdast(processed.trim(), &ParseOptions::gfm()).ok()
}

/// A flattened inline text run with its accumulated styling flags.
/// Both the Org-Mode and DOCX emitters consume these for paragraphs, list
/// items, table cells and headings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InlineRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link_url: Option<String>,
}

/// Flatten a list of inline mdast children into styled runs. Nested
/// `Strong(Emphasis(..))` combines flags; unknown inline nodes flatten to
/// their plain text.
pub fn inline_runs(children: &[Node]) -> Vec<InlineRun> {
    let mut runs: Vec<InlineRun> = Vec::new();
    collect_inline_runs(children, false, false, &mut runs);
    runs
}

fn collect_inline_runs(children: &[Node], bold: bool, italic: bool, runs: &mut Vec<InlineRun>) {
    for node in children {
        match node {
            Node::Text(t) => push_run(runs, InlineRun {
                text: t.value.clone(),
                bold, italic,
                ..Default::default()
            }),
            Node::Strong(s) => collect_inline_runs(&s.children, true, italic, runs),
            Node::Emphasis(e) => collect_inline_runs(&e.children, bold, true, runs),
            Node::InlineCode(c) => push_run(runs, InlineRun {
                text: c.value.clone(),
                bold, italic,
                code: true,
                ..Default::default()
            }),
            Node::Link(l) => {
                let text = l.children.iter().map(node_plain_text).collect::<String>();
                push_run(runs, InlineRun {
                    text,
                    bold, italic,
                    link_url: Some(l.url.clone()),
                    ..Default::default()
                });
            }
            Node::Break(_) => push_run(runs, InlineRun {
                text: "\n".to_string(),
                bold, italic,
                ..Default::default()
            }),
            other => {
                let text = node_plain_text(other);
                if !text.is_empty() {
                    push_run(runs, InlineRun { text, bold, italic, ..Default::default() });
                }
            }
        }
    }
}

/// Append a run, merging with the previous run when the styling is identical
/// (keeps emitter output free of redundant adjacent markup).
fn push_run(runs: &mut Vec<InlineRun>, run: InlineRun) {
    if run.text.is_empty() {
        return;
    }
    if let Some(last) = runs.last_mut() {
        if last.bold == run.bold
            && last.italic == run.italic
            && last.code == run.code
            && last.link_url.is_none()
            && run.link_url.is_none()
        {
            last.text.push_str(&run.text);
            return;
        }
    }
    runs.push(run);
}

/// Recursively flatten a node to its plain-text content — the universal
/// fallback for unsupported constructs (images, raw HTML, footnotes, ...).
pub fn node_plain_text(node: &Node) -> String {
    match node {
        Node::Text(t) => t.value.clone(),
        Node::InlineCode(c) => c.value.clone(),
        Node::Code(c) => c.value.clone(),
        Node::Html(h) => h.value.clone(),
        Node::Break(_) => "\n".to_string(),
        Node::Image(i) => i.alt.clone(),
        Node::ImageReference(i) => i.alt.clone(),
        Node::ThematicBreak(_) => String::new(),
        other => {
            match other.children() {
                Some(children) => children.iter().map(node_plain_text).collect(),
                None => String::new(),
            }
        }
    }
}

// --- Org-Mode emitter ------------------------------------------------------

/// Convert a Markdown AI/assistant response to Org-Mode markup. On parse
/// failure the trimmed raw text is returned (headline-guarded).
pub fn markdown_to_orgmode(text: &str) -> String {
    match parse_response(text) {
        Some(Node::Root(root)) => {
            let blocks: Vec<String> = root
                .children
                .iter()
                .map(org_block)
                .filter(|s| !s.is_empty())
                .collect();
            blocks.join("\n\n")
        }
        _ => orgmode_raw_fallback(text),
    }
}

/// Raw-text fallback when the response cannot be parsed as Markdown.
fn orgmode_raw_fallback(text: &str) -> String {
    guard_org_headlines(text.trim())
}

/// Prevent emitted body lines from being parsed as Org headlines: a line
/// starting with `*`+ followed by a space gets a zero-width space prefix.
/// (Bold at line start, `*bold*`, has no space after the `*` and is fine.)
fn guard_org_headlines(text: &str) -> String {
    text.lines()
        .map(|line| {
            let stars = line.len() - line.trim_start_matches('*').len();
            if stars > 0 && line[stars..].starts_with(' ') {
                format!("\u{200B}{}", line)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn org_block(node: &Node) -> String {
    match node {
        Node::Paragraph(p) => guard_org_headlines(&org_inline_text(&inline_runs(&p.children))),
        Node::Heading(h) => {
            // Never emit `*`-star headlines from response content: render the
            // heading as a bold standalone line instead, so it cannot
            // interleave with the exporter's own outline structure.
            let text: String = h.children.iter().map(node_plain_text).collect();
            let text = text.trim();
            if text.is_empty() {
                String::new()
            } else {
                format!("*{}*", text)
            }
        }
        Node::List(list) => org_list(list),
        Node::Code(code) => org_code(code),
        Node::Table(table) => org_table(table),
        Node::Blockquote(quote) => {
            let inner: Vec<String> = quote
                .children
                .iter()
                .map(org_block)
                .filter(|s| !s.is_empty())
                .collect();
            format!("#+begin_quote\n{}\n#+end_quote", inner.join("\n\n"))
        }
        Node::ThematicBreak(_) => "-----".to_string(),
        other => guard_org_headlines(node_plain_text(other).trim()),
    }
}

/// Render flattened inline runs as Org inline markup. Whitespace at the run
/// edges is emitted outside the emphasis markers (Org requires the markers to
/// abut non-space characters).
fn org_inline_text(runs: &[InlineRun]) -> String {
    let mut out = String::new();
    for run in runs {
        if let Some(url) = &run.link_url {
            if run.text == *url {
                // Autolink (text == url): the bare url, not `[[url][url]]`.
                out.push_str(url);
            } else {
                out.push_str(&format!("[[{}][{}]]", url, run.text));
            }
        } else if run.code {
            org_push_wrapped(&mut out, &run.text, "~", "~");
        } else if run.bold && run.italic {
            org_push_wrapped(&mut out, &run.text, "*/", "/*");
        } else if run.bold {
            org_push_wrapped(&mut out, &run.text, "*", "*");
        } else if run.italic {
            org_push_wrapped(&mut out, &run.text, "/", "/");
        } else {
            out.push_str(&run.text);
        }
    }
    out
}

fn org_push_wrapped(out: &mut String, text: &str, open: &str, close: &str) {
    let after_lead = text.trim_start();
    let lead = &text[..text.len() - after_lead.len()];
    let trimmed = after_lead.trim_end();
    let trail = &after_lead[trimmed.len()..];
    out.push_str(lead);
    if !trimmed.is_empty() {
        out.push_str(open);
        out.push_str(trimmed);
        out.push_str(close);
    }
    out.push_str(trail);
}

fn org_list(list: &List) -> String {
    let mut out_lines: Vec<String> = Vec::new();
    let mut counter: u64 = u64::from(list.start.unwrap_or(1));
    for item in &list.children {
        let Node::ListItem(li) = item else { continue };
        let marker = if list.ordered {
            let m = format!("{}. ", counter);
            counter += 1;
            m
        } else {
            "- ".to_string()
        };
        // Continuation lines (and nested lists) align under the item's text.
        let cont_pad = " ".repeat(marker.len());
        let mut item_text = String::new();
        for block in &li.children {
            let rendered = org_block(block);
            if rendered.is_empty() {
                continue;
            }
            if !item_text.is_empty() {
                // A nested list attaches directly under its parent item's
                // text; other blocks (loose-item paragraphs, code) are
                // separated by a blank line.
                if matches!(block, Node::List(_)) {
                    item_text.push('\n');
                } else {
                    item_text.push_str("\n\n");
                }
            }
            item_text.push_str(&rendered);
        }
        if item_text.is_empty() {
            out_lines.push(marker.trim_end().to_string());
            continue;
        }
        for (i, line) in item_text.lines().enumerate() {
            if i == 0 {
                out_lines.push(format!("{}{}", marker, line));
            } else if line.is_empty() {
                out_lines.push(String::new());
            } else {
                out_lines.push(format!("{}{}", cont_pad, line));
            }
        }
    }
    out_lines.join("\n")
}

fn org_code(code: &Code) -> String {
    let (open, close) = match code.lang.as_deref() {
        Some(lang) if !lang.is_empty() => (format!("#+begin_src {}", lang), "#+end_src"),
        _ => ("#+begin_example".to_string(), "#+end_example"),
    };
    let mut out = String::new();
    out.push_str(&open);
    out.push('\n');
    for line in code.value.lines() {
        // Org convention: escape body lines starting with `*` or `#+` with a
        // leading comma so they cannot terminate or restructure the block.
        if line.starts_with('*') || line.starts_with("#+") {
            out.push(',');
        }
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(close);
    out
}

fn org_table(table: &Table) -> String {
    let mut lines: Vec<String> = Vec::new();
    for (row_idx, row) in table.children.iter().enumerate() {
        let Node::TableRow(row) = row else { continue };
        let cells: Vec<String> = row
            .children
            .iter()
            .map(|cell| {
                let runs = match cell {
                    Node::TableCell(tc) => inline_runs(&tc.children),
                    _ => Vec::new(),
                };
                // Org has no in-cell pipe escape: replace `|` so a cell
                // cannot break the row.
                org_inline_text(&runs).replace('\n', " ").replace('|', "¦")
            })
            .collect();
        let ncols = cells.len().max(1);
        lines.push(format!("| {} |", cells.join(" | ")));
        if row_idx == 0 {
            lines.push(format!("|{}|", vec!["---"; ncols].join("+")));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_children(text: &str) -> Vec<Node> {
        match parse_response(text) {
            Some(Node::Root(root)) => root.children,
            other => panic!("expected Root node, got: {:?}", other),
        }
    }

    /// First paragraph's inline children of the parsed text.
    fn first_paragraph_children(text: &str) -> Vec<Node> {
        for node in parse_children(text) {
            if let Node::Paragraph(p) = node {
                return p.children;
            }
        }
        panic!("no paragraph found in: {}", text);
    }

    #[test]
    fn test_parse_response_gfm_table() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let children = parse_children(md);
        assert!(matches!(children.first(), Some(Node::Table(_))));
    }

    #[test]
    fn test_parse_response_fenced_table_unwrapped() {
        // Pre-processing strips the code fence wrongly wrapped around a table.
        let md = "```\n| A | B |\n|---|---|\n| 1 | 2 |\n```";
        let children = parse_children(md);
        assert!(matches!(children.first(), Some(Node::Table(_))));
    }

    #[test]
    fn test_inline_runs_basic_styles() {
        let children = first_paragraph_children("**bold** *it* `code` [t](u)");
        let runs = inline_runs(&children);

        assert_eq!(runs.len(), 7);
        assert_eq!(runs[0], InlineRun { text: "bold".into(), bold: true, ..Default::default() });
        assert_eq!(runs[1].text, " ");
        assert_eq!(runs[2], InlineRun { text: "it".into(), italic: true, ..Default::default() });
        assert_eq!(runs[3].text, " ");
        assert_eq!(runs[4], InlineRun { text: "code".into(), code: true, ..Default::default() });
        assert_eq!(runs[5].text, " ");
        assert_eq!(runs[6], InlineRun {
            text: "t".into(),
            link_url: Some("u".into()),
            ..Default::default()
        });
    }

    #[test]
    fn test_inline_runs_nested_bold_italic() {
        let children = first_paragraph_children("**bold *both***");
        let runs = inline_runs(&children);

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0], InlineRun { text: "bold ".into(), bold: true, ..Default::default() });
        assert_eq!(runs[1], InlineRun { text: "both".into(), bold: true, italic: true, ..Default::default() });
    }

    #[test]
    fn test_inline_runs_merges_same_style() {
        // Delete (GFM strikethrough) is outside the supported set: it
        // flattens to plain text and merges with the neighboring text runs.
        let children = first_paragraph_children("before ~~gone~~ after");
        let runs = inline_runs(&children);

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "before gone after");
        assert!(!runs[0].bold);
    }

    #[test]
    fn test_node_plain_text_unsupported_constructs() {
        // Image: alt text is kept.
        let children = first_paragraph_children("An ![alt text](img.png) here");
        let text: String = children.iter().map(node_plain_text).collect();
        assert_eq!(text, "An alt text here");
    }

    #[test]
    fn test_node_plain_text_html_node() {
        let children = parse_children("Text with <b>html</b> inline");
        let text: String = children.iter().map(node_plain_text).collect();
        assert_eq!(text, "Text with <b>html</b> inline");
    }

    // --- Org-Mode emitter ---------------------------------------------------

    #[test]
    fn test_org_inline_emphasis_and_link() {
        let out = markdown_to_orgmode("**bold** *it* `code` [text](https://x.y)");
        assert_eq!(out, "*bold* /it/ ~code~ [[https://x.y][text]]");
    }

    #[test]
    fn test_org_bold_italic_combined() {
        let out = markdown_to_orgmode("**bold *both***");
        // Edge whitespace stays outside the emphasis markers, so the bold
        // run and the bold-italic run are emitted as separate valid markup.
        assert_eq!(out, "*bold* */both/*");
    }

    #[test]
    fn test_org_autolink_bare_url() {
        let out = markdown_to_orgmode("See https://example.org here");
        assert_eq!(out, "See https://example.org here");
    }

    #[test]
    fn test_org_heading_is_bold_line_not_headline() {
        let out = markdown_to_orgmode("## Section Title\n\nBody text.");
        assert_eq!(out, "*Section Title*\n\nBody text.");
    }

    #[test]
    fn test_org_nested_list_indentation() {
        let md = "- alpha\n- beta\n  - inner one\n  - inner two\n- gamma";
        let out = markdown_to_orgmode(md);
        assert_eq!(
            out,
            "- alpha\n- beta\n  - inner one\n  - inner two\n- gamma"
        );
    }

    #[test]
    fn test_org_ordered_list_honors_start() {
        let md = "3. third\n4. fourth";
        let out = markdown_to_orgmode(md);
        assert_eq!(out, "3. third\n4. fourth");
    }

    #[test]
    fn test_org_ordered_list_nested_indent() {
        let md = "1. first\n   1. sub\n2. second";
        let out = markdown_to_orgmode(md);
        assert_eq!(out, "1. first\n   1. sub\n2. second");
    }

    #[test]
    fn test_org_loose_list_item_paragraphs() {
        let md = "- first para\n\n  second para\n\n- next item";
        let out = markdown_to_orgmode(md);
        assert_eq!(out, "- first para\n\n  second para\n- next item");
    }

    #[test]
    fn test_org_table_with_bold_cell() {
        let md = "| A | B |\n|---|---|\n| **x** | y |";
        let out = markdown_to_orgmode(md);
        assert_eq!(out, "| A | B |\n|---+---|\n| *x* | y |");
    }

    #[test]
    fn test_org_table_pipe_in_cell_escaped() {
        let md = "| A | B |\n|---|---|\n| a \\| b | y |";
        let out = markdown_to_orgmode(md);
        assert!(out.contains("| a ¦ b | y |"));
    }

    #[test]
    fn test_org_code_block_with_lang() {
        let out = markdown_to_orgmode("```python\nprint('hi')\n```");
        assert_eq!(out, "#+begin_src python\nprint('hi')\n#+end_src");
    }

    #[test]
    fn test_org_code_block_without_lang_comma_escape() {
        let out = markdown_to_orgmode("```\n* star line\n#+keyword\nplain\n```");
        assert_eq!(
            out,
            "#+begin_example\n,* star line\n,#+keyword\nplain\n#+end_example"
        );
    }

    #[test]
    fn test_org_blockquote_and_thematic_break() {
        let out = markdown_to_orgmode("> quoted text\n\n---");
        assert_eq!(out, "#+begin_quote\nquoted text\n#+end_quote\n\n-----");
    }

    #[test]
    fn test_org_headline_guard_on_fallback() {
        // Exercised directly: `to_mdast` practically cannot fail on plain
        // markdown, so the raw-text fallback is tested at the function level.
        let out = orgmode_raw_fallback("* looks like a headline\nnormal line\n** another");
        assert_eq!(
            out,
            "\u{200B}* looks like a headline\nnormal line\n\u{200B}** another"
        );
    }
}

// Shared Markdown-to-export conversion core.
//
// AI/assistant responses arrive as Markdown. The UI renders them via
// `prompt_utils::markdown_to_html`; the Org-Mode and DOCX export paths parse
// the same Markdown once into an mdast AST here and drive their emitters from
// it, so their feature coverage cannot drift apart. See
// tasks/2026-07-19-072729-prd---markdown-conversion-in-exports.md for the
// construct mapping.

use markdown::mdast::Node;
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
}

//! The HTML writer shared by the Ānandajoti pages: inline text, page markers,
//! note refs, paragraphs and the Notes section, and the page frame.
//!
//! Note refs use the app's footnote markup (`sup.footnote-reference > a`),
//! `id="note-ref-4"` → `href="#note-4"`, and later refs to the same note
//! `note-ref-4-2`, …; each note links back to its first ref
//! (`a.footnote-backref`).

use std::collections::{BTreeMap, BTreeSet};

use ego_tree::NodeRef;
use scraper::{ElementRef, Node};

use super::report::Report;
use super::source::{is_page_marker, note_ref_number, Block, Notes};

pub fn escape(text: &str) -> String {
    html_escape::encode_text(text).into_owned()
}

/// The text of a page-marker span as (before, marker, after). The marker runs
/// from the first `{` or `[` to the last `}` or `]`; the source's grey span
/// sometimes also covers the text around it (`[28.167] –`, a dialogue dash),
/// which stays outside the `pts-ref` span. A marker split over two spans
/// (`{3.` and `465}`) has a bracket on one side only, and that side runs to
/// the end of the text. Whitespace at either end stays outside too.
pub fn split_page_marker(text: &str) -> (&str, &str, &str) {
    let trimmed = text.trim();
    let start_offset = text.len() - text.trim_start().len();
    let mut start = 0;
    let mut end = trimmed.len();
    if let Some(i) = trimmed.find(['{', '[']) {
        start = i;
    }
    if let Some(i) = trimmed.rfind(['}', ']']) {
        end = i + 1;
    }
    if start >= end {
        // No bracket pair in order: keep the whole text as the marker.
        start = 0;
        end = trimmed.len();
    }
    let lead_end = start_offset + start;
    let marker_end = start_offset + end;
    (&text[..lead_end], &text[lead_end..marker_end], &text[marker_end..])
}

/// How a span class is rendered.
pub enum SpanStyle {
    Bold,
    Italic,
    BoldItalic,
    Superscript,
    /// Only size or colour: the text is kept as it is.
    Plain,
    /// Not known: counted in the report.
    Unmapped,
}

/// The class tables of one epub.
pub struct ClassMap {
    /// The gray spans that hold page markers.
    pub page_marker_classes: &'static [&'static str],
    pub span_style: fn(&str) -> SpanStyle,
    /// A paragraph class as (semantic class, known).
    pub paragraph_class: fn(&str) -> (Option<&'static str>, bool),
    /// The span class of the author's own additions (the commentator's notes,
    /// the translator's overview), and the paragraph class a paragraph holding
    /// one gets.
    pub commentator_span_class: &'static str,
    pub commentator_paragraph_class: &'static str,
}

/// The page frame: the whole `content_html`. `header_inner` is the content of
/// `<header>`, `wrapper_class` the class of the `<div>` around `body`.
pub fn frame(article_id: &str, header_inner: &str, wrapper_class: &str, body: &str) -> String {
    let mut html = String::new();
    html.push_str("<div class=\"suttacentral html-text\">\n");
    html.push_str(&format!("<article id=\"{}\" lang=\"en\">\n", article_id));
    html.push_str("<header>\n");
    html.push_str(header_inner);
    html.push_str(&format!("</header>\n<div class=\"{}\">\n", wrapper_class));
    html.push_str(body);
    html.push_str("</div>\n</article>\n</div>\n");
    html
}

pub struct Writer<'r, 'a> {
    pub notes: &'r Notes<'a>,
    pub report: &'r mut Report,
    pub classes: &'r ClassMap,
    /// Every id on the page so far.
    pub ids: BTreeSet<String>,
    /// Refs seen per note, keyed by note target.
    ref_counts: BTreeMap<(String, String), u32>,
    /// Notes in order of first reference.
    pub note_order: Vec<(String, String)>,
    pub note_ref_count: usize,
    /// Where the writer is, for report lines: "007.html: story 1.1".
    pub context: String,
}

impl<'r, 'a> Writer<'r, 'a> {
    pub fn new(
        notes: &'r Notes<'a>,
        report: &'r mut Report,
        classes: &'r ClassMap,
        article_id: &str,
        context: String,
    ) -> Self {
        Writer {
            notes,
            report,
            classes,
            ids: BTreeSet::from([article_id.to_string()]),
            ref_counts: BTreeMap::new(),
            note_order: Vec::new(),
            note_ref_count: 0,
            context,
        }
    }
    /// `base`, or `base-2`, `base-3`, … if it is taken.
    pub fn unique_id(&mut self, base: &str) -> String {
        let mut id = base.to_string();
        let mut k = 2;
        while self.ids.contains(&id) {
            id = format!("{}-{}", base, k);
            k += 1;
        }
        self.ids.insert(id.clone());
        id
    }

    pub fn children(&mut self, out: &mut String, node: NodeRef<Node>) {
        for child in node.children() {
            self.inline(out, child);
        }
    }

    /// Render inline content: text, `<br>`, emphasis, page markers and note
    /// refs. The epub's own anchors are dropped, and links that do not lead
    /// to a note become plain text.
    pub fn inline(&mut self, out: &mut String, node: NodeRef<Node>) {
        let el = match node.value() {
            Node::Text(text) => {
                out.push_str(&escape(text));
                return;
            }
            Node::Element(_) => ElementRef::wrap(node).unwrap(),
            _ => return,
        };
        let name = el.value().name();
        let class = el.value().attr("class").unwrap_or("");
        match name {
            "br" => out.push_str("<br>"),
            "a" => match el.value().attr("href") {
                Some(href) => match self.notes.target_of(href) {
                    Some(target) => self.note_ref(out, &el, target),
                    None => self.children(out, node),
                },
                None => self.children(out, node),
            },
            "span" if is_page_marker(&el, self.classes.page_marker_classes) => {
                let text: String = el.text().collect();
                let (lead, marker, trail) = split_page_marker(&text);
                out.push_str(&escape(lead));
                if !marker.is_empty() {
                    out.push_str("<span class=\"pts-ref\">");
                    out.push_str(&escape(marker));
                    out.push_str("</span>");
                }
                out.push_str(&escape(trail));
            }
            "span" => match (self.classes.span_style)(class) {
                SpanStyle::Bold => self.wrapped(out, node, "<b>", "</b>"),
                SpanStyle::Italic => self.wrapped(out, node, "<i>", "</i>"),
                SpanStyle::BoldItalic => self.wrapped(out, node, "<b><i>", "</i></b>"),
                SpanStyle::Superscript => self.wrapped(out, node, "<sup>", "</sup>"),
                SpanStyle::Plain => self.children(out, node),
                SpanStyle::Unmapped => {
                    self.report.unmapped(format!("<span class=\"{}\">", class));
                    self.children(out, node);
                }
            },
            "b" | "i" | "em" | "strong" | "sup" | "sub" => {
                let (open, close) = (format!("<{}>", name), format!("</{}>", name));
                self.wrapped(out, node, &open, &close);
            }
            _ => {
                self.report.unmapped(format!("<{}> inline", name));
                self.children(out, node);
            }
        }
    }

    pub fn wrapped(&mut self, out: &mut String, node: NodeRef<Node>, open: &str, close: &str) {
        out.push_str(open);
        self.children(out, node);
        out.push_str(close);
    }

    pub fn note_ref(&mut self, out: &mut String, el: &ElementRef, target: (String, String)) {
        let ref_number = note_ref_number(el);
        let Some(note) = self.notes.by_target.get(&target) else {
            self.report.defect(format!(
                "{}: note ref {} points to {}#{}, which is no note",
                self.context,
                ref_number.map(|n| format!("{{{}}}", n)).unwrap_or_else(|| "(no number)".into()),
                target.0,
                target.1
            ));
            let text: String = el.text().collect();
            out.push_str(&format!("<sup>{}</sup>", escape(&text)));
            return;
        };
        if ref_number != Some(note.number) {
            self.report.defect(format!(
                "{}: note ref {:?} points to note {}",
                self.context,
                el.text().collect::<String>(),
                note.number
            ));
        }
        self.note_ref_count += 1;
        let count = self.ref_counts.entry(target.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            self.note_order.push(target);
        }
        let base = if *count == 1 { format!("note-ref-{}", note.number) } else { format!("note-ref-{}-{}", note.number, count) };
        let number = note.number;
        let id = self.unique_id(&base);
        out.push_str(&format!(
            "<sup class=\"footnote-reference\"><a id=\"{}\" href=\"#note-{}\">{}</a></sup>",
            id, number, number
        ));
    }

    /// A block of the text: a paragraph with its semantic class.
    pub fn block(&mut self, out: &mut String, block: &Block) {
        let tag = block.tag();
        if tag != "p" {
            self.report.unmapped(format!("<{} class=\"{}\"> block", tag, block.class()));
            out.push_str("<p>");
            self.children(out, *block.el);
            out.push_str("</p>\n");
            return;
        }
        let (class, known) = (self.classes.paragraph_class)(block.class());
        if !known {
            self.report.unmapped(format!("<p class=\"{}\">", block.class()));
        }
        let mut classes: Vec<&str> = class.into_iter().collect();
        let has_commentator_span = block.el.descendants().any(|n| {
            ElementRef::wrap(n).is_some_and(|e| e.value().name() == "span" && e.value().attr("class") == Some(self.classes.commentator_span_class))
        });
        if has_commentator_span && !classes.contains(&self.classes.commentator_paragraph_class) {
            classes.push(self.classes.commentator_paragraph_class);
        }
        if classes.is_empty() {
            out.push_str("<p>");
        } else {
            out.push_str(&format!("<p class=\"{}\">", classes.join(" ")));
        }
        self.children(out, *block.el);
        out.push_str("</p>\n");
    }

    pub fn notes_section(&mut self, out: &mut String) {
        if self.note_order.is_empty() {
            return;
        }
        out.push_str("<section class=\"notes\">\n<h2>Notes</h2>\n<ol>\n");
        for target in self.note_order.clone() {
            let note = &self.notes.by_target[&target];
            let id = self.unique_id(&format!("note-{}", note.number));
            out.push_str(&format!("<li id=\"{}\" value=\"{}\">", id, note.number));
            let last = note.paragraphs.len() - 1;
            for (i, paragraph) in note.paragraphs.iter().enumerate() {
                let mut text = String::new();
                let skip = if i == 0 { note.first_paragraph_start } else { 0 };
                for child in paragraph.children().skip(skip) {
                    self.inline(&mut text, child);
                }
                out.push_str("<p>");
                out.push_str(text.trim());
                if i == last {
                    out.push_str(&format!(
                        " <a class=\"footnote-backref\" href=\"#note-ref-{}\">↩</a>",
                        note.number
                    ));
                }
                out.push_str("</p>");
            }
            out.push_str("</li>\n");
        }
        out.push_str("</ol>\n</section>\n");
    }
}


//! One vagga page: the HTML stored as the record's `content_html`.
//!
//! The page is framed like the other `/en/anandajoti` records
//! (`div.suttacentral.html-text > article > header`), so the reader's sutta
//! CSS applies. Inside it:
//!
//! - `nav.page-toc`: one entry per story, "Dhp 1 — 1.1 The Story about …";
//! - per story, one `<a id="dhp-vN">` per verse, then `section.story`
//!   (`id="story-1-1"`) with its heading, `h3.story-title-pali`,
//!   `div.story-meta`, `nav.substory-toc` (only when it has sub-stories),
//!   and the text, with `h4.substory-heading` (`id="story-1-1-1a"`);
//! - `section.notes`: the notes the page refers to, in order of first
//!   reference, with the book's numbers.
//!
//! Note refs use the app's footnote markup (`sup.footnote-reference > a`),
//! `id="note-ref-4"` → `href="#note-4"`, and later refs to the same note
//! `note-ref-4-2`, …; each note links back to its first ref
//! (`a.footnote-backref`).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use ego_tree::NodeRef;
use regex::Regex;
use scraper::{ElementRef, Node};

use super::extract::{
    collapse_whitespace, is_page_marker, note_ref_number, Block, BodyItem, Notes, Report, Story, Vagga,
};
use super::page_uid;

pub struct Page {
    pub uid: String,
    pub file_name: String,
    pub html: String,
    pub vagga_number: u32,
    pub title: String,
    pub title_pali: String,
    pub verse_start: u32,
    pub verse_end: u32,
    pub story_count: usize,
    pub note_ref_count: usize,
    pub note_count: usize,
}

static RE_SUBSTORY_LABEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+[a-z]+)\.?(?:\s|$)").unwrap());
static RE_SUBSTORY_PART: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^Part (\d+)\.?(?:\s|$)").unwrap());

fn escape(text: &str) -> String {
    html_escape::encode_text(text).into_owned()
}

/// The label of a sub-story heading, for its anchor: "1a Story of …" →
/// `1a`, "Part 5. Rejection …" → `part-5`.
pub fn substory_label(title: &str) -> Option<String> {
    if let Some(c) = RE_SUBSTORY_LABEL.captures(title) {
        return Some(c[1].to_lowercase());
    }
    RE_SUBSTORY_PART.captures(title).map(|c| format!("part-{}", &c[1]))
}

/// How a paragraph class is rendered: its semantic class, if any, and
/// whether the class is known.
fn paragraph_class(class: &str) -> (Option<&'static str>, bool) {
    match class {
        "p3" | "p5" | "p13" | "p7" | "p14" => (None, true),
        "p6" | "p10" | "p11" => (Some("verse"), true),
        "p8" => (Some("story-past"), true),
        "p9" | "p12" | "p15" => (Some("commentator-note"), true),
        "p16" => (None, true),
        _ => (None, false),
    }
}

/// The green span of the commentator's notes.
const COMMENTATOR_SPAN_CLASS: &str = "t16";

/// Spans that only set size or colour; their text is kept as it is. The note
/// ref classes are listed for the rare span of that class outside a link.
const PLAIN_SPAN_CLASSES: [&str; 14] =
    ["t2", "t3", "t4", "t7", "t8", "t9", "t12", "t13", "t15", "t16", "t19", "t20", "t24", "t26"];
const PLAIN_SPAN_CLASSES_TH: [&str; 7] = ["th12", "th22", "th31", "th33", "th34", "th42", "th44"];

struct Writer<'r, 'a> {
    notes: &'r Notes<'a>,
    report: &'r mut Report,
    ids: BTreeSet<String>,
    /// Refs seen per note, keyed by note target.
    ref_counts: BTreeMap<(String, String), u32>,
    /// Notes in order of first reference.
    note_order: Vec<(String, String)>,
    note_ref_count: usize,
    /// Where the writer is, for report lines: "007.html: story 1.1".
    context: String,
}

impl Writer<'_, '_> {
    /// `base`, or `base-2`, `base-3`, … if it is taken.
    fn unique_id(&mut self, base: &str) -> String {
        let mut id = base.to_string();
        let mut k = 2;
        while self.ids.contains(&id) {
            id = format!("{}-{}", base, k);
            k += 1;
        }
        self.ids.insert(id.clone());
        id
    }

    fn children(&mut self, out: &mut String, node: NodeRef<Node>) {
        for child in node.children() {
            self.inline(out, child);
        }
    }

    /// Render inline content: text, `<br>`, emphasis, page markers and note
    /// refs. The epub's own anchors are dropped, and links that do not lead
    /// to a note become plain text.
    fn inline(&mut self, out: &mut String, node: NodeRef<Node>) {
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
            "span" if is_page_marker(&el) => {
                let text: String = el.text().collect();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    out.push_str(&escape(&text));
                    return;
                }
                let lead = &text[..text.len() - text.trim_start().len()];
                let trail = &text[text.trim_end().len()..];
                out.push_str(&escape(lead));
                out.push_str("<span class=\"pts-ref\">");
                out.push_str(&escape(trimmed));
                out.push_str("</span>");
                out.push_str(&escape(trail));
            }
            "span" => match class {
                "t5" | "t14" | "t28" => self.wrapped(out, node, "<b>", "</b>"),
                "t6" | "t18" | "t23" | "th43" => self.wrapped(out, node, "<i>", "</i>"),
                "t11" => self.wrapped(out, node, "<b><i>", "</i></b>"),
                "t22" => self.wrapped(out, node, "<sup>", "</sup>"),
                _ => {
                    if !PLAIN_SPAN_CLASSES.contains(&class) && !PLAIN_SPAN_CLASSES_TH.contains(&class) {
                        self.report.unmapped(format!("<span class=\"{}\">", class));
                    }
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

    fn wrapped(&mut self, out: &mut String, node: NodeRef<Node>, open: &str, close: &str) {
        out.push_str(open);
        self.children(out, node);
        out.push_str(close);
    }

    fn note_ref(&mut self, out: &mut String, el: &ElementRef, target: (String, String)) {
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
    fn block(&mut self, out: &mut String, block: &Block) {
        let tag = block.tag();
        if tag != "p" {
            self.report.unmapped(format!("<{} class=\"{}\"> block", tag, block.class()));
            out.push_str("<p>");
            self.children(out, *block.el);
            out.push_str("</p>\n");
            return;
        }
        let (class, known) = paragraph_class(block.class());
        if !known {
            self.report.unmapped(format!("<p class=\"{}\">", block.class()));
        }
        let mut classes: Vec<&str> = class.into_iter().collect();
        let has_commentator_span = block.el.descendants().any(|n| {
            ElementRef::wrap(n).is_some_and(|e| e.value().name() == "span" && e.value().attr("class") == Some(COMMENTATOR_SPAN_CLASS))
        });
        if has_commentator_span && !classes.contains(&"commentator-note") {
            classes.push("commentator-note");
        }
        if classes.is_empty() {
            out.push_str("<p>");
        } else {
            out.push_str(&format!("<p class=\"{}\">", classes.join(" ")));
        }
        self.children(out, *block.el);
        out.push_str("</p>\n");
    }

    fn story(&mut self, out: &mut String, story: &Story, story_id: &str, substory_ids: &[String]) {
        for v in &story.verses {
            let id = format!("dhp-v{}", v);
            if self.ids.insert(id.clone()) {
                out.push_str(&format!("<a id=\"{}\"></a>\n", id));
            }
        }
        out.push_str(&format!("<section class=\"story\" id=\"{}\">\n", story_id));
        out.push_str("<h2>");
        self.children(out, *story.heading.el);
        out.push_str("</h2>\n");
        if let Some(pali) = &story.pali_title {
            out.push_str("<h3 class=\"story-title-pali\">");
            self.children(out, *pali.el);
            out.push_str("</h3>\n");
        }
        if !story.meta.is_empty() {
            out.push_str("<div class=\"story-meta\">\n");
            for block in &story.meta {
                self.block(out, block);
            }
            out.push_str("</div>\n");
        }
        let substories: Vec<_> = story.substories().collect();
        if !substories.is_empty() {
            out.push_str("<nav class=\"substory-toc\"><ul>\n");
            for (sub, id) in substories.iter().zip(substory_ids) {
                out.push_str(&format!("<li><a href=\"#{}\">{}</a></li>\n", id, escape(&sub.title)));
            }
            out.push_str("</ul></nav>\n");
        }
        let mut sub_index = 0;
        for item in &story.body {
            match item {
                BodyItem::Block(block) => self.block(out, block),
                BodyItem::SubStory(sub) => {
                    out.push_str(&format!("<h4 class=\"substory-heading\" id=\"{}\">", substory_ids[sub_index]));
                    self.children(out, *sub.heading.el);
                    out.push_str("</h4>\n");
                    sub_index += 1;
                }
            }
        }
        out.push_str("</section>\n");
    }

    fn notes_section(&mut self, out: &mut String) {
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

/// Render one vagga page.
pub fn render_vagga(vagga: &Vagga, verse_start: u32, verse_end: u32, notes: &Notes, report: &mut Report) -> Page {
    let uid = page_uid(verse_start, verse_end);
    let article_id = uid.split('/').next().unwrap().to_string();
    let mut writer = Writer {
        notes,
        report,
        ids: BTreeSet::from([article_id.clone()]),
        ref_counts: BTreeMap::new(),
        note_order: Vec::new(),
        note_ref_count: 0,
        context: format!("{}: vagga {}", vagga.heading.file, vagga.number),
    };

    // Anchor ids first, so that the lists can link to them.
    let mut story_ids: Vec<String> = Vec::new();
    let mut substory_ids: Vec<Vec<String>> = Vec::new();
    for story in &vagga.stories {
        let story_id = writer.unique_id(&format!("story-{}-{}", story.vagga_number, story.number));
        let ids = story
            .substories()
            .enumerate()
            .map(|(k, sub)| {
                let base = match substory_label(&sub.title) {
                    Some(label) => format!("{}-{}", story_id, label),
                    None => format!("{}-sub-{}", story_id, k + 1),
                };
                writer.unique_id(&base)
            })
            .collect();
        story_ids.push(story_id);
        substory_ids.push(ids);
    }

    let mut body = String::new();
    body.push_str("<nav class=\"page-toc\"><ul>\n");
    for (story, id) in vagga.stories.iter().zip(&story_ids) {
        let text = match &story.verse_line {
            Some(line) => format!("{} — {}", line, story.title),
            None => story.title.clone(),
        };
        body.push_str(&format!("<li><a href=\"#{}\">{}</a></li>\n", id, escape(&text)));
    }
    body.push_str("</ul></nav>\n");

    writer.context = format!("{}: vagga {}", vagga.heading.file, vagga.number);
    for block in &vagga.intro {
        writer.block(&mut body, block);
    }
    for ((story, id), sub_ids) in vagga.stories.iter().zip(&story_ids).zip(&substory_ids) {
        writer.context = format!("{}: story {}", story.heading.file, story.label());
        writer.story(&mut body, story, id, sub_ids);
    }
    writer.notes_section(&mut body);

    let mut html = String::new();
    html.push_str("<div class=\"suttacentral html-text\">\n");
    html.push_str(&format!("<article id=\"{}\" lang=\"en\">\n", article_id));
    html.push_str("<header>\n<ul>\n<li class=\"division\">Minor Collection</li>\n");
    html.push_str("<li class=\"subdivision\">Dhamma Verses Commentary</li>\n</ul>\n");
    html.push_str(&format!("<h1>{}</h1>\n", escape(&vagga.title)));
    if !vagga.title_pali.is_empty() {
        html.push_str(&format!("<p class=\"vagga-title-pali\">{}</p>\n", escape(&vagga.title_pali)));
    }
    html.push_str("</header>\n<div class=\"anandajoti-dhp-comm\">\n");
    html.push_str(&body);
    html.push_str("</div>\n</article>\n</div>\n");

    Page {
        file_name: format!("{}.html", article_id.trim_end_matches(".att")),
        uid,
        html,
        vagga_number: vagga.number,
        title: collapse_whitespace(&vagga.title),
        title_pali: vagga.title_pali.clone(),
        verse_start,
        verse_end,
        story_count: vagga.stories.len(),
        note_ref_count: writer.note_ref_count,
        note_count: writer.note_order.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substory_labels() {
        assert_eq!(substory_label("1a Story of the Past: The Wicked Physician").as_deref(), Some("1a"));
        assert_eq!(substory_label("33c. Story of the Present").as_deref(), Some("33c"));
        assert_eq!(substory_label("Part 5. Rejection of Māgandiyā by the Buddha").as_deref(), Some("part-5"));
        assert_eq!(substory_label("Story of the Past: The Brothers"), None);
        assert_eq!(substory_label("The Conversion"), None);
        assert_eq!(substory_label("1 Story"), None);
    }
}

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

use std::sync::LazyLock;

use regex::Regex;

use super::extract::{BodyItem, Story, Vagga};
use super::page_uid;
use crate::bootstrap::anandajoti_epub::page::{escape, frame, ClassMap, SpanStyle, Writer};
use crate::bootstrap::anandajoti_epub::report::Report;
use crate::bootstrap::anandajoti_epub::source::{collapse_whitespace, Notes};

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

fn span_style(class: &str) -> SpanStyle {
    match class {
        "t5" | "t14" | "t28" => SpanStyle::Bold,
        "t6" | "t18" | "t23" | "th43" => SpanStyle::Italic,
        "t11" => SpanStyle::BoldItalic,
        "t22" => SpanStyle::Superscript,
        _ if PLAIN_SPAN_CLASSES.contains(&class) || PLAIN_SPAN_CLASSES_TH.contains(&class) => SpanStyle::Plain,
        _ => SpanStyle::Unmapped,
    }
}

pub static CLASS_MAP: ClassMap = ClassMap {
    page_marker_classes: &super::extract::PAGE_MARKER_CLASSES,
    span_style,
    paragraph_class,
    commentator_span_class: COMMENTATOR_SPAN_CLASS,
    commentator_paragraph_class: "commentator-note",
};

fn story_html(w: &mut Writer, out: &mut String, story: &Story, story_id: &str, substory_ids: &[String]) {
    for v in &story.verses {
        let id = format!("dhp-v{}", v);
        if w.ids.insert(id.clone()) {
            out.push_str(&format!("<a id=\"{}\"></a>\n", id));
        }
    }
    out.push_str(&format!("<section class=\"story\" id=\"{}\">\n", story_id));
    out.push_str("<h2>");
    w.children(out, *story.heading.el);
    out.push_str("</h2>\n");
    if let Some(pali) = &story.pali_title {
        out.push_str("<h3 class=\"story-title-pali\">");
        w.children(out, *pali.el);
        out.push_str("</h3>\n");
    }
    if !story.meta.is_empty() {
        out.push_str("<div class=\"story-meta\">\n");
        for block in &story.meta {
            w.block(out, block);
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
            BodyItem::Block(block) => w.block(out, block),
            BodyItem::SubStory(sub) => {
                out.push_str(&format!("<h4 class=\"substory-heading\" id=\"{}\">", substory_ids[sub_index]));
                w.children(out, *sub.heading.el);
                out.push_str("</h4>\n");
                sub_index += 1;
            }
        }
    }
    out.push_str("</section>\n");
}


/// Render one vagga page.
pub fn render_vagga(vagga: &Vagga, verse_start: u32, verse_end: u32, notes: &Notes, report: &mut Report) -> Page {
    let uid = page_uid(verse_start, verse_end);
    let article_id = uid.split('/').next().unwrap().to_string();
    let mut writer = Writer::new(
        notes,
        report,
        &CLASS_MAP,
        &article_id,
        format!("{}: vagga {}", vagga.heading.file, vagga.number),
    );

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
        story_html(&mut writer, &mut body, story, id, sub_ids);
    }
    writer.notes_section(&mut body);

    let mut header = String::new();
    header.push_str("<ul>\n<li class=\"division\">Minor Collection</li>\n");
    header.push_str("<li class=\"subdivision\">Dhamma Verses Commentary</li>\n</ul>\n");
    header.push_str(&format!("<h1>{}</h1>\n", escape(&vagga.title)));
    if !vagga.title_pali.is_empty() {
        header.push_str(&format!("<p class=\"vagga-title-pali\">{}</p>\n", escape(&vagga.title_pali)));
    }
    let html = frame(&article_id, &header, "anandajoti-dhp-comm", &body);

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

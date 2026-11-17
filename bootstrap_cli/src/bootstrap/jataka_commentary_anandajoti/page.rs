//! One Jātaka page: the HTML stored as the record's `content_html`.
//!
//! The page is framed like the other `/en/anandajoti` records
//! (`div.suttacentral.html-text > article > header`), so the reader's sutta
//! CSS applies. Inside it, in `div.anandajoti-jataka-comm`:
//!
//! - `section.jataka` (`id="ja-62"`) with `h2.jataka-heading` ("Ja 62
//!   Aṇḍabhūtajātaka", the English title after a `<br>`, the heading's note
//!   refs), `nav.section-toc` (only with two or more sections), the
//!   paragraphs, and `h3.section-heading` (`id="ja-536-the-small-wood"`)
//!   for the epub's `h4` headings;
//! - an `<a id="ja-62-v1"></a>` before each numbered Pāli verse;
//! - `section.notes`: the notes the page refers to, in order of first
//!   reference, with the book's numbers.
//!
//! The classes are listed in the module doc (`mod.rs`).

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

use super::extract::{Item, Jataka, Nipata, PAGE_MARKER_CLASSES};
use crate::bootstrap::anandajoti_epub::page::{escape, frame, ClassMap, SpanStyle, Writer};
use crate::bootstrap::anandajoti_epub::report::Report;
use crate::bootstrap::anandajoti_epub::source::{collapse_whitespace, text_without_refs, Notes};

pub struct Page {
    pub file_name: String,
    pub html: String,
    pub number: u32,
    pub title: String,
    pub title_pali: String,
    pub nipata_number: u32,
    pub nipata: String,
    pub verse_count: usize,
    pub note_ref_count: usize,
    pub note_count: usize,
}

static RE_VERSE_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+)\. ").unwrap());

/// The paragraph classes of the Pāli verse lines, which open with "N. ".
const VERSE_CLASSES: [&str; 3] = ["p10", "p21", "p22"];

fn paragraph_class(class: &str) -> (Option<&'static str>, bool) {
    match class {
        "p3" | "p4" | "p5" | "p7" | "p20" => (None, true),
        "p10" | "p21" | "p22" => (Some("verse"), true),
        "p19" => (Some("verse-translation"), true),
        "p8" => (Some("verse-quote"), true),
        "p13" => (Some("list-line"), true),
        "p14" | "p15" | "p23" | "p24" => (Some("indented"), true),
        "p16" => (Some("spacer"), true),
        _ => (None, false),
    }
}

/// The translator's own additions: the overview, identification, Compare and
/// Keywords lines.
const TRANSLATOR_SPAN_CLASS: &str = "t15";

/// Spans that only set size or colour; their text is kept as it is. The note
/// ref classes are listed for the rare span of that class outside a link.
const PLAIN_SPAN_CLASSES: [&str; 52] = [
    "t2", "t3", "t4", "t5", "t6", "t7", "t8", "t9", "t12", "t13", "t15", "t16", "t17", "t19", "t20", "t22", "t23",
    "t24", "t25", "t26", "t27", "t28", "t29", "t30", "t31", "t32", "t36", "t37", "t38", "t40", "t44", "t48", "t49",
    "t50", "t51", "t52", "t53", "t54", "t55", "t56", "t57", "t59", "t61", "th22", "th32", "th33", "th34", "th42",
    "th52", "t1", "t10", "t21",
];

fn span_style(class: &str) -> SpanStyle {
    match class {
        "t14" | "t33" | "t47" => SpanStyle::Bold,
        "t11" | "t34" | "t39" | "t41" | "t42" | "t43" | "t62" | "t63" | "t65" => SpanStyle::Italic,
        "t60" => SpanStyle::BoldItalic,
        "t45" | "t58" | "t64" | "t66" => SpanStyle::Superscript,
        _ if PLAIN_SPAN_CLASSES.contains(&class) => SpanStyle::Plain,
        _ => SpanStyle::Unmapped,
    }
}

pub static CLASS_MAP: ClassMap = ClassMap {
    page_marker_classes: &PAGE_MARKER_CLASSES,
    span_style,
    paragraph_class,
    commentator_span_class: TRANSLATOR_SPAN_CLASS,
    commentator_paragraph_class: "translator-note",
};

/// `The Small Wood` → `the-small-wood`: ASCII letters and digits only.
fn slug(title: &str) -> String {
    let ascii = simsapa_backend::helpers::pali_to_ascii(Some(title)).to_lowercase();
    let mut out = String::new();
    for c in ascii.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// Render one Jātaka page.
pub fn render_jataka(jataka: &Jataka, nipata: &Nipata, notes: &Notes, report: &mut Report) -> Page {
    let number = jataka.number;
    let article_id = format!("ja{}.att", number);
    let mut writer = Writer::new(notes, report, &CLASS_MAP, &article_id, format!("{}: Ja {}", jataka.heading.file, number));

    // Anchor ids first, so that the section list can link to them.
    let section_id = writer.unique_id(&format!("ja-{}", number));
    let sections: Vec<_> = jataka.sections().collect();
    let heading_ids: Vec<String> = sections
        .iter()
        .enumerate()
        .map(|(k, section)| {
            let slug = slug(&section.title);
            let base = if slug.is_empty() { format!("ja-{}-section-{}", number, k + 1) } else { format!("ja-{}-{}", number, slug) };
            writer.unique_id(&base)
        })
        .collect();

    let mut body = String::new();
    body.push_str(&format!("<section class=\"jataka\" id=\"{}\">\n", section_id));

    // The heading: the note refs of the book's heading stay, before and
    // after the line break.
    let (mut refs_before, mut refs_after) = (String::new(), String::new());
    let mut seen_br = false;
    for child in jataka.heading.el.children() {
        let Some(el) = scraper::ElementRef::wrap(child) else { continue };
        if el.value().name() == "br" {
            seen_br = true;
        } else if el.value().name() == "a" && el.value().attr("href").is_some() {
            writer.inline(if seen_br { &mut refs_after } else { &mut refs_before }, child);
        }
    }
    body.push_str(&format!(
        "<h2 class=\"jataka-heading\">Ja {} {}{}<br>{}{}</h2>\n",
        number,
        escape(&jataka.title_pali),
        refs_before,
        escape(&jataka.title),
        refs_after
    ));

    if sections.len() >= 2 {
        body.push_str("<nav class=\"section-toc\"><ul>\n");
        for (section, id) in sections.iter().zip(&heading_ids) {
            body.push_str(&format!("<li><a href=\"#{}\">{}</a></li>\n", id, escape(&section.title)));
        }
        body.push_str("</ul></nav>\n");
    }

    let mut verse_numbers: BTreeSet<u32> = BTreeSet::new();
    let mut verse_count = 0;
    let mut section_index = 0;
    for item in &jataka.items {
        match item {
            Item::Block(block) => {
                writer.context = format!("{}: Ja {}", block.file, number);
                if block.tag() == "p" && VERSE_CLASSES.contains(&block.class()) {
                    let text = collapse_whitespace(&text_without_refs(*block.el, &PAGE_MARKER_CLASSES));
                    if let Some(caps) = RE_VERSE_NUMBER.captures(&text)
                        && let Ok(verse) = caps[1].parse::<u32>()
                    {
                        verse_count += 1;
                        if !verse_numbers.insert(verse) {
                            writer.report.defect(format!("{}: verse {} appears twice", writer.context, verse));
                        }
                        let id = writer.unique_id(&format!("ja-{}-v{}", number, verse));
                        body.push_str(&format!("<a id=\"{}\"></a>\n", id));
                    }
                }
                writer.block(&mut body, block);
            }
            Item::Section(section) => {
                writer.context = format!("{}: Ja {}", section.heading.file, number);
                // Verse numbering restarts in each section.
                verse_numbers.clear();
                body.push_str(&format!("<h3 class=\"section-heading\" id=\"{}\">", heading_ids[section_index]));
                writer.children(&mut body, *section.heading.el);
                body.push_str("</h3>\n");
                section_index += 1;
            }
        }
    }
    body.push_str("</section>\n");
    writer.notes_section(&mut body);

    let nipata_title = format!("Book {}. {}", nipata.roman, nipata.name);
    let mut header = String::new();
    header.push_str("<ul>\n<li class=\"division\">Minor Collection</li>\n");
    header.push_str("<li class=\"subdivision\">The Jātaka Commentary</li>\n");
    header.push_str(&format!("<li class=\"nipata\">{}</li>\n</ul>\n", escape(&nipata_title)));
    header.push_str(&format!("<h1>{}</h1>\n", escape(&jataka.title)));
    header.push_str(&format!("<p class=\"jataka-title-pali\">{}</p>\n", escape(&jataka.title_pali)));
    let html = frame(&article_id, &header, "anandajoti-jataka-comm", &body);

    Page {
        file_name: format!("ja{}.html", number),
        html,
        number,
        title: jataka.title.clone(),
        title_pali: jataka.title_pali.clone(),
        nipata_number: nipata.number,
        nipata: nipata.name.clone(),
        verse_count,
        note_ref_count: writer.note_ref_count,
        note_count: writer.note_order.len(),
    }
}

//! Ajahn Ṭhānissaro's introductions, carried over from the dhammatalks.org
//! records.
//!
//! suttas-core removes the introduction from each of his pages (its
//! `src/sources/than/page.ts`, `bodyStart()`), so a segmented record built
//! from suttas-core does not have it. This module cuts the introduction out
//! of the dhammatalks.org record's HTML by the same rule, so what Simsapa
//! keeps is exactly what suttas-core dropped:
//!
//! - an `h2.intro` section, up to the `* * *` separator or the first other
//!   `h2`, unless a `* * *` comes before it (then the `h2.intro` is a section
//!   heading, "I" and "II" on SN 43);
//! - otherwise the run of `<p>`s after the `<h1>` whose text is mostly in
//!   `<em>`, with the `* * *` among them, stopping at Iti's opening formula
//!   (`p.itiSecPP`).
//!
//! See docs/suttas-core-import.md.

use ego_tree::NodeRef;
use scraper::{ElementRef, Html, Node, Selector};

/// Whether `node` ends an `h2.intro` section.
fn is_intro_end(node: &NodeRef<Node>) -> bool {
    match node.value() {
        Node::Element(el) => {
            (el.name() == "h2" && !el.classes().any(|c| c == "intro"))
                || el.attr("class").is_some_and(|c| c.contains("stars"))
        }
        Node::Comment(c) => c.contains("stars"),
        _ => false,
    }
}

fn is_stars(el: &ElementRef) -> bool {
    el.value().attr("class").is_some_and(|c| c.contains("stars"))
}

fn has_class(el: &ElementRef, class: &str) -> bool {
    el.value().classes().any(|c| c == class)
}

/// The share of a paragraph's text that is inside `<em>`.
fn em_share(p: &ElementRef) -> f64 {
    let em_sel = Selector::parse("em").unwrap();
    let all: usize = p.text().map(|t| t.chars().count()).sum();
    if all == 0 {
        return 0.0;
    }
    let em: usize = p.select(&em_sel).flat_map(|e| e.text()).map(|t| t.chars().count()).sum();
    em as f64 / all as f64
}

/// The introduction of a dhammatalks.org page, as the HTML of its blocks
/// (the `* * *` separators left out), or `None` when the page has none.
pub fn extract_introduction(html: &str) -> Option<String> {
    let doc = Html::parse_fragment(html);
    let h1 = doc.select(&Selector::parse("h1").unwrap()).next()?;
    let mut blocks: Vec<String> = Vec::new();

    let intro = doc.select(&Selector::parse("h2.intro").unwrap()).next();
    let closed = intro.is_some_and(|h2| {
        h2.prev_siblings()
            .filter_map(ElementRef::wrap)
            .any(|el| el.value().name() == "p" && is_stars(&el))
    });

    if let (Some(h2), false) = (intro, closed) {
        blocks.push(h2.html());
        let mut cur = h2.next_sibling();
        while let Some(node) = cur {
            if is_intro_end(&node) {
                break;
            }
            if let Some(el) = ElementRef::wrap(node) {
                blocks.push(el.html());
            }
            cur = node.next_sibling();
        }
    } else {
        for node in h1.next_siblings() {
            let Some(el) = ElementRef::wrap(node) else {
                continue;
            };
            if el.value().name() != "p" || has_class(&el, "itiSecPP") {
                break;
            }
            if is_stars(&el) {
                continue;
            }
            if em_share(&el) <= 0.75 {
                break;
            }
            blocks.push(el.html());
        }
    }

    let joined = blocks.join("\n");
    let text_len: usize = Html::parse_fragment(&joined)
        .root_element()
        .text()
        .map(|t| t.trim().chars().count())
        .sum();
    // A heading alone, with no text under it, is not an introduction.
    let heading_only = blocks.len() <= 1 && intro.is_some() && !closed;
    if text_len == 0 || heading_only {
        None
    } else {
        Some(joined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(body: &str) -> String {
        format!("<div class=\"dhammatalks_org\"><div id=\"X\"><h1>Title<br>Sutta (X 1)</h1>{}</div></div>", body)
    }

    #[test]
    fn test_h2_intro_section_up_to_stars() {
        let html = page(concat!(
            "<h2 id=\"toc_1\" class=\"intro\">Introduction</h2>",
            "<p><em>First intro paragraph.</em></p>",
            "<div class=\"iblock\"><p><em>a list item</em></p></div>",
            "<p>Plain intro text too.</p>",
            "<p class=\"stars\">* * *</p>",
            "<p>“Monks, …” the sutta.</p>",
        ));
        let intro = extract_introduction(&html).unwrap();
        assert!(intro.starts_with("<h2 ") && intro.contains(">Introduction</h2>"), "{}", intro);
        assert!(intro.contains("First intro paragraph."));
        assert!(intro.contains("a list item"));
        assert!(intro.contains("Plain intro text too."));
        assert!(!intro.contains("* * *"));
        assert!(!intro.contains("the sutta"));
    }

    #[test]
    fn test_h2_intro_section_up_to_the_first_other_h2() {
        let html = page("<h2 class=\"intro\">Introduction</h2><p><em>Intro.</em></p><h2>Part I</h2><p>Body.</p>");
        let intro = extract_introduction(&html).unwrap();
        assert!(intro.contains("Intro."));
        assert!(!intro.contains("Part I"));
    }

    #[test]
    fn test_em_run_after_h1() {
        // sn1.20, an5.75: italic paragraphs, a stars line, then the text.
        let html = page(concat!(
            "<!--end suttahead-->",
            "<p><em>The Pali Canon is unique.</em></p>",
            "<p><em>This discourse also contains word play on</em> (kāla)<em> and time.</em></p>",
            "<p class=\"stars\">* * *</p>",
            "<p>“Monks, there are these five types of warriors.”</p>",
        ));
        let intro = extract_introduction(&html).unwrap();
        assert!(intro.contains("The Pali Canon is unique."));
        assert!(intro.contains("word play"));
        assert!(!intro.contains("warriors"));
    }

    #[test]
    fn test_closed_h2_intro_is_a_section_heading() {
        // sn43: a short italic intro, stars, then h2.intro "I" opening the body.
        let html = page(concat!(
            "<p><em>This saṁyutta provides a list of 33 names.</em></p>",
            "<p class=\"stars\">* * *</p>",
            "<h2 class=\"intro\" id=\"toc_1\">I</h2>",
            "<p>“Monks, I will teach you the unfabricated.”</p>",
        ));
        let intro = extract_introduction(&html).unwrap();
        assert_eq!(intro, "<p><em>This saṁyutta provides a list of 33 names.</em></p>");
    }

    #[test]
    fn test_no_introduction() {
        assert_eq!(extract_introduction(&page("<p>“Monks, …”</p><p><em>Italic later.</em></p>")), None);
        // Iti's opening formula is set in <em> but is the text.
        assert_eq!(extract_introduction(&page("<p class=\"itiSecPP\"><em>This was said by the Blessed One.</em></p>")), None);
        assert_eq!(extract_introduction("<p>No heading.</p>"), None);
    }
}

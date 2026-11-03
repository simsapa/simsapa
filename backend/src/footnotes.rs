//! Translator footnotes of segmented (Bilara) texts: the stored form in
//! `sutta_comments.content_json`, and the markup the sutta renderer makes of it.
//!
//! A `sutta_comments` value is either a plain SuttaCentral comment (one string
//! per segment, no number, no position) or one or more notes in this form, in
//! number order:
//!
//! ```html
//! <span class='ssp-fn' data-n='2' data-w='6'>note HTML</span>
//! <span class='ssp-fn ssp-fn-editor' data-n='7'>[Editor's note] …</span>
//! ```
//!
//! `data-n` is the note number, `data-w` the zero-based index of the word the
//! note follows (absent for a whole-segment note), `ssp-fn-editor` marks an
//! editor's note. The bootstrap writes this form with `encode_footnotes()` and
//! the renderer reads it with `parse_comment_value()`, so the two cannot drift.
//!
//! Both kinds render as numbered footnotes: a reference after the word (or at
//! the segment's end), and a list of definitions after the text. See
//! docs/suttas-core-import.md.

use scraper::{Html, Selector};

use crate::logger::warn;

/// One footnote of a segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footnote {
    pub number: u32,
    /// Zero-based index of the word the note follows, counted as
    /// `word_count()` counts. `None`: the note is placed at the segment's end.
    pub word_index: Option<u32>,
    pub editor: bool,
    pub html: String,
}

/// A `sutta_comments` value, told apart by the presence of an `ssp-fn` element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentValue {
    Footnotes(Vec<Footnote>),
    Plain(String),
}

/// The stored form of a segment's notes, in the order given (callers pass them
/// in number order).
pub fn encode_footnotes(notes: &[Footnote]) -> String {
    notes
        .iter()
        .map(|n| {
            let class = if n.editor { "ssp-fn ssp-fn-editor" } else { "ssp-fn" };
            let w = match n.word_index {
                Some(w) => format!(" data-w='{}'", w),
                None => String::new(),
            };
            format!("<span class='{}' data-n='{}'{}>{}</span>", class, n.number, w, n.html)
        })
        .collect()
}

/// Reads a `sutta_comments` value. A value with no `ssp-fn` element is a plain
/// comment. Notes with an unreadable number are skipped with a warning.
pub fn parse_comment_value(value: &str) -> CommentValue {
    if !value.contains("ssp-fn") {
        return CommentValue::Plain(value.to_string());
    }

    let fragment = Html::parse_fragment(value);
    let selector = Selector::parse("span.ssp-fn").expect("valid selector");
    let mut notes = Vec::new();

    for el in fragment.select(&selector) {
        // A note nested in another note's text is part of that text.
        if el.ancestors().filter_map(scraper::ElementRef::wrap).any(|a| a.value().has_class("ssp-fn", scraper::CaseSensitivity::CaseSensitive)) {
            continue;
        }
        let attrs = el.value();
        let number = match attrs.attr("data-n").and_then(|n| n.trim().parse::<u32>().ok()) {
            Some(n) => n,
            None => {
                warn(&format!("Footnote without a readable data-n skipped: {}", el.html()));
                continue;
            }
        };
        let word_index = attrs.attr("data-w").and_then(|w| w.trim().parse::<u32>().ok());
        let editor = attrs.has_class("ssp-fn-editor", scraper::CaseSensitivity::CaseSensitive);
        notes.push(Footnote { number, word_index, editor, html: el.inner_html() });
    }

    if notes.is_empty() {
        CommentValue::Plain(value.to_string())
    } else {
        CommentValue::Footnotes(notes)
    }
}

/// The id prefix that makes footnote ids unique on a page: the record uid
/// lowercased, every character outside `[a-z0-9-]` replaced by `-`. A column
/// index can be appended by the caller when one record is shown twice.
pub fn footnote_id_prefix(uid: &str) -> String {
    uid.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect()
}

fn note_id(kind: &str, prefix: &str, number: u32) -> String {
    if prefix.is_empty() {
        format!("{}-{}", kind, number)
    } else {
        format!("{}-{}-{}", kind, prefix, number)
    }
}

/// The reference markup for one note. The number is the link text, so pages
/// read without the reader's script still show it; the reader moves it into
/// `data-n` (`normalise_footnote_markers()` in src-ts/footnote_modal.ts).
pub fn marker_html(number: u32, id_prefix: &str) -> String {
    format!(
        "<span class='footnote-reference'><a href='#{}' id='{}'>{}</a></span>",
        note_id("fn", id_prefix, number),
        note_id("fr", id_prefix, number),
        number,
    )
}

/// Characters a note marker is placed before when they end the word, so that
/// `disappeared:` renders as `disappeared²:`.
fn is_trailing_punctuation(c: char) -> bool {
    matches!(
        c,
        '.' | ',' | ';' | ':' | '!' | '?' | '’' | '”' | '\'' | '"' | ')' | ']' | '—' | '–' | '…'
    )
}

/// Byte ranges of the words of `text`, counted as suttas-core counts them: the
/// string split on whitespace, empty tokens dropped. Tags are part of the words
/// they touch, and punctuation-only tokens are words.
fn word_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                spans.push((s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        spans.push((s, text.len()));
    }
    spans
}

/// The number of words of a segment, as `word_index` counts them.
pub fn word_count(text: &str) -> usize {
    word_spans(text).len()
}

/// Whether byte position `pos` is inside a tag (`<` seen without its `>`).
fn tag_end_if_inside(text: &str, pos: usize) -> Option<usize> {
    let before = &text[..pos];
    let open = before.rfind('<')?;
    if before[open..].contains('>') {
        return None;
    }
    text[pos..].find('>').map(|i| pos + i + 1)
}

/// Where a note following the word `span` goes: after the word, before its
/// trailing punctuation and closing tags, never inside a tag.
fn position_after_word(text: &str, span: (usize, usize)) -> usize {
    let (start, end) = span;
    let word = &text[start..end];

    // The first byte of the word that is neither markup nor punctuation: the
    // marker never moves before it, so a punctuation-only word keeps its note
    // after it.
    let mut first_letter: Option<usize> = None;
    let mut in_tag = false;
    for (i, c) in word.char_indices() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag && !is_trailing_punctuation(c) => {
                first_letter = Some(start + i);
                break;
            }
            _ => {}
        }
    }

    let mut pos = end;
    if let Some(floor) = first_letter {
        loop {
            let before = &text[floor..pos];
            if before.ends_with('>') {
                // Step back over a closing tag only: `<em>word:</em>` puts the
                // marker inside the emphasis, before the colon.
                match before.rfind("</") {
                    Some(i) if !before[i..].contains(' ') && before[i..].matches('>').count() == 1 => {
                        pos = floor + i;
                    }
                    _ => break,
                }
            } else if let Some(c) = before.chars().next_back().filter(|c| is_trailing_punctuation(*c)) {
                pos -= c.len_utf8();
            } else {
                break;
            }
        }
    }

    // A word that ends inside a tag (a tag with spaces in it splits into
    // several words): move to the tag's end.
    tag_end_if_inside(text, pos).unwrap_or(pos)
}

/// The end of a segment's text, before its trailing whitespace, which joins
/// it to the next segment.
fn segment_end(text: &str) -> usize {
    text.trim_end().len()
}

/// Inserts each note's reference into the segment text, after word
/// `word_index` (or at the segment's end), notes at one position in the order
/// given. A `word_index` past the last word puts the note at the end, with a
/// warning: the text and the notes disagree.
pub fn insert_markers(segment_html: &str, notes: &[Footnote], id_prefix: &str) -> String {
    if notes.is_empty() {
        return segment_html.to_string();
    }

    let spans = word_spans(segment_html);
    let mut placed: Vec<(usize, usize, String)> = notes
        .iter()
        .enumerate()
        .map(|(order, note)| {
            let pos = match note.word_index {
                Some(w) => match spans.get(w as usize) {
                    Some(span) => position_after_word(segment_html, *span),
                    None => {
                        warn(&format!(
                            "Footnote {} ({}): word index {} is past the last word ({} words); placed at the segment's end",
                            note.number, id_prefix, w, spans.len()
                        ));
                        segment_end(segment_html)
                    }
                },
                None => segment_end(segment_html),
            };
            (pos, order, marker_html(note.number, id_prefix))
        })
        .collect();
    placed.sort_by_key(|(pos, order, _)| (*pos, *order));

    let mut out = String::with_capacity(segment_html.len() + placed.len() * 96);
    let mut last = 0;
    for (pos, _, marker) in placed {
        out.push_str(&segment_html[last..pos]);
        out.push_str(&marker);
        last = pos;
    }
    out.push_str(&segment_html[last..]);
    out
}

/// The list of note definitions shown after the text. Empty when there are no
/// notes. Each item's id is the target of its reference's `href`.
///
/// `column_label`: in the multi-column view, the label of the column the notes
/// belong to. It goes into the heading and into `data-label`, which the
/// footnote bar shows beside a note's number when several columns have notes.
pub fn definitions_html(notes: &[Footnote], id_prefix: &str, column_label: Option<&str>) -> String {
    if notes.is_empty() {
        return String::new();
    }
    let items: String = notes
        .iter()
        .map(|n| {
            let class = if n.editor { "ssp-fn-def ssp-fn-editor" } else { "ssp-fn-def" };
            format!(
                "<li id='{}' value='{}' class='{}'>{}</li>",
                note_id("fn", id_prefix, n.number),
                n.number,
                class,
                n.html,
            )
        })
        .collect();
    match column_label {
        Some(label) => format!(
            "<section class='ssp-footnotes' data-label='{}'><h2>Notes: {}</h2><ol>{}</ol></section>",
            label.replace('\'', "&#39;"), label, items
        ),
        None => format!("<section class='ssp-footnotes'><h2>Notes</h2><ol>{}</ol></section>", items),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(number: u32, word_index: Option<u32>) -> Footnote {
        Footnote { number, word_index, editor: false, html: format!("note {}", number) }
    }

    fn m(n: u32) -> String {
        marker_html(n, "p")
    }

    #[test]
    fn test_encode_parse_round_trip() {
        let notes = vec![
            Footnote { number: 2, word_index: Some(6), editor: false, html: "See <em>SN 36:11</em>.".to_string() },
            Footnote { number: 7, word_index: None, editor: true, html: "[Editor's note] Reading uncertain.".to_string() },
        ];
        let encoded = encode_footnotes(&notes);
        assert_eq!(parse_comment_value(&encoded), CommentValue::Footnotes(notes));
    }

    #[test]
    fn test_plain_comment_detected() {
        let v = "Literally “the four bases”. See <a href='https://suttacentral.net/sn47.1'>SN 47.1</a>.";
        assert_eq!(parse_comment_value(v), CommentValue::Plain(v.to_string()));
    }

    #[test]
    fn test_footnote_id_prefix() {
        assert_eq!(footnote_id_prefix("mn1.2/en/Sujato"), "mn1-2-en-sujato");
    }

    #[test]
    fn test_word_index_with_em_inside_segment() {
        let s = "the <em>Tathāgata</em> said ";
        assert_eq!(
            insert_markers(s, &[note(1, Some(1))], "p"),
            format!("the <em>Tathāgata{}</em> said ", m(1))
        );
    }

    #[test]
    fn test_trailing_punctuation() {
        assert_eq!(
            insert_markers("it disappeared: and ", &[note(2, Some(1))], "p"),
            format!("it disappeared{}: and ", m(2))
        );
        assert_eq!(
            insert_markers("at the end, ", &[note(1, Some(2))], "p"),
            format!("at the end{}, ", m(1))
        );
        assert_eq!(
            insert_markers("“Vāsava ,” he said", &[note(3, Some(1))], "p"),
            // A punctuation-only word keeps its note after the whole word.
            format!("“Vāsava ,”{} he said", m(3))
        );
        assert_eq!(
            insert_markers("“Vāsava ,” he said", &[note(3, Some(0))], "p"),
            format!("“Vāsava{} ,” he said", m(3))
        );
    }

    #[test]
    fn test_verse_segment_with_newline() {
        let s = "first line\nsecond line ";
        assert_eq!(
            insert_markers(s, &[note(1, Some(2))], "p"),
            format!("first line\nsecond{} line ", m(1))
        );
    }

    #[test]
    fn test_two_notes_on_one_word_in_number_order() {
        let s = "one two three";
        assert_eq!(
            insert_markers(s, &[note(4, Some(1)), note(5, Some(1))], "p"),
            format!("one two{}{} three", m(4), m(5))
        );
    }

    #[test]
    fn test_none_index_at_end_before_trailing_space() {
        assert_eq!(
            insert_markers("town. ", &[note(1, None)], "p"),
            format!("town.{} ", m(1))
        );
    }

    #[test]
    fn test_index_past_end_goes_to_end() {
        assert_eq!(
            insert_markers("one two", &[note(1, Some(9))], "p"),
            format!("one two{}", m(1))
        );
    }

    #[test]
    fn test_empty_segment() {
        assert_eq!(insert_markers("", &[note(1, None)], "p"), m(1));
        assert_eq!(insert_markers("", &[note(1, Some(0))], "p"), m(1));
    }

    #[test]
    fn test_never_inside_a_tag() {
        // `<a href='x'>link</a>` splits into the words `<a` and `href='x'>link</a>`.
        let s = "see <a href='x'>link</a> here";
        assert_eq!(
            insert_markers(s, &[note(1, Some(1))], "p"),
            format!("see <a href='x'>{}link</a> here", m(1))
        );
    }

    #[test]
    fn test_definitions_html() {
        let notes = vec![
            note(1, None),
            Footnote { number: 2, word_index: None, editor: true, html: "[Editor's note] x".to_string() },
        ];
        let html = definitions_html(&notes, "mn1-en-bodhi", None);
        assert!(html.starts_with("<section class='ssp-footnotes'><h2>Notes</h2><ol>"));
        assert!(html.contains("<li id='fn-mn1-en-bodhi-1' value='1' class='ssp-fn-def'>note 1</li>"));
        assert!(html.contains("<li id='fn-mn1-en-bodhi-2' value='2' class='ssp-fn-def ssp-fn-editor'>"));
        assert_eq!(definitions_html(&[], "p", None), "");
        let labelled = definitions_html(&notes, "p", Some("Bhikkhu Bodhi"));
        assert!(labelled.starts_with("<section class='ssp-footnotes' data-label='Bhikkhu Bodhi'><h2>Notes: Bhikkhu Bodhi</h2>"));
    }
}

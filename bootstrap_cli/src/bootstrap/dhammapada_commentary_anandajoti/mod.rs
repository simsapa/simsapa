//! Ānandajoti's *Dhamma Verses Commentary* (a revised translation of the
//! Dhammapada-aṭṭhakathā) as one sutta page per Dhammapada vagga,
//! `dhp<start>-<end>.att/en/anandajoti`, the reference of the CST commentary
//! record of the same vagga. See docs/dhammapada-commentary-anandajoti.md.
//!
//! Two stages:
//!
//! - **Extract** (`dhammapada-commentary-extract`, run by hand when the epub
//!   changes): `Dhamma-Verses-Comm.epub` → `dhp<start>-<end>.html` per vagga,
//!   `manifest.json` and `extract-report.txt`, in
//!   `bootstrap-assets-resources/dhammapada-commentary-anandajoti/`.
//! - **Import** (a bootstrap step): one `suttas` row per manifest entry.
//!
//! # The epub, as the extraction reads it
//!
//! - The spine (`content.opf`) runs `001.html`–`039.html`, then `notes.html`.
//!   `001`–`006` are front matter (the Prologue, Ganthārambhakathā, is an
//!   `<h2>` in `006`). The vaggas run from `007` to `037`. `038.html` holds only
//!   `<h1>&nbsp;</h1>`, and the Conclusion (Nigamanakathā) is `039.html`.
//! - The vagga files hold only `<p>` and `<h1>`–`<h4>` blocks, with `<a>`,
//!   `<span>` and `<br/>` inside them: no images, tables or inline styles.
//! - A vagga starts at an `<h1>` numbered `N. `. The English and the Pāli
//!   names are split by a `<br/>`, except in vaggas 8, 23, 25 and 26, where
//!   they are split by ", " only (reported).
//! - A story is an `<h2>` numbered `<vagga>.<n> ` (class `h20`, `h21` or
//!   `h22`), always followed by its Pāli title, an `<h3 class="h30">`, then
//!   its metadata: the `p7` paragraphs (`p14` in one story) up to the first
//!   other block. They give the verses (`Dhp 1`, `Dhp 54-55`, once with a
//!   note ref), Burlingame's title, parallels, a summary, the cast, keywords
//!   and a star rating (`**` to `*****`).
//! - A sub-story is an `<h4 class="h40">`, labelled `1a`, `1a.`, `Part 5.` or
//!   not at all (`Story of the Past: …`, `The …`).
//! - A file may begin in the middle of a story (`015.html` at the sub-story
//!   "15b The Novice Tissa"), so content is assigned by heading order, never
//!   by file.
//! - Block classes: `p3`, `p5`, `p13` prose; `p6`, `p10`, `p11` verse, lines
//!   split by `<br/>`; `p8` a story of the past (indented, navy); `p9`, `p12`,
//!   `p15` the commentator's own notes (indented, green, as the Introduction
//!   says).
//! - Span classes: gray `t10`, `t17`, `t21`, `t25` are page markers,
//!   `{1.3}` (PTS) and `[28.146]` (Harvard Oriental Series), often both in
//!   one span, sometimes inside a verse line. Bold `t5`, `t14` (the Pāli
//!   verse), `t28`; italic `t6`, `t18`, `t23`, `th43` (the translator's
//!   additions, as the Introduction says); `t11` bold italic; `t22`
//!   superscript line numbers in citations. `t12`, `t13`, `t16`, `t20` only
//!   set size or colour.
//! - A note reference is `<a href="notes.html#aNNNN"><span …>{N}</span></a>`,
//!   with nine span classes, some inside `<h4>` headings: it is recognised by
//!   its `href`. Each note is referenced once.
//! - `notes.html`: one `<p class="p16">` per note, opening with
//!   `<a id="aNNNN"></a>` and a back-link `<a href="<file>#aNN">{N}</a>`; one
//!   note runs on into a second `p16` without them.
//! - The epub's own `<a id="aNN">` and `<a id="ncxNN">` anchors are dropped.
//! - The Conclusion gives each vagga's story count in words: "In the Chapter
//!   about the Pairs are fourteen stories.", "In the Chapter about
//!   Heedfulness there are nine." and "In the Miscellaneous Chapter there are
//!   nine.", some followed by a note ref, and the total ("the 305 stories").

pub mod extract;
pub mod import;
pub mod page;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

/// The SuttaCentral Dhammapada vagga ranges, in vagga order: the
/// `dhp<start>-<end>/pli/ms` uids, which are also the CST vagga uids of the
/// tipitaka-xml-parser's Dhammapada mapping.
pub const SC_VAGGA_RANGES: [(u32, u32); 26] = [
    (1, 20), (21, 32), (33, 43), (44, 59), (60, 75), (76, 89), (90, 99), (100, 115),
    (116, 128), (129, 145), (146, 156), (157, 166), (167, 178), (179, 196), (197, 208),
    (209, 220), (221, 234), (235, 255), (256, 272), (273, 289), (290, 305), (306, 319),
    (320, 333), (334, 359), (360, 382), (383, 423),
];

/// The uid of a vagga page: `dhp1-20.att/en/anandajoti`.
pub fn page_uid(verse_start: u32, verse_end: u32) -> String {
    format!("dhp{}-{}.att/en/anandajoti", verse_start, verse_end)
}

/// `manifest.json` in the output folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub epub_file: String,
    pub epub_sha256: String,
    pub vaggas: Vec<ManifestVagga>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestVagga {
    pub uid: String,
    pub file: String,
    pub vagga_number: u32,
    /// "1. The Chapter about the Pairs"
    pub title: String,
    /// "Yamakavagga"
    pub title_pali: String,
    pub verse_start: u32,
    pub verse_end: u32,
    pub story_count: usize,
}

pub const MANIFEST_FILE: &str = "manifest.json";
pub const REPORT_FILE: &str = "extract-report.txt";

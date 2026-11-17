//! Ānandajoti's *The Birth Stories (Jātakatthavaṇṇanā) (Revised 2023)*
//! (`The-Jataka.epub`) as one page per Jātaka, `ja<N>.att/en/anandajoti`.
//! See docs/jataka-commentary-anandajoti.md. The epub reading, the writer
//! for inline text and notes, the page frame and the report are shared with
//! the Dhammapada pages in `anandajoti_epub`.
//!
//! # The epub
//!
//! 122 content files (`001`–`122.html`) and three notes files (`notes.html`,
//! `notes2.html`, `notes3.html`), read as one stream of block elements in
//! spine order, never cut by file: seven Jātakas cross a file boundary. The
//! front matter (`001`–`015`: Introduction, Studies, Bibliography,
//! Abbreviations, the dates essay, the Nidānakathā) comes first, then the
//! Jātakas from `019.html`; the epub ends inside Ja 547.
//!
//! | Element | Role |
//! |---|---|
//! | `h1.h10` | Volume ("The Birth Stories, Volume II"): ends a Jātaka |
//! | `h2.h20/h21` | `Book II. Dukanipāta  The Section with Two Verses`: the Nipāta; from Book XII on it ends in the number range "(464-473)" |
//! | `h3.h30/h32` | A Jātaka: `Ja 62 Aṇḍabhūtajātaka{note}<br/>The Birth Story about … (1s)` |
//! | `h3.h32` "Preface", `h5.h51` "End of the First Book" | End a Jātaka; the blocks up to the next Jātaka are skipped |
//! | `h4.h40–h43` | A section inside a Jātaka (99, in ten Jātakas) |
//!
//! The Nipāta of a Jātaka is the last `Book <Roman>.` heading before it. The
//! section label in the heading (`(1s)`, `(Pak)`, `(Mahānipāta)`; it can sit
//! after the Pāli name) is left out of the title; it is unreliable as a
//! Nipāta (some headings have none), so it is only cross-checked.
//!
//! No headings divide the Story of the Present, the Story of the Past, the
//! verses and the connection: a page's sections are the `h4` headings only.
//! Verses are numbered per Jātaka, "N. " opening the Pāli verse paragraph
//! (`p10`, `p21`, `p22`); a Jātaka without such a paragraph (Ja 341) has no
//! verse anchor, and a repeated number is reported and its anchor
//! de-duplicated.
//!
//! # Classes
//!
//! | Class | Rendered as |
//! |---|---|
//! | `p3 p4 p5 p7 p20` | `<p>` |
//! | `p10 p21 p22` | `p.verse`: a line of the Pāli verse |
//! | `p19` | `p.verse-translation`: Ānandajoti's translation of the verse |
//! | `p8` | `p.verse-quote`: a line of verse quoted in the story |
//! | `p13` | `p.list-line` |
//! | `p14 p15 p23 p24` | `p.indented` |
//! | `p16` | `p.spacer`: a stanza break (no-break space only) |
//! | a paragraph with a `t15` span | also `.translator-note`: the overview, identification, Compare and Keywords lines |
//! | span `t18 t35 t46` | page marker, `span.pts-ref` (`[1.1]`, `{1.95}`, `[1.1] {1.95}`) |
//! | span `t14 t33 t47` | `<b>` |
//! | span `t11 t34 t39 t41 t42 t43 t62 t63 t65` | `<i>` |
//! | span `t60` | `<b><i>` |
//! | span `t45 t58 t64 t66` | `<sup>` |
//! | the other spans of the survey | their text (colour and size only) |
//!
//! Anything else is counted as unmapped markup in the report.
//!
//! # Notes
//!
//! 1766 notes numbered 1–1766 across the three notes files, one paragraph
//! each, opening with an anchor and a `{N}` link back to the content file.
//! Every note is referenced once. Note refs also occur inside headings.

pub mod extract;
pub mod import;
pub mod page;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

/// The number of Jātakas in the book.
pub const JATAKA_COUNT: u32 = 547;

/// The uid of a Jātaka page: `ja62.att/en/anandajoti`.
pub fn page_uid(number: u32) -> String {
    format!("ja{}.att/en/anandajoti", number)
}

/// `manifest.json` in the output folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub epub_file: String,
    pub epub_sha256: String,
    pub jatakas: Vec<ManifestJataka>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestJataka {
    pub uid: String,
    pub file: String,
    pub jataka_number: u32,
    /// "The Birth Story about Being Inexperienced"
    pub title: String,
    /// "Aṇḍabhūtajātaka"
    pub title_pali: String,
    pub nipata_number: u32,
    /// "Ekanipāta"
    pub nipata: String,
    pub verse_count: usize,
}

pub const MANIFEST_FILE: &str = "manifest.json";
pub const REPORT_FILE: &str = "extract-report.txt";

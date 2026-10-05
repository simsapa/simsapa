//! The per-node texts of the sutta navigation tree: summaries and context
//! notes, kept in TOML files under `assets/`.
//!
//! Every file is keyed by quoted `node_uid`. The two blurb files are a flat
//! table of `"<node_uid>" = """<text>"""`; the context-notes file has one
//! `["<node_uid>"]` table per node, holding the note and the dates of its
//! origin. See docs/sutta-navigation-tree.md.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;
use std::time::Instant;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::app_settings::{
    NAVIGATION_BLURB_OVERRIDES_TOML, NAVIGATION_BLURB_SUTTACENTRAL_TOML,
    NAVIGATION_CONTEXT_NOTES_TOML,
};
use crate::logger::{error, info};

/// Parse a node-texts file into `node_uid` → unwrapped text.
///
/// An entry whose text is empty is **kept**: whether an empty entry means
/// "nothing to show" or "show nothing here" depends on the file, so the caller
/// decides.
///
/// Any shape other than a flat table of strings is an error for the whole
/// file. That includes an **unquoted key**: TOML reads `sn56.11 = "…"` as a
/// nested table, which is not a string and is rejected here.
pub fn parse_node_texts(toml_text: &str) -> Result<BTreeMap<String, String>> {
    let texts: BTreeMap<String, String> = toml::from_str(toml_text)
        .context("must be a flat table of \"node_uid\" = \"text\" (quote every key)")?;
    Ok(texts
        .into_iter()
        .map(|(uid, text)| (uid, unwrap_note(&text)))
        .collect())
}

/// One entry of the context-notes file: the note a node shows, and when the
/// text it describes originated.
///
/// A year is a plain number with BCE negative (`-500` is 500 BCE). Either
/// bound can be absent: only `from` is "not before", only `to` is "not after",
/// and neither is undated.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextNote {
    pub note: String,
    pub from: Option<i32>,
    pub to: Option<i32>,
    /// The bounds are approximate.
    #[serde(default)]
    pub circa: bool,
}

/// Parse the context-notes file into `node_uid` → entry, the note unwrapped.
///
/// An entry whose note is empty is **kept**, as in [`parse_node_texts`].
///
/// Anything but a table per node is an error for the whole file: an entry
/// without `note`, a flat `"uid" = "text"` string, an **unquoted key**
/// (`[sn56.11]` is the table `sn56` holding the table `11`), and a field this
/// struct does not have — so a misspelled `form = -500` fails the parse
/// instead of leaving the entry undated.
pub fn parse_context_notes(toml_text: &str) -> Result<BTreeMap<String, ContextNote>> {
    let notes: BTreeMap<String, ContextNote> = toml::from_str(toml_text).context(
        "must be one [\"node_uid\"] table per node, with `note` and optionally `from`, `to`, `circa` (quote every key)",
    )?;
    Ok(notes
        .into_iter()
        .map(|(uid, entry)| {
            let note = unwrap_note(&entry.note);
            (uid, ContextNote { note, ..entry })
        })
        .collect())
}

/// The entries whose dates cannot be right, one line each:
/// `"dn": from (-200) is later than to (-500)`.
///
/// It reports and nothing else. An entry with one bound or none is not a
/// defect, and neither is `from == to`, a single year.
pub fn context_note_date_defects(notes: &BTreeMap<String, ContextNote>) -> Vec<String> {
    notes
        .iter()
        .filter_map(|(uid, entry)| match (entry.from, entry.to) {
            (Some(from), Some(to)) if from > to => Some(format!(
                "\"{}\": from ({}) is later than to ({})",
                uid, from, to
            )),
            _ => None,
        })
        .collect()
}

/// Undo the hand-wrapping of a text: the files are written to be edited and
/// reviewed, so a text is wrapped across lines there, and those line breaks are
/// not part of it. Lines are joined with a space; a blank line is a paragraph
/// break.
pub fn unwrap_note(note: &str) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in note.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.join(" "));
                current.clear();
            }
        } else {
            current.extend(words);
        }
    }
    if !current.is_empty() {
        paragraphs.push(current.join(" "));
    }
    paragraphs.join("\n\n")
}

/// The summaries and context notes of the navigation tree, by `node_uid`.
#[derive(Debug, Default)]
pub struct NavigationTexts {
    /// SuttaCentral's blurbs. No empty entries.
    suttacentral: HashMap<String, String>,
    /// The project's corrections to the above. An **empty** entry is kept: it
    /// is what hides a SuttaCentral blurb.
    overrides: HashMap<String, String>,
    /// The project's own notes. No entry with an empty note.
    context_notes: HashMap<String, ContextNote>,
}

impl NavigationTexts {
    /// Build from the text of the three files.
    ///
    /// A file that does not parse is logged, by name, and treated as empty; the
    /// other two are still used. The tree itself does not depend on any of
    /// them, so a broken file costs the texts it held and nothing else.
    pub fn from_toml(suttacentral: &str, overrides: &str, context_notes: &str) -> Self {
        let ignored = |file_name: &str, e: anyhow::Error| {
            error(&format!(
                "navigation_texts: assets/{} does not parse and is ignored: {:#}",
                file_name, e
            ));
        };

        let parse = |file_name: &str, toml_text: &str, keep_empty: bool| -> HashMap<String, String> {
            match parse_node_texts(toml_text) {
                Ok(texts) => texts
                    .into_iter()
                    .filter(|(_, text)| keep_empty || !text.is_empty())
                    .collect(),
                Err(e) => {
                    ignored(file_name, e);
                    HashMap::new()
                }
            }
        };

        let context_notes = match parse_context_notes(context_notes) {
            Ok(notes) => notes
                .into_iter()
                .filter(|(_, entry)| !entry.note.is_empty())
                .collect(),
            Err(e) => {
                ignored("navigation-context-notes.toml", e);
                HashMap::new()
            }
        };

        Self {
            suttacentral: parse("navigation-blurb-suttacentral.toml", suttacentral, false),
            overrides: parse("navigation-blurb-overrides.toml", overrides, true),
            context_notes,
        }
    }

    /// The summary a node shows: its override if there is one — an empty
    /// override meaning none — and SuttaCentral's blurb otherwise.
    pub fn blurb(&self, node_uid: &str) -> Option<&str> {
        match self.overrides.get(node_uid) {
            Some(text) => Some(text.as_str()).filter(|text| !text.is_empty()),
            None => self.suttacentral.get(node_uid).map(|text| text.as_str()),
        }
    }

    pub fn context_note(&self, node_uid: &str) -> Option<&str> {
        self.context_notes.get(node_uid).map(|entry| entry.note.as_str())
    }

    /// Every context note with its dates, by `node_uid`, in no particular
    /// order.
    pub fn dated_context_notes(&self) -> impl Iterator<Item = (&str, &ContextNote)> {
        self.context_notes.iter().map(|(uid, entry)| (uid.as_str(), entry))
    }
}

static NAVIGATION_TEXTS: OnceLock<NavigationTexts> = OnceLock::new();

/// The embedded texts, parsed on the first call and kept for the life of the
/// process. A call that arrives while another thread is parsing waits for that
/// parse instead of starting its own.
///
/// **Not for the startup path**: the parse is the cost of opening the Sutta
/// Navigation window, and is paid when the window is asked for. See
/// docs/startup-sequence-and-caches.md.
pub fn navigation_texts() -> &'static NavigationTexts {
    NAVIGATION_TEXTS.get_or_init(|| {
        let started = Instant::now();
        let texts = NavigationTexts::from_toml(
            NAVIGATION_BLURB_SUTTACENTRAL_TOML,
            NAVIGATION_BLURB_OVERRIDES_TOML,
            NAVIGATION_CONTEXT_NOTES_TOML,
        );
        info(&format!(
            "navigation_texts: parsed in {} ms: {} SuttaCentral blurbs, {} overrides, {} context notes",
            started.elapsed().as_millis(),
            texts.suttacentral.len(),
            texts.overrides.len(),
            texts.context_notes.len()
        ));
        texts
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUTTACENTRAL: &str = r#"
"sn56.11" = "The Buddha's first discourse."
"dn" = "The Long Discourses."
"mn" = "The Middle Discourses."
"#;

    const CONTEXT_NOTES: &str = r#"
["sn56.11"]
note = "Chanted on Āsāḷha Pūjā."

["mn"]
note = ""
"#;

    #[test]
    fn an_override_replaces_adds_and_suppresses() {
        let texts = NavigationTexts::from_toml(
            SUTTACENTRAL,
            r#"
"dn" = """
The corrected
summary.
"""
"cst:tika" = "Sub-commentaries."
"mn" = ""
"#,
            CONTEXT_NOTES,
        );

        // Replace.
        assert_eq!(texts.blurb("dn"), Some("The corrected summary."));
        // Add, on a node SuttaCentral has nothing for.
        assert_eq!(texts.blurb("cst:tika"), Some("Sub-commentaries."));
        // Suppress.
        assert_eq!(texts.blurb("mn"), None);
        // No override: SuttaCentral's.
        assert_eq!(texts.blurb("sn56.11"), Some("The Buddha's first discourse."));
        // No entry anywhere.
        assert_eq!(texts.blurb("sn99.99"), None);
    }

    #[test]
    fn a_context_note_is_looked_up_by_node_uid_and_an_empty_one_is_none() {
        let texts = NavigationTexts::from_toml(SUTTACENTRAL, "", CONTEXT_NOTES);

        assert_eq!(texts.context_note("sn56.11"), Some("Chanted on Āsāḷha Pūjā."));
        assert_eq!(texts.context_note("mn"), None);
        assert_eq!(texts.context_note("dn"), None);
    }

    #[test]
    fn a_file_that_does_not_parse_leaves_the_other_two_working() {
        let broken_overrides =
            NavigationTexts::from_toml(SUTTACENTRAL, "dn = not a string", CONTEXT_NOTES);
        assert_eq!(broken_overrides.blurb("dn"), Some("The Long Discourses."));
        assert_eq!(broken_overrides.context_note("sn56.11"), Some("Chanted on Āsāḷha Pūjā."));

        let broken_suttacentral =
            NavigationTexts::from_toml("not toml at all", r#""dn" = "Ours.""#, CONTEXT_NOTES);
        assert_eq!(broken_suttacentral.blurb("dn"), Some("Ours."));
        assert_eq!(broken_suttacentral.blurb("mn"), None);
        assert_eq!(broken_suttacentral.context_note("sn56.11"), Some("Chanted on Āsāḷha Pūjā."));

        // One bad entry costs the whole file, the good entry with it.
        let broken_notes = NavigationTexts::from_toml(
            SUTTACENTRAL,
            "",
            "[\"sn56.11\"]\nnote = \"A note.\"\n\n[\"dn\"]\nnote = 3\n",
        );
        assert_eq!(broken_notes.blurb("dn"), Some("The Long Discourses."));
        assert_eq!(broken_notes.context_note("sn56.11"), None);
        assert_eq!(broken_notes.dated_context_notes().count(), 0);
    }

    /// The app treats a file that does not parse as empty, so a broken one
    /// would ship as a tree with its texts silently missing. This is what stops
    /// it reaching a build.
    #[test]
    fn the_embedded_files_parse() {
        for (file_name, toml_text) in [
            ("navigation-blurb-suttacentral.toml", NAVIGATION_BLURB_SUTTACENTRAL_TOML),
            ("navigation-blurb-overrides.toml", NAVIGATION_BLURB_OVERRIDES_TOML),
        ] {
            if let Err(e) = parse_node_texts(toml_text) {
                panic!("assets/{} does not parse: {:#}", file_name, e);
            }
        }

        let notes = match parse_context_notes(NAVIGATION_CONTEXT_NOTES_TOML) {
            Ok(notes) => notes,
            Err(e) => panic!("assets/navigation-context-notes.toml does not parse: {:#}", e),
        };
        assert_eq!(notes.len(), 44);
        assert!(notes.values().all(|entry| !entry.note.is_empty()));
    }

    #[test]
    fn a_context_note_table_reads_its_note_and_optional_dates() {
        let notes = parse_context_notes(
            r#"
["kp"]
note = "Only a note."

["ne"]
from = -300
circa = true
note = "Not before."

["pe"]
to = -100
note = "Not after."

["dn"]
from = -500
to = -200
note = """
Origin: 5th–3rd century BCE,
   northern India.

A second paragraph.
"""
"#,
        )
        .unwrap();

        let entry = |note: &str, from, to, circa| ContextNote { note: note.to_string(), from, to, circa };
        assert_eq!(notes.len(), 4);
        assert_eq!(notes["kp"], entry("Only a note.", None, None, false));
        assert_eq!(notes["ne"], entry("Not before.", Some(-300), None, true));
        assert_eq!(notes["pe"], entry("Not after.", None, Some(-100), false));
        assert_eq!(
            notes["dn"],
            entry(
                "Origin: 5th–3rd century BCE, northern India.\n\nA second paragraph.",
                Some(-500),
                Some(-200),
                false
            )
        );
    }

    #[test]
    fn the_dated_entries_are_reachable_and_an_empty_note_is_dropped() {
        let texts = NavigationTexts::from_toml(
            "",
            "",
            "[\"dn\"]\nfrom = -500\nto = -200\nnote = \"Early.\"\n\n[\"mn\"]\nfrom = -500\nnote = \"  \"\n",
        );

        let dated: Vec<(&str, &ContextNote)> = texts.dated_context_notes().collect();
        assert_eq!(dated.len(), 1);
        assert_eq!(dated[0].0, "dn");
        assert_eq!((dated[0].1.from, dated[0].1.to, dated[0].1.circa), (Some(-500), Some(-200), false));
        assert_eq!(texts.context_note("dn"), Some("Early."));
        assert_eq!(texts.context_note("mn"), None);
    }

    #[test]
    fn a_context_notes_file_of_another_shape_fails() {
        // A misspelled field.
        assert!(parse_context_notes("[\"dn\"]\nform = -500\nnote = \"A note.\"\n").is_err());
        // No note.
        assert!(parse_context_notes("[\"dn\"]\nfrom = -500\n").is_err());
        // The flat shape of the blurb files.
        assert!(parse_context_notes(r#""dn" = "A note.""#).is_err());
        // A year that is not a whole number.
        assert!(parse_context_notes("[\"dn\"]\nfrom = \"500 BCE\"\nnote = \"A note.\"\n").is_err());
        // TOML reads this as the table `sn56` holding the table `11`, so the
        // note would land on no node at all.
        assert!(parse_context_notes("[sn56.11]\nnote = \"A note.\"\n").is_err());
        assert!(parse_context_notes("not toml at all").is_err());
    }

    #[test]
    fn a_from_later_than_its_to_is_reported() {
        let notes = parse_context_notes(
            r#"
["an"]
from = -500
note = "One bound."

["dn"]
from = -200
to = -500
note = "Swapped."

["mn"]
from = -250
to = -250
note = "A single year."

["sn"]
note = "Undated."
"#,
        )
        .unwrap();

        assert_eq!(
            context_note_date_defects(&notes),
            vec![r#""dn": from (-200) is later than to (-500)"#.to_string()]
        );
    }

    #[test]
    fn a_file_that_is_not_a_string_map_fails() {
        assert!(parse_node_texts(r#""sn56.11" = 3"#).is_err());
        assert!(parse_node_texts("not toml at all").is_err());
    }

    #[test]
    fn an_unquoted_dotted_key_fails_rather_than_nesting() {
        // TOML reads this as the table `sn56` holding the key `11`, so the text
        // would land on no node at all.
        assert!(parse_node_texts(r#"sn56.11 = "The first discourse.""#).is_err());
    }

    #[test]
    fn a_hand_wrapped_text_is_joined_and_keeps_its_paragraphs() {
        let texts = parse_node_texts(
            "\"dn\" = \"\"\"\nOrigin: 5th–3rd century BCE,\n   northern India.\n\n\nA second\nparagraph.\n\"\"\"\n",
        )
        .unwrap();
        assert_eq!(
            texts["dn"],
            "Origin: 5th–3rd century BCE, northern India.\n\nA second paragraph."
        );
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        let texts = parse_node_texts(r#""sn56.11" = "  The first discourse.  ""#).unwrap();
        assert_eq!(texts["sn56.11"], "The first discourse.");
    }

    #[test]
    fn an_empty_entry_is_returned_as_an_empty_string() {
        let texts = parse_node_texts(
            r#"
"sn56.11" = ""
"sn56.12" = "   "
"sn56.13" = "Text."
"#,
        )
        .unwrap();
        assert_eq!(texts.len(), 3);
        assert_eq!(texts["sn56.11"], "");
        assert_eq!(texts["sn56.12"], "");
    }
}

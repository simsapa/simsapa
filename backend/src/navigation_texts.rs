//! The per-node texts of the sutta navigation tree: summaries and context
//! notes, kept in TOML files under `assets/`.
//!
//! Every such file has one format and one key scheme: a flat table of
//! `"<node_uid>" = """<text>"""`. See docs/sutta-navigation-tree.md.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;
use std::time::Instant;

use anyhow::{Context, Result};

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
    /// The project's own notes. No empty entries.
    context_notes: HashMap<String, String>,
}

impl NavigationTexts {
    /// Build from the text of the three files.
    ///
    /// A file that does not parse is logged, by name, and treated as empty; the
    /// other two are still used. The tree itself does not depend on any of
    /// them, so a broken file costs the texts it held and nothing else.
    pub fn from_toml(suttacentral: &str, overrides: &str, context_notes: &str) -> Self {
        let parse = |file_name: &str, toml_text: &str, keep_empty: bool| -> HashMap<String, String> {
            match parse_node_texts(toml_text) {
                Ok(texts) => texts
                    .into_iter()
                    .filter(|(_, text)| keep_empty || !text.is_empty())
                    .collect(),
                Err(e) => {
                    error(&format!(
                        "navigation_texts: assets/{} does not parse and is ignored: {:#}",
                        file_name, e
                    ));
                    HashMap::new()
                }
            }
        };

        Self {
            suttacentral: parse("navigation-blurb-suttacentral.toml", suttacentral, false),
            overrides: parse("navigation-blurb-overrides.toml", overrides, true),
            context_notes: parse("navigation-context-notes.toml", context_notes, false),
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
        self.context_notes.get(node_uid).map(|text| text.as_str())
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
"sn56.11" = "Chanted on Āsāḷha Pūjā."
"mn" = ""
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

        let broken_notes = NavigationTexts::from_toml(SUTTACENTRAL, "", r#""dn" = 3"#);
        assert_eq!(broken_notes.blurb("dn"), Some("The Long Discourses."));
        assert_eq!(broken_notes.context_note("sn56.11"), None);
    }

    /// The app treats a file that does not parse as empty, so a broken one
    /// would ship as a tree with its texts silently missing. This is what stops
    /// it reaching a build.
    #[test]
    fn the_embedded_files_parse() {
        for (file_name, toml_text) in [
            ("navigation-blurb-suttacentral.toml", NAVIGATION_BLURB_SUTTACENTRAL_TOML),
            ("navigation-blurb-overrides.toml", NAVIGATION_BLURB_OVERRIDES_TOML),
            ("navigation-context-notes.toml", NAVIGATION_CONTEXT_NOTES_TOML),
        ] {
            if let Err(e) = parse_node_texts(toml_text) {
                panic!("assets/{} does not parse: {:#}", file_name, e);
            }
        }
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

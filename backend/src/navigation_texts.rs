//! The per-node texts of the sutta navigation tree: summaries and context
//! notes, kept in TOML files under `assets/`.
//!
//! Every such file has one format and one key scheme: a flat table of
//! `"<node_uid>" = """<text>"""`. See docs/sutta-navigation-tree.md.

use std::collections::BTreeMap;

use anyhow::{Context, Result};

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

#[cfg(test)]
mod tests {
    use super::*;

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

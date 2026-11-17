//! Import helpers shared by the Ānandajoti page importers: the columns copied
//! from the `.att/en/opentipitaka` record, the `content_plain` text, and the
//! epub SHA-256 check.

use std::path::Path;
use std::sync::LazyLock;

use anyhow::Result;
use diesel::prelude::*;
use regex::Regex;

use simsapa_backend::db::appdata_schema::suttas;
use simsapa_backend::helpers::sutta_html_to_plain_text;
use simsapa_backend::logger;

use super::epub::sha256_hex;
use crate::bootstrap::helpers::uid_to_ref;
use crate::bootstrap::SuttaData;

pub const SOURCE_UID: &str = "anandajoti";
pub const COPYRIGHT: &str = "Ānandajoti Bhikkhu";
/// Creative Commons Attribution-ShareAlike 4.0, as the author publishes the
/// books. The epubs themselves state no licence.
pub const LICENSE: Option<&str> = Some("CC BY-SA 4.0");

/// Parts of a page that are left out of `content_plain`: the Notes section, the
/// note reference numbers (which would run into the words around them), and the
/// page and sub-story tables of contents (which repeat the headings). They are
/// matched in the markup `anandajoti_epub::page` writes, which nests none of them.
static RE_NOT_PLAIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)<section class="notes">.*?</section>|<sup class="footnote-reference">.*?</sup>|<nav\b[^>]*>.*?</nav>"#,
    )
    .unwrap()
});

/// The text a page contributes to the Contains and Fulltext indexes.
pub fn page_plain_text(content_html: &str) -> String {
    sutta_html_to_plain_text(&RE_NOT_PLAIN.replace_all(content_html, " "))
}

/// The columns copied from the `.att/en/opentipitaka` record of the same page.
/// The Daw Mya Tin import (`dhammapada_tipitaka.rs`) and the Jātaka import use them too.
#[derive(Debug, Clone, PartialEq, Eq, Queryable)]
pub struct CopiedColumns {
    pub sutta_ref: String,
    pub nikaya: String,
    pub group_path: Option<String>,
    pub group_index: Option<i32>,
    pub order_index: Option<i32>,
    pub sutta_range_group: Option<String>,
    pub sutta_range_start: Option<i32>,
    pub sutta_range_end: Option<i32>,
}

impl CopiedColumns {
    pub(crate) fn load(conn: &mut SqliteConnection, uid: &str) -> Result<Option<Self>> {
        Ok(suttas::table
            .filter(suttas::uid.eq(uid))
            .select((
                suttas::sutta_ref,
                suttas::nikaya,
                suttas::group_path,
                suttas::group_index,
                suttas::order_index,
                suttas::sutta_range_group,
                suttas::sutta_range_start,
                suttas::sutta_range_end,
            ))
            .first::<CopiedColumns>(conn)
            .optional()?)
    }

    /// As the CST import derives them from the vagga reference
    /// (`dhp21-32.att`), with no grouping.
    pub(crate) fn derived(reference: &str) -> Self {
        let sc_code = reference.strip_suffix(".att").unwrap_or(reference);
        let (sutta_range_group, sutta_range_start, sutta_range_end) = SuttaData::parse_range_from_uid(reference);
        CopiedColumns {
            sutta_ref: uid_to_ref(sc_code),
            nikaya: "khuddaka".to_string(),
            group_path: None,
            group_index: None,
            order_index: None,
            sutta_range_group,
            sutta_range_start,
            sutta_range_end,
        }
    }
}

/// Warns, and never fails, when the epub is missing or differs from the one
/// the manifest was made from. `what` names the pages ("the Dhammapada
/// commentary pages"), `advice` the command to re-run.
pub fn check_epub_sha256(epub_file: &Path, manifest_sha256: &str, what: &str, advice: &str) {
    match std::fs::read(epub_file) {
        Ok(bytes) => {
            let sha256 = sha256_hex(&bytes);
            if sha256 != manifest_sha256 {
                logger::warn(&format!(
                    "{} has SHA-256 {}, but {} were extracted from {}: {}",
                    epub_file.display(),
                    sha256,
                    what,
                    manifest_sha256,
                    advice
                ));
            }
        }
        Err(e) => logger::warn(&format!(
            "Cannot read {} to check {} against it ({}): {}",
            epub_file.display(),
            what,
            e,
            advice
        )),
    }
}

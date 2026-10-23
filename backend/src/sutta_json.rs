//! A sutta's text in SuttaCentral's Bilara JSON format, as served by
//! `GET /sutta.json`. Kept Qt-free so the CLI can serve and print the same
//! answer.
//!
//! **Never re-sort `content_json`.** The object's key order *is* the order of
//! the paragraphs in the text, and the segment ids do not sort into it: sorted,
//! `mn1:10.1` follows `mn1:1.6`, and 326 of `mn1/pli/ms`'s 334 segments move.
//! Pass the stored string on unchanged; never round-trip it through
//! `serde_json::Value` (built without `preserve_order`, it sorts the keys) or any
//! other unordered map.
//!
//! See docs/simsapa-localhost-api-search-endpoints.md (`GET /sutta.json`).

use crate::db::appdata::AppdataDbHandle;
use crate::db::appdata_models::Sutta;
use crate::helpers::convert_verse_ref_to_sutta_uid;

/// The outcome of looking up a sutta's `content_json`.
#[derive(Debug, Clone, PartialEq)]
pub enum SuttaContentJson {
    /// `uid` is the record that answered, which can differ from the request
    /// (`mn1` → `mn1/pli/ms`, `dhp182` → `dhp179-196/pli/ms`).
    /// `content_json` is the stored column, unchanged.
    Found { uid: String, content_json: String },
    /// The sutta exists but is stored as HTML (e.g. `pli/cst` texts and most
    /// non-SuttaCentral translations).
    NoContentJson { uid: String },
    NotFound,
}

/// Resolve `uid` to a sutta record without substituting a different text.
///
/// In order: verse references (`dhp182`, `thag179/pli/ms`) are converted, the
/// uid is read exactly, a bare code (`mn1`) is read as `<code>/pli/ms`, and a
/// single reference inside a stored range (`sn45.92/pli/ms` →
/// `sn45.92-95/pli/ms`) is read with the same `/lang/author`.
///
/// Unlike the GUI routes' `lookup_sutta_with_fallback`, a uid that names a
/// translation (`mn1/en/nosuch`) never falls back to the Pāli: a data caller
/// asked for that text, and getting another one silently is wrong.
pub fn resolve_sutta_strict(appdata: &AppdataDbHandle, uid: &str) -> Option<Sutta> {
    let uid = convert_verse_ref_to_sutta_uid(uid.trim());
    if uid.is_empty() {
        return None;
    }
    if let Some(sutta) = appdata.get_sutta(&uid) {
        return Some(sutta);
    }
    let uid = if uid.contains('/') {
        uid
    } else {
        let pli_uid = format!("{}/pli/ms", uid);
        if let Some(sutta) = appdata.get_sutta(&pli_uid) {
            return Some(sutta);
        }
        pli_uid
    };
    // `get_sutta_by_range()` returns *some* record in the range when none has
    // the requested `/lang/author` (`mn1/en/nosuch` → `mn1.att/pli/cst`), so
    // keep its answer only when the suffix matches.
    let suffix = &uid[uid.find('/').unwrap_or(0)..];
    appdata.get_sutta_by_range(&uid).filter(|s| s.uid.ends_with(suffix))
}

/// Look up the Bilara JSON (`suttas.content_json`) for `uid`, resolved with
/// [`resolve_sutta_strict`]. An empty column counts as absent.
pub fn get_sutta_content_json(appdata: &AppdataDbHandle, uid: &str) -> SuttaContentJson {
    match resolve_sutta_strict(appdata, uid) {
        None => SuttaContentJson::NotFound,
        Some(sutta) => match sutta.content_json {
            Some(content_json) if !content_json.trim().is_empty() => SuttaContentJson::Found {
                uid: sutta.uid,
                content_json,
            },
            _ => SuttaContentJson::NoContentJson { uid: sutta.uid },
        },
    }
}

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

/// One record of a sutta's Bilara companion tables (`sutta_variants`,
/// `sutta_comments`, `sutta_glosses`): which sutta it belongs to, and its
/// segment id → text object, the stored column unchanged.
#[derive(Debug, serde::Serialize)]
pub struct BilaraRecord {
    pub uid: String,
    pub language: Option<String>,
    pub source_uid: Option<String>,
    pub content: Box<serde_json::value::RawValue>,
}

/// The `/sutta_bilara.json` body: the sutta's text and every Bilara record
/// published beside it, each `content` object embedded verbatim (as a
/// `RawValue`, so the segment order is never sorted).
#[derive(Debug, serde::Serialize)]
pub struct SuttaBilaraRecords {
    pub uid: String,
    pub language: String,
    pub source_uid: Option<String>,
    pub content: Box<serde_json::value::RawValue>,
    pub html_template: Option<Box<serde_json::value::RawValue>>,
    pub variants: Option<BilaraRecord>,
    pub comments: Option<BilaraRecord>,
    pub glosses: Option<BilaraRecord>,
}

#[derive(Debug)]
pub enum SuttaBilaraJson {
    /// `json` is the serialized [`SuttaBilaraRecords`].
    Found { uid: String, json: String },
    NoContentJson { uid: String },
    NotFound,
}

/// A stored JSON column as a `RawValue`, or `None` when it is empty or does
/// not parse (logged: a malformed column is a data defect, reported, not
/// repaired).
fn raw_json(column: Option<String>, what: &str, uid: &str) -> Option<Box<serde_json::value::RawValue>> {
    let s = column.filter(|s| !s.trim().is_empty())?;
    match serde_json::value::RawValue::from_string(s) {
        Ok(raw) => Some(raw),
        Err(e) => {
            crate::logger::error(&format!("sutta_json: {} of {} is not valid JSON: {}", what, uid, e));
            None
        }
    }
}

/// The text of `uid` (resolved with [`resolve_sutta_strict`]) with its HTML
/// template, variant readings, comments and glosses.
///
/// The variant readings belong to the Pāli root text, so for a translation
/// they are read from `<reference>/pli/ms` when the translation has none of
/// its own; the record's `uid` says which text it belongs to. The segment ids
/// are shared, so they key into the translation all the same.
pub fn get_sutta_bilara_json(appdata: &AppdataDbHandle, uid: &str) -> SuttaBilaraJson {
    use crate::db::appdata_models::{SuttaComment, SuttaGloss, SuttaVariant};
    use crate::db::appdata_schema::{sutta_comments, sutta_glosses, sutta_variants};
    use diesel::prelude::*;

    let Some(sutta) = resolve_sutta_strict(appdata, uid) else {
        return SuttaBilaraJson::NotFound;
    };
    let Some(content) = raw_json(sutta.content_json.clone(), "content_json", &sutta.uid) else {
        return SuttaBilaraJson::NoContentJson { uid: sutta.uid };
    };

    // Each table holds at most one record per sutta.
    let read_variant = |sutta_id: i32| -> Option<BilaraRecord> {
        let r = appdata.do_read(|conn| {
            sutta_variants::table
                .filter(sutta_variants::sutta_id.eq(sutta_id))
                .select(SuttaVariant::as_select())
                .first(conn)
                .optional()
        });
        let v = r.map_err(|e| crate::logger::error(&format!("sutta_json: sutta_variants: {}", e))).ok()??;
        Some(BilaraRecord {
            content: raw_json(v.content_json, "sutta_variants", &v.sutta_uid)?,
            uid: v.sutta_uid,
            language: v.language,
            source_uid: v.source_uid,
        })
    };
    let read_comment = |sutta_id: i32| -> Option<BilaraRecord> {
        let r = appdata.do_read(|conn| {
            sutta_comments::table
                .filter(sutta_comments::sutta_id.eq(sutta_id))
                .select(SuttaComment::as_select())
                .first(conn)
                .optional()
        });
        let c = r.map_err(|e| crate::logger::error(&format!("sutta_json: sutta_comments: {}", e))).ok()??;
        Some(BilaraRecord {
            content: raw_json(c.content_json, "sutta_comments", &c.sutta_uid)?,
            uid: c.sutta_uid,
            language: c.language,
            source_uid: c.source_uid,
        })
    };
    let read_gloss = |sutta_id: i32| -> Option<BilaraRecord> {
        let r = appdata.do_read(|conn| {
            sutta_glosses::table
                .filter(sutta_glosses::sutta_id.eq(sutta_id))
                .select(SuttaGloss::as_select())
                .first(conn)
                .optional()
        });
        let g = r.map_err(|e| crate::logger::error(&format!("sutta_json: sutta_glosses: {}", e))).ok()??;
        Some(BilaraRecord {
            content: raw_json(g.content_json, "sutta_glosses", &g.sutta_uid)?,
            uid: g.sutta_uid,
            language: g.language,
            source_uid: g.source_uid,
        })
    };

    let variants = read_variant(sutta.id).or_else(|| {
        if sutta.language == "pli" {
            return None;
        }
        let reference = sutta.uid.split('/').next().unwrap_or_default();
        let root = appdata.get_sutta(&format!("{}/pli/ms", reference))?;
        read_variant(root.id)
    });

    let records = SuttaBilaraRecords {
        html_template: raw_json(sutta.content_json_tmpl.clone(), "content_json_tmpl", &sutta.uid),
        variants,
        comments: read_comment(sutta.id),
        glosses: read_gloss(sutta.id),
        content,
        uid: sutta.uid.clone(),
        language: sutta.language.clone(),
        source_uid: sutta.source_uid.clone(),
    };
    // Strings, options and already-validated `RawValue`s: serializing cannot fail.
    let json = serde_json::to_string(&records).expect("SuttaBilaraRecords serializes");
    SuttaBilaraJson::Found { uid: sutta.uid, json }
}

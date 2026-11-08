//! Stored DPD headword selections: the numeric `<id>/dpd` uid is the key, the
//! `lemma_1` travels beside it for people and models.
//!
//! A DPD headword keeps its id across DPD releases while its `lemma_1` label
//! can change (homonym numbers are renumbered, and some pairs swap). So a
//! lemma-form uid (`abhinīhāra-1/dpd`) can name a different sense after a DPD
//! upgrade, and a numeric one (`7945/dpd`) cannot. The entry points that take
//! uids from outside (Open JSON, `import-gloss-data`, the set-phrase seeder)
//! pass them through `normalize_dpd_selected_uid()`, so only numeric uids are
//! stored. See `docs/gloss-ai-word-selection.md`, "The uid two-lane gotcha".

use std::collections::HashMap;

use anyhow::{Context, Result};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

use crate::helpers::word_uid_sanitize;

/// The `lemma_1` ↔ id mapping of one `dpd.sqlite3`, loaded once for a batch of
/// normalisations (~90k headwords, two short strings each).
#[derive(Debug, Default)]
pub struct DpdLemmaIndex {
    /// `word_uid_sanitize(lemma_1)`, lowercased → ids. This is the lemma-form
    /// uid without its `/dpd` suffix, the same comparison
    /// `gloss_option_uid_matches()` makes at runtime.
    by_sanitized: HashMap<String, Vec<i32>>,
    /// `lemma_1` verbatim → ids.
    by_lemma: HashMap<String, Vec<i32>>,
    lemma_by_id: HashMap<i32, String>,
}

impl DpdLemmaIndex {
    /// Read every headword's `(id, lemma_1)` from a `dpd.sqlite3` connection.
    pub fn load(conn: &mut SqliteConnection) -> Result<Self> {
        use crate::db::dpd_schema::dpd_headwords::dsl::*;
        let rows: Vec<(i32, String)> = dpd_headwords
            .select((id, lemma_1))
            .load(conn)
            .context("DpdLemmaIndex::load(): failed to read dpd_headwords")?;
        Ok(Self::from_headwords(rows))
    }

    pub fn from_headwords(rows: impl IntoIterator<Item = (i32, String)>) -> Self {
        let mut index = Self::default();
        for (hw_id, lemma) in rows {
            index
                .by_sanitized
                .entry(sanitized_key(&lemma))
                .or_default()
                .push(hw_id);
            index.by_lemma.entry(lemma.clone()).or_default().push(hw_id);
            index.lemma_by_id.insert(hw_id, lemma);
        }
        index
    }

    /// The `lemma_1` of a headword id, if the id exists in this release.
    pub fn lemma_of(&self, hw_id: i32) -> Option<&str> {
        self.lemma_by_id.get(&hw_id).map(|s| s.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.lemma_by_id.is_empty()
    }
}

fn sanitized_key(lemma: &str) -> String {
    word_uid_sanitize(lemma).to_lowercase()
}

/// The headword id of a numeric DPD uid (`7945/dpd` → `7945`), `None` for any
/// other form.
pub fn dpd_headword_id_of(uid: &str) -> Option<i32> {
    let base = uid.strip_suffix("/dpd")?;
    if base.is_empty() || !base.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    base.parse().ok()
}

/// A DPD uid that names a headword by its lemma (`ārāma-4/dpd`): ends in
/// `/dpd`, is not numeric and is not a root (`√kar/dpd`, keyed by the root,
/// which is stable).
pub fn is_lemma_form_dpd_uid(uid: &str) -> bool {
    match uid.strip_suffix("/dpd") {
        Some(base) => !base.is_empty() && dpd_headword_id_of(uid).is_none() && !base.starts_with('√'),
        None => false,
    }
}

/// The result of a normalisation: the uid to store, and a report line when the
/// file's `selected_lemma` disagrees with the installed DPD's `lemma_1` for
/// that id. The entry is still stored by its id: the id is the stable key, and
/// the report is how a file written against an older DPD release shows up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedUid {
    pub uid: String,
    pub report: Option<String>,
}

/// Normalise a stored-selection uid taken from outside the app.
///
/// - not a DPD uid, a root uid (`√kar/dpd`), or empty with no lemma: unchanged;
/// - numeric (`7945/dpd`): unchanged if the id exists, an error otherwise;
/// - lemma-form (`ārāma-4/dpd`): resolved to its numeric uid, an error when no
///   headword or more than one headword has that sanitized lemma;
/// - empty uid with a `selected_lemma`: resolved by the exact `lemma_1`, which
///   must name exactly one headword.
///
/// When both a uid and a `selected_lemma` are given and the installed
/// `lemma_1` of the resolved id differs, the id is kept and a report is
/// returned with both labels.
pub fn normalize_dpd_selected_uid(
    index: &DpdLemmaIndex,
    uid: &str,
    selected_lemma: Option<&str>,
) -> Result<NormalizedUid, String> {
    let uid = uid.trim();
    let selected_lemma = selected_lemma.map(str::trim).filter(|l| !l.is_empty());

    let unchanged = || NormalizedUid { uid: uid.to_string(), report: None };

    if uid.is_empty() {
        let Some(lemma) = selected_lemma else {
            return Ok(unchanged());
        };
        let hw_id = single_id(index.by_lemma.get(lemma), || {
            format!("selected_lemma '{}' names no DPD headword", lemma)
        }, |n| {
            format!("selected_lemma '{}' names {} DPD headwords", lemma, n)
        })?;
        return Ok(NormalizedUid { uid: format!("{}/dpd", hw_id), report: None });
    }

    let Some(base) = uid.strip_suffix("/dpd") else {
        return Ok(unchanged());
    };
    if base.starts_with('√') {
        return Ok(unchanged());
    }

    let hw_id = match dpd_headword_id_of(uid) {
        Some(hw_id) => {
            if index.lemma_of(hw_id).is_none() {
                return Err(format!("'{}': no DPD headword has id {}", uid, hw_id));
            }
            hw_id
        }
        None => single_id(index.by_sanitized.get(&base.to_lowercase()), || {
            format!("'{}' names no DPD headword", uid)
        }, |n| {
            format!("'{}' is ambiguous: {} DPD headwords have that lemma", uid, n)
        })?,
    };

    let installed = index.lemma_of(hw_id).unwrap_or_default();
    let report = selected_lemma.filter(|l| *l != installed).map(|l| {
        format!(
            "'{}': the file's selected_lemma is '{}', the installed DPD's lemma for id {} is '{}'; kept by id",
            uid, l, hw_id, installed
        )
    });

    Ok(NormalizedUid { uid: format!("{}/dpd", hw_id), report })
}

fn single_id(
    ids: Option<&Vec<i32>>,
    none: impl FnOnce() -> String,
    many: impl FnOnce(usize) -> String,
) -> Result<i32, String> {
    match ids.map(|v| v.as_slice()).unwrap_or_default() {
        [] => Err(none()),
        [one] => Ok(*one),
        more => Err(many(more.len())),
    }
}

/// The `lemma_1` of each numeric DPD uid in `uids` that exists in the given
/// `dpd.sqlite3`, keyed by the uid. Other uids are left out. Used to fill
/// `selected_lemma` in the files a person reads.
pub fn lemmas_for_uids<'a>(
    conn: &mut SqliteConnection,
    uids: impl IntoIterator<Item = &'a str>,
) -> Result<HashMap<String, String>> {
    use crate::db::dpd_schema::dpd_headwords::dsl::*;
    let mut ids: Vec<i32> = uids.into_iter().filter_map(dpd_headword_id_of).collect();
    ids.sort_unstable();
    ids.dedup();

    let mut out = HashMap::new();
    for chunk in ids.chunks(500) {
        let rows: Vec<(i32, String)> = dpd_headwords
            .filter(id.eq_any(chunk))
            .select((id, lemma_1))
            .load(conn)
            .context("lemmas_for_uids(): failed to read dpd_headwords")?;
        for (hw_id, lemma) in rows {
            out.insert(format!("{}/dpd", hw_id), lemma);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> DpdLemmaIndex {
        DpdLemmaIndex::from_headwords([
            (7944, "abhinīhāra 2".to_string()),
            (7945, "abhinīhāra 1".to_string()),
            (12463, "ārāma 4".to_string()),
            (50000, "bhikkhu".to_string()),
            (34626, "dhamma 1.01".to_string()),
            // Two headwords sharing one sanitized form (as in a release
            // without UNIQUE (lemma_1)).
            (60001, "twin".to_string()),
            (60002, "twin".to_string()),
        ])
    }

    fn ok(uid: &str, lemma: Option<&str>) -> NormalizedUid {
        normalize_dpd_selected_uid(&index(), uid, lemma).unwrap()
    }

    #[test]
    fn numeric_uid_is_unchanged() {
        assert_eq!(ok("7945/dpd", None), NormalizedUid { uid: "7945/dpd".into(), report: None });
        assert_eq!(ok("7945/dpd", Some("abhinīhāra 1")).report, None);
    }

    #[test]
    fn unknown_numeric_uid_is_an_error() {
        assert!(normalize_dpd_selected_uid(&index(), "1/dpd", None).is_err());
    }

    #[test]
    fn lemma_form_uid_is_resolved() {
        assert_eq!(ok("ārāma-4/dpd", None).uid, "12463/dpd");
        assert_eq!(ok("bhikkhu/dpd", None).uid, "50000/dpd");
        assert_eq!(ok("dhamma-1-01/dpd", None).uid, "34626/dpd");
        assert_eq!(ok("abhinīhāra-1/dpd", None).uid, "7945/dpd");
    }

    #[test]
    fn lemma_only_is_resolved() {
        assert_eq!(ok("", Some("ārāma 4")).uid, "12463/dpd");
    }

    #[test]
    fn unknown_or_ambiguous_lemma_is_an_error() {
        let ix = index();
        assert!(normalize_dpd_selected_uid(&ix, "nosuchword-3/dpd", None).is_err());
        assert!(normalize_dpd_selected_uid(&ix, "twin/dpd", None).is_err());
        assert!(normalize_dpd_selected_uid(&ix, "", Some("nosuchword")).is_err());
        assert!(normalize_dpd_selected_uid(&ix, "", Some("twin")).is_err());
        // The sanitized form is not a lemma_1.
        assert!(normalize_dpd_selected_uid(&ix, "", Some("ārāma-4")).is_err());
    }

    #[test]
    fn mismatching_lemma_keeps_the_id_and_reports() {
        // A file written when id 7945 was 'abhinīhāra 2'.
        let n = ok("7945/dpd", Some("abhinīhāra 2"));
        assert_eq!(n.uid, "7945/dpd");
        let report = n.report.expect("a report");
        assert!(report.contains("abhinīhāra 2") && report.contains("abhinīhāra 1"), "{}", report);
    }

    #[test]
    fn root_and_other_dictionaries_are_unchanged() {
        assert_eq!(ok("√kar/dpd", None).uid, "√kar/dpd");
        assert_eq!(ok("dhamma/ncped", Some("x")).uid, "dhamma/ncped");
        assert_eq!(ok("", None).uid, "");
    }

    #[test]
    fn uid_forms() {
        assert_eq!(dpd_headword_id_of("7945/dpd"), Some(7945));
        assert_eq!(dpd_headword_id_of("ārāma-4/dpd"), None);
        assert_eq!(dpd_headword_id_of("7945/ncped"), None);
        assert!(is_lemma_form_dpd_uid("ārāma-4/dpd"));
        assert!(!is_lemma_form_dpd_uid("7945/dpd"));
        assert!(!is_lemma_form_dpd_uid("√kar/dpd"));
        assert!(!is_lemma_form_dpd_uid("dhamma/ncped"));
    }

    #[test]
    fn load_and_lemmas_for_uids_read_a_database() {
        use diesel::connection::SimpleConnection;
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        conn.batch_execute(
            "CREATE TABLE dpd_headwords (id INTEGER PRIMARY KEY, lemma_1 TEXT NOT NULL);
             INSERT INTO dpd_headwords VALUES (12463, 'ārāma 4'), (50000, 'bhikkhu');",
        )
        .unwrap();
        let ix = DpdLemmaIndex::load(&mut conn).unwrap();
        assert_eq!(ix.lemma_of(12463), Some("ārāma 4"));

        let lemmas = lemmas_for_uids(&mut conn, ["12463/dpd", "ārāma-4/dpd", "1/dpd"]).unwrap();
        assert_eq!(lemmas.len(), 1);
        assert_eq!(lemmas.get("12463/dpd").map(|s| s.as_str()), Some("ārāma 4"));
    }
}

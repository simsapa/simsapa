use std::collections::HashSet;

use diesel::prelude::*;
use chrono::NaiveDateTime;
use anyhow::{Context, Result};

use crate::db::dictionaries_models::*;
use crate::db::DatabaseHandle;
use crate::logger::error;

pub type DictionariesDbHandle = DatabaseHandle;

impl DictionariesDbHandle {
    pub fn get_word(&self, word_uid: &str) -> Option<DictWord> {
        use crate::db::dictionaries_schema::dict_words::dsl::*;

        let dict_word = self.do_read(|db_conn| {
            dict_words
                .filter(uid.eq(word_uid))
                .select(DictWord::as_select())
                .first(db_conn)
                .optional()
        });

        match dict_word {
            Ok(x) => x,
            Err(e) => {
                error(&format!("{}", e));
                None
            }
        }
    }

    /// Get distinct language values from dict_words table
    /// Returns a sorted Vec<String> with NULL values filtered out
    pub fn get_distinct_languages(&self) -> Vec<String> {
        use crate::db::dictionaries_schema::dict_words::dsl::*;

        let result = self.do_read(|db_conn| {
            dict_words
                .select(language)
                .filter(language.is_not_null())
                .distinct()
                .load::<Option<String>>(db_conn)
        });

        match result {
            Ok(langs) => {
                let mut unique_langs: Vec<String> = langs
                    .into_iter()
                    .flatten() // Filter out None values
                    .filter(|lang| !lang.is_empty()) // Filter out empty strings
                    .collect();
                unique_langs.sort();
                unique_langs
            }
            Err(e) => {
                error(&format!("get_distinct_languages(): {}", e));
                Vec::new()
            }
        }
    }

    /// Get distinct dict_label (dictionary source) values from dict_words table
    /// Returns a sorted Vec<String>
    pub fn get_distinct_sources(&self) -> Vec<String> {
        use crate::db::dictionaries_schema::dict_words::dsl::*;

        let result = self.do_read(|db_conn| {
            dict_words
                .select(dict_label)
                .distinct()
                .load::<String>(db_conn)
        });

        match result {
            Ok(mut sources) => {
                // Filter out empty strings and sort
                sources.retain(|s| !s.is_empty());
                sources.sort();
                sources
            }
            Err(e) => {
                error(&format!("get_distinct_sources(): {}", e));
                Vec::new()
            }
        }
    }

    pub fn create_dictionary(&self, new_dict: NewDictionary) -> Result<Dictionary> {
        use crate::db::dictionaries_schema::dictionaries;

        self.do_write(|db_conn| {
            diesel::insert_into(dictionaries::table)
                .values(&new_dict)
                .returning(Dictionary::as_returning())
                .get_result(db_conn)
        }).with_context(|| format!("Insert failed for dictionary: {}", new_dict.label))
    }

    // NOTE: the `dict_words.dictionary_id` FK is `ON DELETE CASCADE`
    // (see `backend/migrations/dictionaries/2026-07-23-000000_initial_schema/up.sql`),
    // so deleting the parent `dictionaries` row wipes all child `dict_words`
    // in a single statement. This is the path used by user-dictionary
    // delete — it is simpler than batched deletes and acceptable because the
    // operation runs on a worker thread with an indeterminate progress UI;
    // no per-row progress or mid-delete cancellation is supported.
    //
    // Background: the bundled libsqlite3-sys is built WITHOUT
    // SQLITE_ENABLE_UPDATE_DELETE_LIMIT, so `DELETE … LIMIT n` returns
    // `near "LIMIT": syntax error`; if batched deletes are ever needed they
    // must use the portable form
    // `DELETE FROM t WHERE id IN (SELECT id FROM t WHERE … LIMIT ?)`.
    pub fn delete_dictionary_by_label(&self, dict_label_val: &str) -> Result<usize> {
        use crate::db::dictionaries_schema::dictionaries::dsl::*;

        self.do_write(|db_conn| {
            diesel::delete(dictionaries.filter(label.eq(dict_label_val))).execute(db_conn)
        })
    }

    pub fn create_dict_resource(&self, new_resource: &NewDictResource) -> Result<usize> {
        use crate::db::dictionaries_schema::dict_resources;

        self.do_write(|db_conn| {
            diesel::insert_into(dict_resources::table)
                .values(new_resource)
                .execute(db_conn)
        }).with_context(|| format!(
            "Insert failed for dict_resource: dict {} path {}",
            new_resource.dictionary_id, new_resource.resource_path
        ))
    }

    /// Look up a single resource blob by dictionary id + relative path.
    pub fn get_dict_resource(&self, dictionary_id_param: i32, resource_path_param: &str) -> Result<Option<DictResource>> {
        use crate::db::dictionaries_schema::dict_resources::dsl::*;

        self.do_read(|db_conn| {
            dict_resources
                .filter(dictionary_id.eq(dictionary_id_param))
                .filter(resource_path.eq(resource_path_param))
                .select(DictResource::as_select())
                .first(db_conn)
                .optional()
        })
    }

    /// List all resources for a dictionary (used for CSS/JS injection at render time).
    pub fn list_dict_resources(&self, dictionary_id_param: i32) -> Result<Vec<DictResource>> {
        use crate::db::dictionaries_schema::dict_resources::dsl::*;

        self.do_read(|db_conn| {
            dict_resources
                .filter(dictionary_id.eq(dictionary_id_param))
                .select(DictResource::as_select())
                .load(db_conn)
        })
    }

    /// Delete all resources for a dictionary (called on dictionary delete; the
    /// FK is ON DELETE CASCADE so this is mainly explicit for clarity/safety).
    pub fn delete_dict_resources(&self, dictionary_id_param: i32) -> Result<usize> {
        use crate::db::dictionaries_schema::dict_resources::dsl::*;

        self.do_write(|db_conn| {
            diesel::delete(dict_resources.filter(dictionary_id.eq(dictionary_id_param))).execute(db_conn)
        })
    }

    pub fn create_dict_word(&self, new_dict_word: &NewDictWord) -> Result<DictWord> {
        use crate::db::dictionaries_schema::dict_words;

        self.do_write(|db_conn| {
            diesel::insert_into(dict_words::table)
                .values(new_dict_word)
                .returning(DictWord::as_returning())
                .get_result(db_conn)
        })
    }

    /// List dictionaries, ordered by label.
    pub fn list_dictionaries(&self, filter_is_user_imported: Option<bool>) -> Result<Vec<Dictionary>> {
        use crate::db::dictionaries_schema::dictionaries::dsl::*;

        if let Some(is_user_imported_value) = filter_is_user_imported {
            self.do_read(|db_conn| {
                dictionaries
                    .filter(is_user_imported.eq(is_user_imported_value))
                    .order(label.asc())
                    .select(Dictionary::as_select())
                    .load::<Dictionary>(db_conn)
            }).context("list_dictionaries failed")
        } else {
            self.do_read(|db_conn| {
                dictionaries
                    .order(label.asc())
                    .select(Dictionary::as_select())
                    .load::<Dictionary>(db_conn)
            }).context("list_dictionaries failed")
        }
    }

    /// Count all `dict_words` rows (for `/health`). A 0 here means the
    /// dictionaries DB is not loaded / not installed.
    pub fn count_dict_words(&self) -> Result<i64> {
        use crate::db::dictionaries_schema::dict_words::dsl::*;
        self.do_read(|db_conn| dict_words.count().get_result::<i64>(db_conn))
            .context("count_dict_words failed")
    }

    /// Count `dict_words` rows belonging to a given dictionary.
    pub fn count_words_for_dictionary(&self, dict_id: i32) -> Result<i64> {
        use crate::db::dictionaries_schema::dict_words::dsl::*;

        self.do_read(|db_conn| {
            dict_words
                .filter(dictionary_id.eq(dict_id))
                .count()
                .get_result::<i64>(db_conn)
        }).context("count_words_for_dictionary failed")
    }

    /// Return user-imported `dictionaries` rows whose `indexed_at IS NULL`.
    pub fn list_dictionaries_needing_index(&self) -> Result<Vec<Dictionary>> {
        use crate::db::dictionaries_schema::dictionaries::dsl::*;

        self.do_read(|db_conn| {
            dictionaries
                .filter(is_user_imported.eq(true))
                .filter(indexed_at.is_null())
                .order(label.asc())
                .select(Dictionary::as_select())
                .load::<Dictionary>(db_conn)
        }).context("list_dictionaries_needing_index failed")
    }

    /// Rename a user-imported dictionary's label. In a single transaction:
    ///   - update `dictionaries.label`
    ///   - update `dict_words.dict_label`
    ///   - rewrite `dict_words.uid` from `<word>/<old_label>` to `<word>/<new_label>`
    ///   - set `dictionaries.indexed_at = NULL`
    pub fn rename_dictionary_label(&self, old_label: &str, new_label: &str) -> Result<()> {
        self.do_write(|db_conn| {
            db_conn.transaction::<_, diesel::result::Error, _>(|tx| {
                rename_dictionary_label_in(tx, old_label, new_label)
            })
        }).with_context(|| format!("rename_dictionary_label({} -> {}) failed", old_label, new_label))
    }

    /// Set `dictionaries.indexed_at` for one row.
    pub fn set_indexed_at(&self, dict_id: i32, ts: NaiveDateTime) -> Result<()> {
        use crate::db::dictionaries_schema::dictionaries::dsl::*;

        self.do_write(|db_conn| {
            diesel::update(dictionaries.filter(id.eq(dict_id)))
                .set(indexed_at.eq(Some(ts)))
                .execute(db_conn)
                .map(|_| ())
        }).context("set_indexed_at failed")
    }

    /// Set `dictionaries.indexed_at` for the row matching `dict_label`.
    ///
    /// Targets a single dictionary by label so callers can mark only the
    /// dictionaries they have just indexed into Tantivy, leaving any other
    /// rows with `indexed_at IS NULL` untouched (so the startup reconcile
    /// pass still picks them up).
    pub fn set_indexed_at_by_label(&self, dict_label_value: &str, ts: NaiveDateTime) -> Result<()> {
        use crate::db::dictionaries_schema::dictionaries::dsl::*;

        self.do_write(|db_conn| {
            diesel::update(dictionaries.filter(label.eq(dict_label_value)))
                .set(indexed_at.eq(Some(ts)))
                .execute(db_conn)
                .map(|_| ())
        }).with_context(|| format!("set_indexed_at_by_label({}) failed", dict_label_value))
    }

    /// Set of distinct `dict_words.source_uid` values (column `dict_label`)
    /// belonging to non-user-imported dictionaries.
    /// This is the canonical "shipped/built-in label set" for label-collision
    /// validation (PRD §8a). Some shipped sources (e.g. bold-definitions) use a
    /// per-row `ref_code` as `dict_label`, so we MUST compute this from
    /// `dict_words` and not from `dictionaries.label`.
    pub fn list_shipped_source_uids(&self) -> Result<HashSet<String>> {
        use crate::db::dictionaries_schema::dict_words;
        use crate::db::dictionaries_schema::dictionaries;

        let rows: Vec<String> = self.do_read(|db_conn| {
            dict_words::table
                .inner_join(dictionaries::table.on(dict_words::dictionary_id.eq(dictionaries::id)))
                .filter(dictionaries::is_user_imported.eq(false))
                .select(dict_words::dict_label)
                .distinct()
                .load::<String>(db_conn)
        }).context("list_shipped_source_uids failed")?;

        Ok(rows.into_iter().collect())
    }

    /// Returns true if `label` collides with any shipped/built-in `source_uid`.
    pub fn is_label_taken_by_shipped(&self, label: &str) -> Result<bool> {
        Ok(self.list_shipped_source_uids()?.contains(label))
    }

    /// Ensure a single shipped parent `dictionaries` row exists for all
    /// bold-definition entries.
    ///
    /// Bold-definition entries live in `dpd.sqlite3::bold_definitions`
    /// rather than in `dict_words`, and they are indexed into the dict
    /// Tantivy index with `source_uid = ref_code` (a per-row Nikāya-
    /// dependent value, e.g. `vina`, `mna`, `vvt`). Creating one
    /// `dictionaries` row per ref_code would litter the registry with
    /// dozens of fake "dictionaries"; instead, a single umbrella row with
    /// `label = "bold_definitions"` represents the category. The reconcile
    /// pass identifies the legitimate `source_uid` set for this category
    /// by querying `DISTINCT bold_definitions.ref_code` directly from the
    /// DPD database.
    ///
    /// Idempotent: returns true if the row was just created, false if it
    /// already existed.
    pub const BOLD_DEFINITIONS_LABEL: &'static str = "bold_definitions";

    pub fn ensure_bold_definitions_parent_dictionary(&self) -> Result<bool> {
        use crate::db::dictionaries_schema::dictionaries::dsl::*;

        let existing: Option<i32> = self.do_read(|db_conn| {
            dictionaries
                .select(id)
                .filter(label.eq(Self::BOLD_DEFINITIONS_LABEL))
                .first::<i32>(db_conn)
                .optional()
        }).context("ensure_bold_definitions_parent_dictionary: lookup")?;

        if existing.is_some() {
            return Ok(false);
        }

        let new_dict = NewDictionary {
            label: Self::BOLD_DEFINITIONS_LABEL,
            title: "Bold Definitions",
            dict_type: "sql",
            language: Some("pli"),
            is_user_imported: false,
            indexed_at: Some(chrono::Utc::now().naive_utc()),
            ..Default::default()
        };
        self.create_dictionary(new_dict)
            .context("ensure_bold_definitions_parent_dictionary: insert")?;
        Ok(true)
    }

    /// Find or create DPD Dictionary record with label 'dpd'
    pub fn find_or_create_dpd_dictionary(&self) -> Result<Dictionary> {
        use crate::db::dictionaries_schema::dictionaries::dsl::*;

        let db_conn = &mut self.get_conn()?;

        if let Ok(x) = dictionaries
            .select(Dictionary::as_select())
            .filter(label.eq("dpd"))
            .first(db_conn) { return Ok(x) }

        // If not returned yet, create a new record
        let new_dict = NewDictionary {
            label: "dpd",
            title: "Digital Pāḷi Dictionary",
            dict_type: "sql", // FIXME dict_type = DictTypeName.Sql.value,
            language: Some("pli"),
            ..Default::default()
        };

        self.create_dictionary(new_dict)
    }
}

pub fn create_dict_words_batch(
    db_conn: &mut SqliteConnection,
    new_words: &[NewDictWord],
) -> Result<usize, diesel::result::Error> {
    use crate::db::dictionaries_schema::dict_words;
    diesel::insert_into(dict_words::table)
        .values(new_words)
        .execute(db_conn)
}

/// Rename a dictionary label on any connection holding the dictionaries schema:
/// the live `dictionaries.sqlite3`, or a pending `user_dictionaries.sqlite3`
/// upgrade snapshot. Run it inside a transaction.
///
/// Clears `indexed_at`, and rewrites `dict_words.dict_label` and the
/// `<word>/<old_label>` uid suffix of that dictionary's words.
pub fn rename_dictionary_label_in(
    conn: &mut SqliteConnection,
    old_label: &str,
    new_label: &str,
) -> diesel::QueryResult<()> {
    use crate::db::dictionaries_schema::dictionaries;
    use crate::db::dictionaries_schema::dict_words;

    diesel::update(dictionaries::table.filter(dictionaries::label.eq(old_label)))
        .set((
            dictionaries::label.eq(new_label),
            dictionaries::indexed_at.eq::<Option<NaiveDateTime>>(None),
        ))
        .execute(conn)?;

    diesel::update(dict_words::table.filter(dict_words::dict_label.eq(old_label)))
        .set(dict_words::dict_label.eq(new_label))
        .execute(conn)?;

    // Rewrite the uid suffix of the renamed dictionary's words only. The suffix
    // is compared with substr, not LIKE: `_` is a valid label character and a
    // LIKE wildcard, so `%/a_b` would also match another dictionary's `x/aXb`.
    let suffix_old = format!("/{}", old_label);
    let suffix_new = format!("/{}", new_label);
    diesel::sql_query(
        "UPDATE dict_words \
         SET uid = substr(uid, 1, length(uid) - length(?1)) || ?2 \
         WHERE dict_label = ?3 AND substr(uid, -length(?1)) = ?1",
    )
        .bind::<diesel::sql_types::Text, _>(&suffix_old)
        .bind::<diesel::sql_types::Text, _>(&suffix_new)
        .bind::<diesel::sql_types::Text, _>(new_label)
        .execute(conn)?;

    Ok(())
}

use diesel::prelude::*;
use regex::Regex;
use anyhow::{Context, Result};
use serde::Serialize;

use crate::get_app_data;
use crate::db::appdata_models::*;
use crate::db::DatabaseHandle;
use crate::app_settings::AppSettings;
use crate::logger::{info, error};

static COMMON_WORDS_JSON: &str = include_str!("../../../assets/common-words.json");

/// Curated set-phrase selections for the Gloss tab, seeded at bootstrap into
/// `gloss_phrase_selections` (see `seed_gloss_phrase_selections`).
static GLOSS_PHRASE_SELECTIONS_JSON: &str = include_str!("../../../assets/gloss-phrase-selections.json");

/// Precedence rank of a `gloss_word_context_cache.origin` value:
/// user > built-in > ai. Unknown origins rank lowest.
pub fn gloss_cache_origin_rank(origin: &str) -> u8 {
    match origin {
        "user" => 3,
        "built-in" => 2,
        "ai" => 1,
        _ => 0,
    }
}

pub type AppdataDbHandle = DatabaseHandle;

impl AppdataDbHandle {
    /// Count `suttas` rows (for `/health`). A 0 here means the DB is not
    /// loaded / not installed.
    pub fn count_suttas(&self) -> Result<i64> {
        use crate::db::appdata_schema::suttas::dsl::*;
        self.do_read(|db_conn| suttas.count().get_result::<i64>(db_conn))
            .context("count_suttas failed")
    }

    /// Get distinct sutta languages from the database
    pub fn get_sutta_languages(&self) -> Vec<String> {
        use crate::db::appdata_schema::suttas::dsl::*;

        let result = self.do_read(|db_conn| {
            suttas
                .select(language)
                .distinct()
                .load::<String>(db_conn)
        });

        match result {
            Ok(mut langs) => {
                // Filter out empty strings, convert to lowercase, and deduplicate
                langs.sort();
                let mut seen = std::collections::HashSet::new();
                let mut unique_langs: Vec<String> = Vec::new();

                for lang in langs {
                    if !lang.is_empty() {
                        let lowercase_lang = lang.to_lowercase();
                        if seen.insert(lowercase_lang.clone()) {
                            unique_langs.push(lowercase_lang);
                        }
                    }
                }

                // Sort again to ensure consistent alphabetical order
                unique_langs.sort();
                unique_langs
            },
            Err(e) => {
                error(&format!("get_sutta_languages(): {}", e));
                Vec::new()
            }
        }
    }

    pub fn get_sutta(&self, sutta_uid: &str) -> Option<Sutta> {
        use crate::db::appdata_schema::suttas::dsl::*;

        let sutta = self.do_read(|db_conn| {
            suttas
                .filter(uid.eq(sutta_uid))
                .select(Sutta::as_select())
                .first(db_conn)
                .optional()
        });

        match sutta {
            Ok(x) => x,
            Err(e) => {
                error(&format!("{}", e));
                None
            },
        }
    }

    /// Look up a sutta whose stored range includes the number in `sutta_uid`.
    ///
    /// E.g. a clicked link `sn45.92/pli/ms` has no exact uid, but the sutta
    /// `sn45.92-95/pli/ms` covers that number. This mirrors the range lookup
    /// performed for the search input box (see `QueryTask::uid_sutta_range_all`).
    ///
    /// When the queried uid carries a `/lang/author` suffix, only suttas with
    /// the same suffix are considered, so the link resolves to the matching
    /// translation rather than an arbitrary one.
    pub fn get_sutta_by_range(&self, sutta_uid: &str) -> Option<Sutta> {
        use crate::db::appdata_schema::suttas::dsl::*;
        use crate::helpers::sutta_range_from_ref;

        let range = sutta_range_from_ref(sutta_uid)?;

        // Only proceed when we have a concrete numeric reference.
        let (range_start, _range_end) = match (range.start, range.end) {
            (Some(s), Some(e)) => (s as i32, e as i32),
            _ => return None,
        };

        // Extract a "/lang/author" suffix from the queried uid, if present
        // (e.g. "sn45.92/pli/ms" -> "/pli/ms").
        let suffix = sutta_uid.find('/').map(|i| sutta_uid[i..].to_string());

        let results = self.do_read(|db_conn| {
            suttas
                .filter(sutta_range_group.eq(&range.group))
                .filter(sutta_range_start.is_not_null())
                .filter(sutta_range_end.is_not_null())
                .filter(sutta_range_start.le(range_start))
                .filter(sutta_range_end.ge(range_start))
                .order(uid.asc())
                .select(Sutta::as_select())
                .load::<Sutta>(db_conn)
        });

        let results = match results {
            Ok(rows) => rows,
            Err(e) => {
                error(&format!("get_sutta_by_range(): {}", e));
                return None;
            }
        };

        match &suffix {
            // Prefer the sutta matching the requested language/author suffix.
            Some(s) => results
                .iter()
                .find(|x| x.uid.ends_with(s.as_str()))
                .or_else(|| results.first())
                .cloned(),
            None => results.into_iter().next(),
        }
    }

    /// Find a related sutta (commentary, sub-commentary, or root text) for the given sutta UID.
    ///
    /// `relation` is one of: "att" (commentary), "tik" (sub-commentary), "mula" (root text).
    ///
    /// Returns JSON: `{"found": true, "item_uid": "...", "table_name": "suttas", "sutta_title": "...", "sutta_ref": "..."}`
    /// or `{"found": false, "sutta_title": "..."}` with the current sutta's title for fallback search.
    pub fn find_related_sutta_json(&self, sutta_uid: &str, relation: &str) -> String {
        use crate::db::appdata_schema::suttas::dsl::*;

        // Parse the UID: "mn1/pli/ms" -> ref_part="mn1", lang_source="pli/ms"
        // or "mn1.att/pli/cst" -> ref_part="mn1.att", lang_source="pli/cst"
        let parts: Vec<&str> = sutta_uid.splitn(2, '/').collect();
        if parts.len() < 2 {
            return serde_json::json!({"found": false, "sutta_title": ""}).to_string();
        }

        let ref_part = parts[0]; // e.g. "mn1", "mn1.att", "mn1.tik"
        let lang_source = parts[1]; // e.g. "pli/ms", "pli/cst"

        // Get the current sutta's title for fallback search
        let current_title = self.get_sutta(sutta_uid)
            .and_then(|s| s.title)
            .unwrap_or_default();

        // Derive the base ref (strip .att, .tik suffixes to get the mūla ref)
        let base_ref = ref_part
            .split(".att").next().unwrap_or(ref_part)
            .split(".tik").next().unwrap_or(ref_part);

        // Build the target ref based on relation
        let target_ref = match relation {
            "att" => format!("{}.att", base_ref),
            "tik" => format!("{}.tik", base_ref),
            "mula" => base_ref.to_string(),
            _ => {
                return serde_json::json!({"found": false, "sutta_title": current_title}).to_string();
            }
        };

        // Try to find the target sutta with the same lang/source first
        let target_uid = format!("{}/{}", target_ref, lang_source);
        if let Some(sutta) = self.get_sutta(&target_uid) {
            return serde_json::json!({
                "found": true,
                "item_uid": sutta.uid,
                "table_name": "suttas",
                "sutta_title": sutta.title.unwrap_or_default(),
                "sutta_ref": sutta.sutta_ref,
            }).to_string();
        }

        // Try with pli/cst source (commentary is typically CST)
        let cst_uid = format!("{}/pli/cst", target_ref);
        #[allow(clippy::collapsible_if)]
        if cst_uid != target_uid {
            if let Some(sutta) = self.get_sutta(&cst_uid) {
                return serde_json::json!({
                    "found": true,
                    "item_uid": sutta.uid,
                    "table_name": "suttas",
                    "sutta_title": sutta.title.unwrap_or_default(),
                    "sutta_ref": sutta.sutta_ref,
                }).to_string();
            }
        }

        // Try LIKE search for any matching uid with the target ref
        let like_pattern = format!("{}/%", target_ref);
        let result = self.do_read(|db_conn| {
            suttas
                .filter(uid.like(&like_pattern))
                .select(Sutta::as_select())
                .first(db_conn)
                .optional()
        });

        if let Ok(Some(sutta)) = result {
            return serde_json::json!({
                "found": true,
                "item_uid": sutta.uid,
                "table_name": "suttas",
                "sutta_title": sutta.title.unwrap_or_default(),
                "sutta_ref": sutta.sutta_ref,
            }).to_string();
        }

        // Not found
        serde_json::json!({
            "found": false,
            "sutta_title": current_title,
        }).to_string()
    }

    pub fn get_full_sutta_uid(&self, partial_uid: &str) -> Option<String> {
        use crate::db::appdata_schema::suttas::dsl::*;

        // If UID already contains '/', check if it exists and return it
        if partial_uid.contains('/') {
            let result = self.do_read(|db_conn| {
                suttas
                    .filter(uid.eq(partial_uid))
                    .select(uid)
                    .first::<String>(db_conn)
                    .optional()
            });

            return match result {
                Ok(found_uid) => found_uid,
                Err(e) => {
                    error(&format!("Error checking sutta UID '{}': {}", partial_uid, e));
                    None
                }
            };
        }

        // First, try to find the Pali Mahasangiti version "{partial_uid}/pli/ms"
        let pli_ms_uid = format!("{}/pli/ms", partial_uid);
        let pli_result = self.do_read(|db_conn| {
            suttas
                .filter(uid.eq(&pli_ms_uid))
                .select(uid)
                .first::<String>(db_conn)
                .optional()
        });

        match pli_result {
            Ok(Some(found_uid)) => return Some(found_uid),
            Ok(None) => {
                // Pali MS not found, try LIKE query for any translation
            },
            Err(e) => {
                error(&format!("Error checking Pali MS UID '{}': {}", pli_ms_uid, e));
            }
        }

        // If Pali MS not found, find the first matching UID with LIKE
        let pattern = format!("{}/%", partial_uid);
        let result = self.do_read(|db_conn| {
            suttas
                .filter(uid.like(pattern))
                .select(uid)
                .first::<String>(db_conn)
                .optional()
        });

        match result {
            Ok(found_uid) => found_uid,
            Err(e) => {
                error(&format!("Error finding sutta UID for '{}': {}", partial_uid, e));
                None
            },
        }
    }

    pub fn get_translations_data_json_for_sutta_uid(
        &self,
        sutta_uid: &str,
        include_cst_commentary: bool,
        include_cst_mula: bool,
    ) -> String {
        // See sutta_search_window_state.py::_add_related_tabs()

        // Capture the reference before the first '/'
        let re = Regex::new(r"^([^/]+)/.*").expect("Invalid regex");
        let uid_ref = re.replace(sutta_uid, "$1").to_string();

        // Derive the base ref by stripping .att/.tik suffixes.
        // E.g. "mn1.att" -> "mn1", "mn1.tik" -> "mn1", "mn1" -> "mn1"
        let base_ref = uid_ref
            .split(".att").next().unwrap_or(&uid_ref)
            .split(".tik").next().unwrap_or(&uid_ref);

        let is_commentary = base_ref != uid_ref;

        use crate::db::appdata_schema::suttas::dsl::*;

        let app_data = get_app_data();
        let _lock = app_data.dbm.appdata.write_lock.lock();
        let mut db_conn = app_data.dbm.appdata.get_conn().expect("get_translations_data_json_for_sutta_uid(): No appdata conn");

        let mut res: Vec<Sutta> = Vec::new();

        // Build the uid filter: always include uid_ref/%, optionally include .att/% and .tik/%
        let mut query = suttas.into_boxed()
            .select(Sutta::as_select())
            .filter(uid.ne(sutta_uid));

        if is_commentary {
            // The opened sutta is a commentary (e.g. mn1.att/pli/cst).
            // Always include the mūla translations (base_ref/%).
            // Include other commentary types based on include_cst_commentary setting.
            if include_cst_commentary {
                // Include mūla, .att, and .tik variants
                query = query.filter(
                    uid.like(format!("{}/%", base_ref))
                       .or(uid.like(format!("{}.att%/%", base_ref)))
                       .or(uid.like(format!("{}.tik%/%", base_ref)))
                );
            } else {
                // Include mūla and the same commentary type as the opened sutta
                query = query.filter(
                    uid.like(format!("{}/%", base_ref))
                       .or(uid.like(format!("{}/%", uid_ref)))
                );
            }
        } else if include_cst_commentary {
            // Match mūla and commentary, including .xml variants (e.g. .att.xml/, .tik.xml/)
            query = query.filter(
                uid.like(format!("{}/%", uid_ref))
                   .or(uid.like(format!("{}.att%/%", uid_ref)))
                   .or(uid.like(format!("{}.tik%/%", uid_ref)))
            );
        } else {
            query = query.filter(uid.like(format!("{}/%", uid_ref)));
        }

        if let Ok(a) = query.load(&mut db_conn) {
            res.extend(a);
        }

        // Filter out CST mūla records if not included
        if !include_cst_mula {
            res.retain(|s| {
                // Keep the record unless it's a CST mūla (ends with /cst and is not commentary)
                // Note: .mul.xml/pli/cst records are mūla and should be excluded
                !(s.uid.ends_with("/cst")
                    && !s.uid.contains(".att")
                    && !s.uid.contains(".tik"))
            });
        }

        #[derive(Serialize)]
        struct TranslationData {
            item_uid: String,
            table_name: String,
            sutta_title: String,
            sutta_ref: String,
            language: String,
            author: String,
            /// Whether the text has segmented (Bilara) content — a
            /// non-segmented text cannot be interleaved in the Lines layout,
            /// so the column-bar dropdowns disable it there.
            has_content_json: bool,
        }

        let res_sorted_data: Vec<TranslationData> = sort_suttas(res)
            .into_iter().map(|s| {
                // uid format: "mn1/en/sujato" — the third part is the author.
                let author = s.uid.split('/').nth(2).unwrap_or("").to_string();
                TranslationData {
                    has_content_json: s.content_json.as_deref().map(|c| !c.is_empty()).unwrap_or(false),
                    language: s.language.clone(),
                    author,
                    item_uid: s.uid,
                    table_name: "suttas".to_string(),
                    sutta_title: s.title.unwrap_or("".to_string()),
                    sutta_ref: s.sutta_ref,
                }
            }).collect();

        serde_json::to_string(&res_sorted_data).expect("Can't encode JSON")
    }

    pub fn get_app_settings(&self) -> AppSettings {
        use crate::db::appdata_schema::app_settings::dsl::*;

        let json = self.do_read(|db_conn| {
            app_settings
                .filter(key.eq("app_settings"))
                .select(AppSetting::as_select())
                .first(db_conn)
                .optional()
        });

        match json {
            Ok(None) => AppSettings::default(),
            Ok(Some(setting)) => {
                let mut settings: AppSettings = setting.value
                       .map(|val| serde_json::from_str(&val).expect("Can't decode JSON"))
                       .unwrap_or_default();
                // Existing user settings gain newly added default prompt keys
                // without overwriting edits to existing keys.
                settings.merge_default_system_prompts();
                settings
            },
            Err(e) => {
                error(&format!("{}", e));
                AppSettings::default()
            }
        }
    }

    /// Upsert the `app_settings` row with a freshly serialized `AppSettings::default()`.
    pub fn reset_app_settings_to_defaults(&self) -> Result<usize> {
        use crate::db::appdata_schema::app_settings::dsl::*;

        let settings_json = serde_json::to_string(&AppSettings::default())
            .context("Failed to serialize default AppSettings")?;

        self.do_write(|db_conn| {
            let existing = app_settings
                .filter(key.eq("app_settings"))
                .first::<AppSetting>(db_conn)
                .optional()?;

            match existing {
                Some(setting) => diesel::update(app_settings.find(setting.id))
                    .set(value.eq(Some(settings_json.as_str())))
                    .execute(db_conn),
                None => diesel::insert_into(app_settings)
                    .values(NewAppSetting {
                        key: "app_settings",
                        value: Some(settings_json.as_str()),
                    })
                    .execute(db_conn),
            }
        })
    }

    pub fn get_common_words_json(&self) -> String {
        use crate::db::appdata_schema::app_settings::dsl::*;

        let json = self.do_read(|db_conn| {
            app_settings
                .filter(key.eq("common_words_json"))
                .select(AppSetting::as_select())
                .first(db_conn)
                .optional()
        });

        match json {
            Ok(None) => String::from(COMMON_WORDS_JSON),
            Ok(Some(setting)) => {
                setting.value.unwrap_or(String::from(COMMON_WORDS_JSON))
            }
            Err(e) => {
                error(&format!("{}", e));
                String::from(COMMON_WORDS_JSON)
            }
        }
    }

    pub fn save_common_words_json(&self, words_json: &str) -> Result<usize> {
        use crate::db::appdata_schema::app_settings::dsl::*;

        self.do_write(|db_conn| {
            let existing_setting = app_settings
                .filter(key.eq("common_words_json"))
                .first::<AppSetting>(db_conn)
                .optional()?;

            match existing_setting {
                Some(setting) => {
                    diesel::update(app_settings.find(setting.id))
                        .set(value.eq(Some(words_json)))
                        .execute(db_conn)
                }
                None => {
                    let new_setting = NewAppSetting {
                        key: "common_words_json",
                        value: Some(words_json),
                    };

                    diesel::insert_into(app_settings)
                        .values(&new_setting)
                        .execute(db_conn)
                }
            }
        })
    }

    // === Book-related queries ===

    pub fn get_book_by_uid(&self, book_uid: &str) -> Result<Option<Book>> {
        use crate::db::appdata_schema::books::dsl::*;

        self.do_read(|db_conn| {
            books
                .filter(uid.eq(book_uid))
                .select(Book::as_select())
                .first(db_conn)
                .optional()
        })
    }

    pub fn get_book_spine_item(&self, spine_item_uid_param: &str) -> Result<Option<BookSpineItem>> {
        use crate::db::appdata_schema::book_spine_items::dsl::*;

        self.do_read(|db_conn| {
            book_spine_items
                .filter(spine_item_uid.eq(spine_item_uid_param))
                .select(BookSpineItem::as_select())
                .first(db_conn)
                .optional()
        })
    }

    pub fn get_book_resource(&self, book_uid_param: &str, resource_path_param: &str) -> Result<Option<BookResource>> {
        use crate::db::appdata_schema::book_resources::dsl::*;

        self.do_read(|db_conn| {
            book_resources
                .filter(book_uid.eq(book_uid_param))
                .filter(resource_path.eq(resource_path_param))
                .select(BookResource::as_select())
                .first(db_conn)
                .optional()
        })
    }

    pub fn get_all_books(&self) -> Result<Vec<Book>> {
        use crate::db::appdata_schema::books::dsl::*;

        self.do_read(|db_conn| {
            books
                .select(Book::as_select())
                .order(title.asc())
                .load(db_conn)
        })
    }

    pub fn get_spine_items_for_book(&self, book_uid_param: &str) -> Result<Vec<BookSpineItem>> {
        use crate::db::appdata_schema::book_spine_items::dsl::*;

        self.do_read(|db_conn| {
            book_spine_items
                .filter(book_uid.eq(book_uid_param))
                .order(spine_index.asc())
                .select(BookSpineItem::as_select())
                .load(db_conn)
        })
    }

    pub fn get_book_spine_item_by_path(&self, book_uid_param: &str, resource_path_param: &str) -> Result<Option<BookSpineItem>> {
        use crate::db::appdata_schema::book_spine_items::dsl::*;

        self.do_read(|db_conn| {
            book_spine_items
                .filter(book_uid.eq(book_uid_param))
                .filter(resource_path.eq(resource_path_param))
                .select(BookSpineItem::as_select())
                .first(db_conn)
                .optional()
        })
    }

    pub fn get_prev_book_spine_item(&self, spine_item_uid_param: &str) -> Result<Option<BookSpineItem>> {
        use crate::db::appdata_schema::book_spine_items::dsl::*;

        // First get the current spine item to obtain book_uid and spine_index
        let current_item = self.get_book_spine_item(spine_item_uid_param)?;

        match current_item {
            Some(item) => {
                // Query for spine item with same book_uid and spine_index - 1
                self.do_read(|db_conn| {
                    book_spine_items
                        .filter(book_uid.eq(&item.book_uid))
                        .filter(spine_index.eq(item.spine_index - 1))
                        .select(BookSpineItem::as_select())
                        .first(db_conn)
                        .optional()
                })
            }
            None => Ok(None), // Current item not found, return None
        }
    }

    pub fn get_next_book_spine_item(&self, spine_item_uid_param: &str) -> Result<Option<BookSpineItem>> {
        use crate::db::appdata_schema::book_spine_items::dsl::*;

        // First get the current spine item to obtain book_uid and spine_index
        let current_item = self.get_book_spine_item(spine_item_uid_param)?;

        match current_item {
            Some(item) => {
                // Query for spine item with same book_uid and spine_index + 1
                self.do_read(|db_conn| {
                    book_spine_items
                        .filter(book_uid.eq(&item.book_uid))
                        .filter(spine_index.eq(item.spine_index + 1))
                        .select(BookSpineItem::as_select())
                        .first(db_conn)
                        .optional()
                })
            }
            None => Ok(None), // Current item not found, return None
        }
    }

    pub fn get_prev_sutta(&self, sutta_uid_param: &str) -> Result<Option<Sutta>> {
        use crate::db::appdata_schema::suttas::dsl::*;

        // Get the current sutta
        let current_sutta = match self.get_sutta(sutta_uid_param) {
            Some(s) => s,
            None => return Ok(None),
        };

        // Check if sutta has range information
        let (range_group, range_start) = match (&current_sutta.sutta_range_group, current_sutta.sutta_range_start) {
            (Some(g), Some(s)) => (g.clone(), s),
            _ => return Ok(None), // No range info, can't navigate
        };

        // Calculate the previous range end: current start - 1
        let prev_end = range_start - 1;

        if prev_end < 1 {
            // We're at the first sutta in this group, check for previous numbered group
            return self.get_last_sutta_in_prev_group(&range_group, &current_sutta.language, &current_sutta.source_uid);
        }

        // Query for all suttas in same group with range_end <= prev_end
        // Order by range_end DESC to get the closest previous sutta
        let candidates: Vec<Sutta> = self.do_read(|db_conn| {
            suttas
                .filter(sutta_range_group.eq(&range_group))
                .filter(sutta_range_end.is_not_null())
                .filter(sutta_range_end.le(prev_end))
                .order(sutta_range_end.desc())
                .limit(100)  // Get multiple candidates to allow filtering by language/source
                .select(Sutta::as_select())
                .load(db_conn)
        })?;

        // Prioritize: same source_uid > same language > "en" > "pli"
        Ok(self.prioritize_sutta_by_language_and_source(
            candidates,
            &current_sutta.language,
            &current_sutta.source_uid,
        ))
    }

    pub fn get_next_sutta(&self, sutta_uid_param: &str) -> Result<Option<Sutta>> {
        use crate::db::appdata_schema::suttas::dsl::*;

        // Get the current sutta
        let current_sutta = match self.get_sutta(sutta_uid_param) {
            Some(s) => s,
            None => return Ok(None),
        };

        // Check if sutta has range information
        let (range_group, range_end) = match (&current_sutta.sutta_range_group, current_sutta.sutta_range_end) {
            (Some(g), Some(e)) => (g.clone(), e),
            _ => return Ok(None), // No range info, can't navigate
        };

        // Calculate the next range start: current end + 1
        let next_start = range_end + 1;

        // Query for all suttas in same group with range_start >= next_start
        // Order by range_start ASC to get the closest next sutta
        let candidates: Vec<Sutta> = self.do_read(|db_conn| {
            suttas
                .filter(sutta_range_group.eq(&range_group))
                .filter(sutta_range_start.is_not_null())
                .filter(sutta_range_start.ge(next_start))
                .order(sutta_range_start.asc())
                .limit(100)  // Get multiple candidates to allow filtering by language/source
                .select(Sutta::as_select())
                .load(db_conn)
        })?;

        // Prioritize: same source_uid > same language > "en" > "pli"
        let next_in_group = self.prioritize_sutta_by_language_and_source(
            candidates,
            &current_sutta.language,
            &current_sutta.source_uid,
        );

        if next_in_group.is_some() {
            return Ok(next_in_group);
        }

        // No next sutta in current group, check for next numbered group
        self.get_first_sutta_in_next_group(&range_group, &current_sutta.language, &current_sutta.source_uid)
    }

    fn prioritize_sutta_by_language_and_source(
        &self,
        candidates: Vec<Sutta>,
        current_language: &str,
        current_source: &Option<String>,
    ) -> Option<Sutta> {
        if candidates.is_empty() {
            return None;
        }

        // Group candidates by their range (to handle multiple translations of same sutta)
        // We want to find the first available sutta number, then choose best translation
        let first_range = candidates[0].sutta_range_start;
        let same_range: Vec<Sutta> = candidates
            .into_iter()
            .filter(|s| s.sutta_range_start == first_range)
            .collect();

        // Priority 1: Same source_uid and same language
        if let Some(source) = current_source
            && let Some(sutta) = same_range.iter().find(|s| {
                s.language == current_language && s.source_uid.as_ref() == Some(source)
            }) {
                return Some(sutta.clone());
            }

        // Priority 2: Same language, any source
        if let Some(sutta) = same_range.iter().find(|s| s.language == current_language) {
            return Some(sutta.clone());
        }

        // Priority 3: English, any source
        if let Some(sutta) = same_range.iter().find(|s| s.language == "en") {
            return Some(sutta.clone());
        }

        // Priority 4: Pali, any source
        if let Some(sutta) = same_range.iter().find(|s| s.language == "pli") {
            return Some(sutta.clone());
        }

        // Fallback: return first candidate
        same_range.into_iter().next()
    }

    fn get_last_sutta_in_prev_group(
        &self,
        current_group: &str,
        current_language: &str,
        current_source: &Option<String>,
    ) -> Result<Option<Sutta>> {
        use crate::db::appdata_schema::suttas::dsl::*;

        // Extract base collection and number from group (e.g., "an3" -> "an", 3)
        let (base, num) = match self.extract_group_number(current_group) {
            Some((b, n)) if n > 1 => (b, n),
            _ => return Ok(None), // Not a numbered group or already at 1
        };

        // Try previous numbered group (e.g., "an3" -> "an2")
        let prev_group = format!("{}{}", base, num - 1);

        // Get the last sutta in the previous group (highest range_end)
        let candidates: Vec<Sutta> = self.do_read(|db_conn| {
            suttas
                .filter(sutta_range_group.eq(&prev_group))
                .filter(sutta_range_end.is_not_null())
                .order(sutta_range_end.desc())
                .limit(100)
                .select(Sutta::as_select())
                .load(db_conn)
        })?;

        Ok(self.prioritize_sutta_by_language_and_source(
            candidates,
            current_language,
            current_source,
        ))
    }

    fn get_first_sutta_in_next_group(
        &self,
        current_group: &str,
        current_language: &str,
        current_source: &Option<String>,
    ) -> Result<Option<Sutta>> {
        use crate::db::appdata_schema::suttas::dsl::*;

        // Extract base collection and number from group (e.g., "an3" -> "an", 3)
        let (base, num) = match self.extract_group_number(current_group) {
            Some((b, n)) => (b, n),
            None => return Ok(None), // Not a numbered group
        };

        // Try next numbered group (e.g., "an3" -> "an4")
        let next_group = format!("{}{}", base, num + 1);

        // Get the first sutta in the next group (lowest range_start)
        let candidates: Vec<Sutta> = self.do_read(|db_conn| {
            suttas
                .filter(sutta_range_group.eq(&next_group))
                .filter(sutta_range_start.is_not_null())
                .order(sutta_range_start.asc())
                .limit(100)
                .select(Sutta::as_select())
                .load(db_conn)
        })?;

        Ok(self.prioritize_sutta_by_language_and_source(
            candidates,
            current_language,
            current_source,
        ))
    }

    fn extract_group_number(&self, group: &str) -> Option<(String, i32)> {
        // Extract base collection and number from group
        // Examples: "an3" -> ("an", 3), "sn30" -> ("sn", 30), "mn" -> None
        let re = regex::Regex::new(r"^([a-z-]+)(\d+)$").ok()?;
        let caps = re.captures(group)?;

        let base = caps.get(1)?.as_str().to_string();
        let num = caps.get(2)?.as_str().parse::<i32>().ok()?;

        Some((base, num))
    }

    pub fn delete_book_by_uid(&self, book_uid_param: &str) -> Result<()> {
        use crate::db::appdata_schema::books::dsl::*;

        self.do_write(|db_conn| {
            diesel::delete(books.filter(uid.eq(book_uid_param)))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn update_book_metadata(&self, book_uid_param: &str, title_param: &str, author_param: &str, language_param: &str, enable_embedded_css_param: bool) -> Result<()> {
        use crate::db::appdata_schema::books::dsl::*;

        self.do_write(|db_conn| {
            diesel::update(books.filter(uid.eq(book_uid_param)))
                .set((
                    title.eq(Some(title_param)),
                    author.eq(if author_param.is_empty() { None } else { Some(author_param) }),
                    language.eq(if language_param.is_empty() { None } else { Some(language_param) }),
                    enable_embedded_css.eq(enable_embedded_css_param),
                ))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    // === Chanting CRUD operations ===

    /// Get all chanting collections with nested chants and sections as a JSON-serializable tree
    pub fn get_all_chanting_collections(&self) -> Result<Vec<ChantingCollectionJson>> {
        use crate::db::appdata_schema::chanting_collections::dsl as col_dsl;
        use crate::db::appdata_schema::chanting_chants::dsl as chant_dsl;
        use crate::db::appdata_schema::chanting_sections::dsl as sec_dsl;

        let collections: Vec<ChantingCollection> = self.do_read(|db_conn| {
            col_dsl::chanting_collections
                .select(ChantingCollection::as_select())
                .order(col_dsl::sort_index.asc())
                .load(db_conn)
        })?;

        let chants: Vec<ChantingChant> = self.do_read(|db_conn| {
            chant_dsl::chanting_chants
                .select(ChantingChant::as_select())
                .order(chant_dsl::sort_index.asc())
                .load(db_conn)
        })?;

        let sections: Vec<ChantingSection> = self.do_read(|db_conn| {
            sec_dsl::chanting_sections
                .select(ChantingSection::as_select())
                .order(sec_dsl::sort_index.asc())
                .load(db_conn)
        })?;

        // Build the tree structure
        let result: Vec<ChantingCollectionJson> = collections.into_iter().map(|col| {
            let col_chants: Vec<ChantingChantJson> = chants.iter()
                .filter(|c| c.collection_uid == col.uid)
                .map(|chant| {
                    let chant_sections: Vec<ChantingSectionJson> = sections.iter()
                        .filter(|s| s.chant_uid == chant.uid)
                        .map(|sec| ChantingSectionJson {
                            uid: sec.uid.clone(),
                            chant_uid: sec.chant_uid.clone(),
                            title: sec.title.clone(),
                            content_pali: sec.content_pali.clone(),
                            sort_index: sec.sort_index,
                            is_user_added: sec.is_user_added,
                            metadata_json: sec.metadata_json.clone(),
                            recordings: Vec::new(),
                        })
                        .collect();

                    ChantingChantJson {
                        uid: chant.uid.clone(),
                        collection_uid: chant.collection_uid.clone(),
                        title: chant.title.clone(),
                        description: chant.description.clone(),
                        sort_index: chant.sort_index,
                        is_user_added: chant.is_user_added,
                        metadata_json: chant.metadata_json.clone(),
                        sections: chant_sections,
                    }
                })
                .collect();

            ChantingCollectionJson {
                uid: col.uid.clone(),
                title: col.title.clone(),
                description: col.description.clone(),
                language: col.language.clone(),
                sort_index: col.sort_index,
                is_user_added: col.is_user_added,
                metadata_json: col.metadata_json.clone(),
                chants: col_chants,
            }
        }).collect();

        Ok(result)
    }

    /// Get section detail with all associated recordings
    pub fn get_chanting_section_detail(&self, section_uid_param: &str) -> Result<Option<ChantingSectionJson>> {
        use crate::db::appdata_schema::chanting_sections::dsl as sec_dsl;
        use crate::db::appdata_schema::chanting_recordings::dsl as rec_dsl;

        let section: Option<ChantingSection> = self.do_read(|db_conn| {
            sec_dsl::chanting_sections
                .filter(sec_dsl::uid.eq(section_uid_param))
                .select(ChantingSection::as_select())
                .first(db_conn)
                .optional()
        })?;

        match section {
            Some(sec) => {
                let recordings: Vec<ChantingRecording> = self.do_read(|db_conn| {
                    rec_dsl::chanting_recordings
                        .filter(rec_dsl::section_uid.eq(section_uid_param))
                        .select(ChantingRecording::as_select())
                        .load(db_conn)
                })?;

                let recording_jsons: Vec<ChantingRecordingJson> = recordings.into_iter().map(|r| {
                    ChantingRecordingJson {
                        uid: r.uid,
                        section_uid: r.section_uid,
                        file_name: r.file_name,
                        recording_type: r.recording_type,
                        label: r.label,
                        duration_ms: r.duration_ms,
                        markers_json: r.markers_json,
                        volume: r.volume,
                        playback_position_ms: r.playback_position_ms,
                        waveform_json: r.waveform_json,
                        is_user_added: r.is_user_added,
                    }
                }).collect();

                Ok(Some(ChantingSectionJson {
                    uid: sec.uid,
                    chant_uid: sec.chant_uid,
                    title: sec.title,
                    content_pali: sec.content_pali,
                    sort_index: sec.sort_index,
                    is_user_added: sec.is_user_added,
                    metadata_json: sec.metadata_json,
                    recordings: recording_jsons,
                }))
            }
            None => Ok(None),
        }
    }

    // --- Existence checks by uid (used by the post-upgrade import path) ---

    pub fn chanting_collection_exists_by_uid(&self, check_uid: &str) -> Result<bool> {
        use crate::db::appdata_schema::chanting_collections::dsl::*;
        self.do_read(|db_conn| {
            diesel::select(diesel::dsl::exists(
                chanting_collections.filter(uid.eq(check_uid)),
            ))
            .get_result::<bool>(db_conn)
        })
    }

    pub fn chanting_chant_exists_by_uid(&self, check_uid: &str) -> Result<bool> {
        use crate::db::appdata_schema::chanting_chants::dsl::*;
        self.do_read(|db_conn| {
            diesel::select(diesel::dsl::exists(
                chanting_chants.filter(uid.eq(check_uid)),
            ))
            .get_result::<bool>(db_conn)
        })
    }

    pub fn chanting_section_exists_by_uid(&self, check_uid: &str) -> Result<bool> {
        use crate::db::appdata_schema::chanting_sections::dsl::*;
        self.do_read(|db_conn| {
            diesel::select(diesel::dsl::exists(
                chanting_sections.filter(uid.eq(check_uid)),
            ))
            .get_result::<bool>(db_conn)
        })
    }

    pub fn chanting_recording_exists_by_uid(&self, check_uid: &str) -> Result<bool> {
        use crate::db::appdata_schema::chanting_recordings::dsl::*;
        self.do_read(|db_conn| {
            diesel::select(diesel::dsl::exists(
                chanting_recordings.filter(uid.eq(check_uid)),
            ))
            .get_result::<bool>(db_conn)
        })
    }

    // --- Collection CRUD ---

    pub fn create_chanting_collection(&self, data: &ChantingCollectionJson) -> Result<()> {
        use crate::db::appdata_schema::chanting_collections::dsl::*;

        let new = NewChantingCollection {
            uid: &data.uid,
            title: &data.title,
            description: data.description.as_deref(),
            language: &data.language,
            sort_index: data.sort_index,
            is_user_added: data.is_user_added,
            metadata_json: data.metadata_json.as_deref(),
        };

        self.do_write(|db_conn| {
            diesel::insert_into(chanting_collections)
                .values(&new)
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn update_chanting_collection(&self, data: &ChantingCollectionJson) -> Result<()> {
        use crate::db::appdata_schema::chanting_collections::dsl::*;

        self.do_write(|db_conn| {
            diesel::update(chanting_collections.filter(uid.eq(&data.uid)))
                .set((
                    title.eq(&data.title),
                    description.eq(&data.description),
                    language.eq(&data.language),
                    sort_index.eq(data.sort_index),
                    metadata_json.eq(&data.metadata_json),
                ))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn delete_chanting_collection(&self, collection_uid_param: &str) -> Result<()> {
        use crate::db::appdata_schema::chanting_collections::dsl::*;

        // Delete recordings files for all sections in all chants of this collection
        self.delete_recording_files_for_collection(collection_uid_param)?;

        // CASCADE will handle deleting chants, sections, and recordings rows
        self.do_write(|db_conn| {
            diesel::delete(chanting_collections.filter(uid.eq(collection_uid_param)))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    // --- Chant CRUD ---

    pub fn create_chanting_chant(&self, data: &ChantingChantJson) -> Result<()> {
        use crate::db::appdata_schema::chanting_chants::dsl::*;

        let new = NewChantingChant {
            uid: &data.uid,
            collection_uid: &data.collection_uid,
            title: &data.title,
            description: data.description.as_deref(),
            sort_index: data.sort_index,
            is_user_added: data.is_user_added,
            metadata_json: data.metadata_json.as_deref(),
        };

        self.do_write(|db_conn| {
            diesel::insert_into(chanting_chants)
                .values(&new)
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn update_chanting_chant(&self, data: &ChantingChantJson) -> Result<()> {
        use crate::db::appdata_schema::chanting_chants::dsl::*;

        self.do_write(|db_conn| {
            diesel::update(chanting_chants.filter(uid.eq(&data.uid)))
                .set((
                    title.eq(&data.title),
                    description.eq(&data.description),
                    sort_index.eq(data.sort_index),
                    metadata_json.eq(&data.metadata_json),
                ))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn delete_chanting_chant(&self, chant_uid_param: &str) -> Result<()> {
        use crate::db::appdata_schema::chanting_chants::dsl::*;

        self.delete_recording_files_for_chant(chant_uid_param)?;

        self.do_write(|db_conn| {
            diesel::delete(chanting_chants.filter(uid.eq(chant_uid_param)))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    // --- Section CRUD ---

    pub fn create_chanting_section(&self, data: &ChantingSectionJson) -> Result<()> {
        use crate::db::appdata_schema::chanting_sections::dsl::*;

        let new = NewChantingSection {
            uid: &data.uid,
            chant_uid: &data.chant_uid,
            title: &data.title,
            content_pali: &data.content_pali,
            sort_index: data.sort_index,
            is_user_added: data.is_user_added,
            metadata_json: data.metadata_json.as_deref(),
        };

        self.do_write(|db_conn| {
            diesel::insert_into(chanting_sections)
                .values(&new)
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn update_chanting_section(&self, data: &ChantingSectionJson) -> Result<()> {
        use crate::db::appdata_schema::chanting_sections::dsl::*;

        self.do_write(|db_conn| {
            diesel::update(chanting_sections.filter(uid.eq(&data.uid)))
                .set((
                    title.eq(&data.title),
                    content_pali.eq(&data.content_pali),
                    sort_index.eq(data.sort_index),
                    metadata_json.eq(&data.metadata_json),
                ))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn delete_chanting_section(&self, section_uid_param: &str) -> Result<()> {
        use crate::db::appdata_schema::chanting_sections::dsl::*;

        self.delete_recording_files_for_section(section_uid_param)?;

        self.do_write(|db_conn| {
            diesel::delete(chanting_sections.filter(uid.eq(section_uid_param)))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    // --- Recording CRUD ---

    pub fn create_chanting_recording(&self, data: &ChantingRecordingJson) -> Result<()> {
        use crate::db::appdata_schema::chanting_recordings::dsl::*;

        // Fill in the recording's duration from the audio file itself if the
        // caller didn't provide one. This keeps the list UI's "label (MM:SS)"
        // rendering a pure read of the stored row, with no need to wait for
        // MediaPlayer to load the file.
        let resolved_duration_ms = if data.duration_ms > 0 {
            data.duration_ms
        } else {
            let recordings_dir = crate::get_chanting_recordings_dir();
            let abs_path = if std::path::Path::new(&data.file_name).is_absolute() {
                std::path::PathBuf::from(&data.file_name)
            } else {
                recordings_dir.join(&data.file_name)
            };
            crate::waveform::get_audio_duration_ms(&abs_path.to_string_lossy())
        };

        let new = NewChantingRecording {
            uid: &data.uid,
            section_uid: &data.section_uid,
            file_name: &data.file_name,
            recording_type: &data.recording_type,
            label: data.label.as_deref(),
            duration_ms: resolved_duration_ms,
            markers_json: data.markers_json.as_deref(),
            volume: data.volume,
            playback_position_ms: data.playback_position_ms,
            waveform_json: data.waveform_json.as_deref(),
            is_user_added: data.is_user_added,
        };

        self.do_write(|db_conn| {
            diesel::insert_into(chanting_recordings)
                .values(&new)
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn delete_chanting_recording(&self, recording_uid_param: &str) -> Result<()> {
        use crate::db::appdata_schema::chanting_recordings::dsl::*;

        // Get the recording to find its file_name before deleting
        let recording: Option<ChantingRecording> = self.do_read(|db_conn| {
            chanting_recordings
                .filter(uid.eq(recording_uid_param))
                .select(ChantingRecording::as_select())
                .first(db_conn)
                .optional()
        })?;

        if let Some(rec) = &recording {
            self.delete_recording_file(&rec.file_name);
        }

        self.do_write(|db_conn| {
            diesel::delete(chanting_recordings.filter(uid.eq(recording_uid_param)))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn update_recording_label(&self, recording_uid_param: &str, new_label: &str) -> Result<()> {
        use crate::db::appdata_schema::chanting_recordings::dsl::*;

        self.do_write(|db_conn| {
            diesel::update(chanting_recordings.filter(uid.eq(recording_uid_param)))
                .set(label.eq(Some(new_label)))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn update_recording_markers(&self, recording_uid_param: &str, new_markers_json: &str) -> Result<()> {
        use crate::db::appdata_schema::chanting_recordings::dsl::*;

        self.do_write(|db_conn| {
            diesel::update(chanting_recordings.filter(uid.eq(recording_uid_param)))
                .set(markers_json.eq(Some(new_markers_json)))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn update_recording_volume(&self, recording_uid_param: &str, new_volume: f32) -> Result<()> {
        use crate::db::appdata_schema::chanting_recordings::dsl::*;

        self.do_write(|db_conn| {
            diesel::update(chanting_recordings.filter(uid.eq(recording_uid_param)))
                .set(volume.eq(new_volume))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn update_recording_playback_position(&self, recording_uid_param: &str, position_ms: i32) -> Result<()> {
        use crate::db::appdata_schema::chanting_recordings::dsl::*;

        self.do_write(|db_conn| {
            diesel::update(chanting_recordings.filter(uid.eq(recording_uid_param)))
                .set(playback_position_ms.eq(position_ms))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn update_recording_waveform(&self, recording_uid_param: &str, new_waveform_json: &str) -> Result<()> {
        use crate::db::appdata_schema::chanting_recordings::dsl::*;

        self.do_write(|db_conn| {
            diesel::update(chanting_recordings.filter(uid.eq(recording_uid_param)))
                .set(waveform_json.eq(Some(new_waveform_json)))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    // --- Recording file cleanup helpers ---

    fn delete_recording_file(&self, file_name: &str) {
        let recordings_dir = crate::get_chanting_recordings_dir();
        let file_path = recordings_dir.join(file_name);

        match file_path.try_exists() {
            Ok(true) => {
                if let Err(e) = std::fs::remove_file(&file_path) {
                    error(&format!("Failed to delete recording file {:?}: {}", file_path, e));
                }
            }
            Ok(false) => {} // File doesn't exist, nothing to do
            Err(e) => {
                error(&format!("Failed to check recording file {:?}: {}", file_path, e));
            }
        }
    }

    fn delete_recording_files_for_section(&self, section_uid_param: &str) -> Result<()> {
        use crate::db::appdata_schema::chanting_recordings::dsl as rec_dsl;

        let recordings: Vec<ChantingRecording> = self.do_read(|db_conn| {
            rec_dsl::chanting_recordings
                .filter(rec_dsl::section_uid.eq(section_uid_param))
                .select(ChantingRecording::as_select())
                .load(db_conn)
        })?;

        for rec in &recordings {
            self.delete_recording_file(&rec.file_name);
        }

        Ok(())
    }

    fn delete_recording_files_for_chant(&self, chant_uid_param: &str) -> Result<()> {
        use crate::db::appdata_schema::chanting_sections::dsl as sec_dsl;

        let sections: Vec<ChantingSection> = self.do_read(|db_conn| {
            sec_dsl::chanting_sections
                .filter(sec_dsl::chant_uid.eq(chant_uid_param))
                .select(ChantingSection::as_select())
                .load(db_conn)
        })?;

        for sec in &sections {
            self.delete_recording_files_for_section(&sec.uid)?;
        }

        Ok(())
    }

    pub fn get_chanting_collections_by_uids(&self, uids: &[String]) -> Result<Vec<ChantingCollection>> {
        use crate::db::appdata_schema::chanting_collections::dsl::*;
        self.do_read(|db_conn| {
            chanting_collections
                .filter(uid.eq_any(uids))
                .select(ChantingCollection::as_select())
                .order(sort_index.asc())
                .load(db_conn)
        })
    }

    pub fn get_chanting_chants_by_uids(&self, uids: &[String]) -> Result<Vec<ChantingChant>> {
        use crate::db::appdata_schema::chanting_chants::dsl::*;
        self.do_read(|db_conn| {
            chanting_chants
                .filter(uid.eq_any(uids))
                .select(ChantingChant::as_select())
                .order(sort_index.asc())
                .load(db_conn)
        })
    }

    pub fn get_chanting_sections_by_uids(&self, uids: &[String]) -> Result<Vec<ChantingSection>> {
        use crate::db::appdata_schema::chanting_sections::dsl::*;
        self.do_read(|db_conn| {
            chanting_sections
                .filter(uid.eq_any(uids))
                .select(ChantingSection::as_select())
                .order(sort_index.asc())
                .load(db_conn)
        })
    }

    pub fn get_chanting_recordings_for_sections(&self, section_uids: &[String]) -> Result<Vec<ChantingRecording>> {
        use crate::db::appdata_schema::chanting_recordings::dsl::*;
        let mut rows: Vec<ChantingRecording> = self.do_read(|db_conn| {
            chanting_recordings
                .filter(section_uid.eq_any(section_uids))
                .select(ChantingRecording::as_select())
                .load(db_conn)
        })?;

        // Lazily backfill duration_ms for legacy rows whose duration was never
        // persisted at creation time (pre-duration-probe recordings, imported
        // rows, etc.), so the list UI can render "label (MM:SS)" straight from
        // the stored row.
        let recordings_dir = crate::get_chanting_recordings_dir();
        for rec in rows.iter_mut() {
            if rec.duration_ms > 0 || rec.file_name.is_empty() {
                continue;
            }
            let abs_path = if std::path::Path::new(&rec.file_name).is_absolute() {
                std::path::PathBuf::from(&rec.file_name)
            } else {
                recordings_dir.join(&rec.file_name)
            };
            let ms = crate::waveform::get_audio_duration_ms(&abs_path.to_string_lossy());
            if ms > 0 {
                if let Err(e) = self.update_recording_duration(&rec.uid, ms) {
                    crate::logger::warn(&format!(
                        "Failed to backfill duration_ms for recording {}: {}",
                        rec.uid, e
                    ));
                } else {
                    rec.duration_ms = ms;
                }
            }
        }

        Ok(rows)
    }

    pub fn update_recording_duration(&self, recording_uid_param: &str, new_duration_ms: i32) -> Result<()> {
        use crate::db::appdata_schema::chanting_recordings::dsl::*;

        self.do_write(|db_conn| {
            diesel::update(chanting_recordings.filter(uid.eq(recording_uid_param)))
                .set(duration_ms.eq(new_duration_ms))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    fn delete_recording_files_for_collection(&self, collection_uid_param: &str) -> Result<()> {
        use crate::db::appdata_schema::chanting_chants::dsl as chant_dsl;

        let chants: Vec<ChantingChant> = self.do_read(|db_conn| {
            chant_dsl::chanting_chants
                .filter(chant_dsl::collection_uid.eq(collection_uid_param))
                .select(ChantingChant::as_select())
                .load(db_conn)
        })?;

        for chant in &chants {
            self.delete_recording_files_for_chant(&chant.uid)?;
        }

        Ok(())
    }
}

pub fn delete_sutta() {
    use crate::db::appdata_schema::suttas::dsl::*;

    let pattern = "unwholesome";

    let app_data = get_app_data();
    let _lock = app_data.dbm.appdata.write_lock.lock();
    let db_conn = &mut app_data.dbm.appdata.get_conn().expect("Can't get db conn");

    let num_deleted = diesel::delete(suttas.filter(content_html.like(pattern)))
        .execute(db_conn)
        .expect("Error deleting suttas");

    info(&format!("Deleted {} suttas", num_deleted));
}

pub fn sort_suttas(res: Vec<Sutta>) -> Vec<Sutta> {
    // Sort Pali ms first as the results.
    // Then add Pali other sources,
    // then the non-Pali items, sorted by language.
    //
    // Single-pass manual bucketing means we walk the vector once,
    // avoiding per-element cloning.

    let mut results = Vec::new();
    let mut pli_others = Vec::new();
    let mut remaining = Vec::new();

    for s in res.into_iter() {
        if s.language == "pli" {
            if s.uid.ends_with("/ms") {
                results.push(s);
            } else {
                pli_others.push(s);
            }
        } else {
            remaining.push(s);
        }
    }

    // Sort pli_others so mūla (e.g. mn1/pli/cst) comes before
    // commentary (e.g. mn1.att/pli/cst, mn1.tik/pli/cst).
    // Commentary UIDs contain .att or .tik before the first '/'.
    pli_others.sort_by(|a, b| {
        let a_ref = a.uid.split('/').next().unwrap_or("");
        let b_ref = b.uid.split('/').next().unwrap_or("");
        let a_is_commentary = a_ref.contains(".att") || a_ref.contains(".tik");
        let b_is_commentary = b_ref.contains(".att") || b_ref.contains(".tik");
        a_is_commentary.cmp(&b_is_commentary).then_with(|| a.uid.cmp(&b.uid))
    });
    // Sort non-pli by language, then by uid within the same language
    remaining.sort_by(|a, b| a.language.cmp(&b.language).then_with(|| a.uid.cmp(&b.uid)));
    // Assemble final list
    results.extend(pli_others);
    results.extend(remaining);
    results
}

impl AppdataDbHandle {
    /// Remove suttas and related data for specific language codes
    /// Returns true if deletion was successful
    /// The progress_callback is called after each language is removed with (current_index, total_count, language_code)
    pub fn remove_sutta_languages<F>(&self, language_codes: Vec<String>, mut progress_callback: F) -> Result<bool>
    where
        F: FnMut(usize, usize, &str),
    {
        use crate::db::appdata_schema;

        if language_codes.is_empty() {
            return Ok(true);
        }

        info(&format!("remove_sutta_languages(): Removing languages: {:?}", language_codes));

        let total_count = language_codes.len();
        let mut any_deleted = false;

        // Process each language one by one to provide progress updates
        for (index, lang_code) in language_codes.iter().enumerate() {
            let current_index = index + 1;
            info(&format!("Removing language {}/{}: {}", current_index, total_count, lang_code));

            // Call progress callback BEFORE starting to remove this language
            progress_callback(current_index, total_count, lang_code);

            let result = self.do_write(|db_conn| {
                // Delete suttas for this language
                // SQLite automatically handles CASCADE DELETE for child tables
                // (sutta_variants, sutta_comments, sutta_glosses) because:
                // 1. Foreign keys have ON DELETE CASCADE in the schema
                // 2. Foreign keys are enabled via PRAGMA foreign_keys = ON (see ConnectionCustomizer)
                // 3. Diesel's delete() executes standard SQL DELETE which respects CASCADE
                let suttas_deleted = diesel::delete(
                    appdata_schema::suttas::table
                        .filter(appdata_schema::suttas::language.eq(lang_code))
                ).execute(db_conn)?;

                info(&format!("Deleted {} suttas for language {} (child records deleted via CASCADE)", suttas_deleted, lang_code));

                Ok(suttas_deleted > 0)
            });

            match result {
                Ok(deleted) => {
                    if deleted {
                        any_deleted = true;
                    }
                    info(&format!("Successfully removed language {}", lang_code));
                },
                Err(e) => {
                    error(&format!("Failed to remove language {}: {}", lang_code, e));
                    return Err(e);
                }
            }
        }

        info("remove_sutta_languages(): All languages removed successfully");

        // Refresh the per-area language caches off the calling thread so the
        // search-bar language filter dropdown reflects the removal on the
        // next area switch.
        if any_deleted {
            std::thread::spawn(|| {
                if let Some(app_data) = crate::try_get_app_data() {
                    app_data.refresh_language_caches();
                }
            });
        }

        Ok(any_deleted)
    }

    /// Get sutta languages with their counts in format "code|Name|Count"
    /// Returns a vector of strings sorted alphabetically by language code
    pub fn get_sutta_language_labels_with_counts(&self) -> Vec<String> {
        use crate::db::appdata_schema;
        use crate::lookup::LANG_CODE_TO_NAME;

        let result = self.do_read(|db_conn| {
            appdata_schema::suttas::table
                .group_by(appdata_schema::suttas::language)
                .select((appdata_schema::suttas::language, diesel::dsl::count(appdata_schema::suttas::id)))
                .load::<(String, i64)>(db_conn)
        });

        match result {
            Ok(lang_counts) => {
                let mut labels: Vec<String> = lang_counts
                    .into_iter()
                    .filter(|(lang, _)| !lang.is_empty())
                    .map(|(lang_code, count)| {
                        let lang_name = LANG_CODE_TO_NAME
                            .get(lang_code.as_str())
                            .copied()
                            .unwrap_or(&lang_code);
                        format!("{}|{}|{}", lang_code, lang_name, count)
                    })
                    .collect();

                // Sort alphabetically by language code
                labels.sort();
                labels
            },
            Err(e) => {
                error(&format!("get_sutta_language_labels_with_counts(): {}", e));
                Vec::new()
            }
        }
    }

    // --- Bookmark Folder CRUD ---

    pub fn get_all_bookmark_folders(&self) -> Vec<BookmarkFolder> {
        use crate::db::appdata_schema::bookmark_folders::dsl::*;

        let result = self.do_read(|db_conn| {
            bookmark_folders
                .order(sort_order.asc())
                .select(BookmarkFolder::as_select())
                .load(db_conn)
        });

        match result {
            Ok(folders) => folders,
            Err(e) => {
                error(&format!("get_all_bookmark_folders(): {}", e));
                Vec::new()
            }
        }
    }

    pub fn get_bookmark_items_for_folder(&self, folder_id_param: i32) -> Vec<BookmarkItem> {
        use crate::db::appdata_schema::bookmark_items::dsl::*;

        let result = self.do_read(|db_conn| {
            bookmark_items
                .filter(folder_id.eq(folder_id_param))
                .order(sort_order.asc())
                .select(BookmarkItem::as_select())
                .load(db_conn)
        });

        match result {
            Ok(items) => items,
            Err(e) => {
                error(&format!("get_bookmark_items_for_folder(): {}", e));
                Vec::new()
            }
        }
    }

    pub fn create_bookmark_folder(&self, name_param: &str, is_last_session_param: bool) -> Result<i32> {
        use crate::db::appdata_schema::bookmark_folders::dsl::*;

        // Get max sort_order
        let max_order: i32 = self.do_read(|db_conn| {
            bookmark_folders
                .select(diesel::dsl::max(sort_order))
                .first::<Option<i32>>(db_conn)
        })?.unwrap_or(0);

        let new_folder = NewBookmarkFolder {
            name: name_param,
            sort_order: max_order + 1,
            is_last_session: is_last_session_param,
            is_user_added: true,
        };

        self.do_write(|db_conn| {
            diesel::insert_into(bookmark_folders)
                .values(&new_folder)
                .execute(db_conn)?;

            // Get the last inserted row id
            bookmark_folders
                .order(id.desc())
                .select(id)
                .first::<i32>(db_conn)
        })
    }

    pub fn create_bookmark_item(&self, new_item: &NewBookmarkItem) -> Result<i32> {
        use crate::db::appdata_schema::bookmark_items::dsl::*;

        // Get max sort_order within folder
        let max_order: i32 = self.do_read(|db_conn| {
            bookmark_items
                .filter(folder_id.eq(new_item.folder_id))
                .select(diesel::dsl::max(sort_order))
                .first::<Option<i32>>(db_conn)
        })?.unwrap_or(0);

        let item = NewBookmarkItem {
            folder_id: new_item.folder_id,
            item_uid: new_item.item_uid.clone(),
            table_name: new_item.table_name.clone(),
            title: new_item.title.clone(),
            tab_group: new_item.tab_group.clone(),
            scroll_position: new_item.scroll_position,
            find_query: new_item.find_query.clone(),
            find_match_index: new_item.find_match_index,
            sort_order: max_order + 1,
            is_user_added: new_item.is_user_added,
        };

        self.do_write(|db_conn| {
            diesel::insert_into(bookmark_items)
                .values(&item)
                .execute(db_conn)?;

            bookmark_items
                .order(id.desc())
                .select(id)
                .first::<i32>(db_conn)
        })
    }

    pub fn update_bookmark_folder(&self, folder_id_param: i32, name_param: &str) -> Result<()> {
        use crate::db::appdata_schema::bookmark_folders::dsl::*;

        self.do_write(|db_conn| {
            diesel::update(bookmark_folders.find(folder_id_param))
                .set(name.eq(name_param))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn update_bookmark_item(&self, item_id_param: i32, item_data: &BookmarkItemUpdate) -> Result<()> {
        use crate::db::appdata_schema::bookmark_items::dsl::*;

        self.do_write(|db_conn| {
            if let Some(ref v) = item_data.item_uid {
                diesel::update(bookmark_items.find(item_id_param))
                    .set(item_uid.eq(v))
                    .execute(db_conn)?;
            }
            if let Some(ref v) = item_data.title {
                diesel::update(bookmark_items.find(item_id_param))
                    .set(title.eq(v))
                    .execute(db_conn)?;
            }
            if let Some(ref v) = item_data.tab_group {
                diesel::update(bookmark_items.find(item_id_param))
                    .set(tab_group.eq(v))
                    .execute(db_conn)?;
            }
            if let Some(ref v) = item_data.find_query {
                diesel::update(bookmark_items.find(item_id_param))
                    .set(find_query.eq(v))
                    .execute(db_conn)?;
            }
            if let Some(v) = item_data.find_match_index {
                diesel::update(bookmark_items.find(item_id_param))
                    .set(find_match_index.eq(v))
                    .execute(db_conn)?;
            }
            Ok(())
        })
    }

    pub fn delete_bookmark_folder(&self, folder_id_param: i32) -> Result<()> {
        use crate::db::appdata_schema::bookmark_folders::dsl::*;

        self.do_write(|db_conn| {
            diesel::delete(bookmark_folders.find(folder_id_param))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn delete_bookmark_item(&self, item_id_param: i32) -> Result<()> {
        use crate::db::appdata_schema::bookmark_items::dsl::*;

        self.do_write(|db_conn| {
            diesel::delete(bookmark_items.find(item_id_param))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn reorder_bookmark_items(&self, folder_id_param: i32, item_ids: &[i32]) -> Result<()> {
        use crate::db::appdata_schema::bookmark_items::dsl::*;

        self.do_write(|db_conn| {
            for (idx, item_id_val) in item_ids.iter().enumerate() {
                diesel::update(
                    bookmark_items
                        .filter(id.eq(item_id_val))
                        .filter(folder_id.eq(folder_id_param))
                )
                    .set(sort_order.eq(idx as i32))
                    .execute(db_conn)?;
            }
            Ok(())
        })
    }

    pub fn reorder_bookmark_folders(&self, folder_ids: &[i32]) -> Result<()> {
        use crate::db::appdata_schema::bookmark_folders::dsl::*;

        self.do_write(|db_conn| {
            for (idx, folder_id_val) in folder_ids.iter().enumerate() {
                diesel::update(bookmark_folders.find(*folder_id_val))
                    .set(sort_order.eq(idx as i32))
                    .execute(db_conn)?;
            }
            Ok(())
        })
    }

    pub fn move_bookmark_items_to_folder(&self, item_ids: &[i32], target_folder_id: i32) -> Result<()> {
        use crate::db::appdata_schema::bookmark_items::dsl::*;

        // Get max sort_order in target folder
        let max_order: i32 = self.do_read(|db_conn| {
            bookmark_items
                .filter(folder_id.eq(target_folder_id))
                .select(diesel::dsl::max(sort_order))
                .first::<Option<i32>>(db_conn)
        })?.unwrap_or(0);

        self.do_write(|db_conn| {
            for (idx, item_id_val) in item_ids.iter().enumerate() {
                diesel::update(bookmark_items.find(*item_id_val))
                    .set((
                        folder_id.eq(target_folder_id),
                        sort_order.eq(max_order + 1 + idx as i32),
                    ))
                    .execute(db_conn)?;
            }
            Ok(())
        })
    }

    pub fn delete_last_session_folders(&self) -> Result<()> {
        use crate::db::appdata_schema::bookmark_folders::dsl::*;

        self.do_write(|db_conn| {
            diesel::delete(bookmark_folders.filter(is_last_session.eq(true)))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn get_last_session_folders(&self) -> Vec<BookmarkFolder> {
        use crate::db::appdata_schema::bookmark_folders::dsl::*;

        let result = self.do_read(|db_conn| {
            bookmark_folders
                .filter(is_last_session.eq(true))
                .order(sort_order.asc())
                .select(BookmarkFolder::as_select())
                .load(db_conn)
        });

        match result {
            Ok(folders) => folders,
            Err(e) => {
                error(&format!("get_last_session_folders(): {}", e));
                Vec::new()
            }
        }
    }

    // Gloss / Prompts history CRUD.
    //
    // NOTE: intentionally NO per-save `ANALYZE` here. `DatabaseHandle::analyze`
    // runs a full-DB ANALYZE over all appdata tables, which would be wasteful on
    // every 60s autosave. It is also unnecessary: the only query is a
    // single-table equality + order (`WHERE item_type = ? ORDER BY updated_at
    // DESC`) fully served by the `(item_type, updated_at)` index, which SQLite
    // plans correctly without stats. See
    // docs/user-data-and-sqlite-analyze.md (the slow-query case there was a
    // multi-table join, not this shape).

    pub fn get_history_for_type(&self, item_type_param: HistoryItemType) -> Vec<GlossPromptsHistory> {
        use crate::db::appdata_schema::gloss_prompts_history::dsl::*;

        let result = self.do_read(|db_conn| {
            gloss_prompts_history
                .filter(item_type.eq(item_type_param.as_str()))
                .order(updated_at.desc())
                .select(GlossPromptsHistory::as_select())
                .load(db_conn)
        });

        match result {
            Ok(items) => items,
            Err(e) => {
                error(&format!("get_history_for_type(): {}", e));
                Vec::new()
            }
        }
    }

    pub fn save_new_history(&self, item_type_param: HistoryItemType, data_json_param: &str) -> Result<i32> {
        use crate::db::appdata_schema::gloss_prompts_history::dsl::*;

        let now = chrono::Utc::now().naive_utc();
        let item = NewGlossPromptsHistory {
            item_type: item_type_param.as_str(),
            data_json: data_json_param,
            created_at: Some(now),
            updated_at: Some(now),
        };

        self.do_write(|db_conn| {
            diesel::insert_into(gloss_prompts_history)
                .values(&item)
                .execute(db_conn)?;

            gloss_prompts_history
                .order(id.desc())
                .select(id)
                .first::<i32>(db_conn)
        })
    }

    /// Updates an existing history row, returning the number of rows affected
    /// (0 if `id_param` no longer exists, e.g. it was deleted/cleared). Callers
    /// use the count to fall back to an INSERT rather than silently losing data.
    pub fn update_history(&self, id_param: i32, data_json_param: &str) -> Result<usize> {
        use crate::db::appdata_schema::gloss_prompts_history::dsl::*;

        let now = chrono::Utc::now().naive_utc();
        self.do_write(|db_conn| {
            diesel::update(gloss_prompts_history.find(id_param))
                .set((
                    data_json.eq(data_json_param),
                    updated_at.eq(Some(now)),
                ))
                .execute(db_conn)
        })
    }

    pub fn delete_history_item(&self, id_param: i32) -> Result<()> {
        use crate::db::appdata_schema::gloss_prompts_history::dsl::*;

        self.do_write(|db_conn| {
            diesel::delete(gloss_prompts_history.find(id_param))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    pub fn clear_history(&self, item_type_param: HistoryItemType) -> Result<()> {
        use crate::db::appdata_schema::gloss_prompts_history::dsl::*;

        self.do_write(|db_conn| {
            diesel::delete(gloss_prompts_history.filter(item_type.eq(item_type_param.as_str())))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    // Gloss word-context cache CRUD (AI word selection feature).
    //
    // NOTE: intentionally NO per-write `ANALYZE` here, same rationale as the
    // history CRUD above — the queries are single-table equality lookups fully
    // served by the UNIQUE `(word, context_hash)` index. See
    // docs/user-data-and-sqlite-analyze.md.
    //
    // Callers pass `word` already normalized via `gloss_cache_word_key` and
    // `context_hash` via `gloss_context_hash(normalize_gloss_context(...))`.

    pub fn get_gloss_word_cache(&self, word_param: &str, context_hash_param: &str) -> Option<GlossWordContextCache> {
        use crate::db::appdata_schema::gloss_word_context_cache::dsl::*;

        let result = self.do_read(|db_conn| {
            gloss_word_context_cache
                .filter(word.eq(word_param))
                .filter(context_hash.eq(context_hash_param))
                .select(GlossWordContextCache::as_select())
                .first(db_conn)
                .optional()
        });

        match result {
            Ok(row) => row,
            Err(e) => {
                error(&format!("get_gloss_word_cache(): {}", e));
                None
            }
        }
    }

    /// Fetch all cache rows matching the given `(word, context_hash)` pairs in
    /// one query (used to pre-fetch a paragraph's rows before gloss processing).
    pub fn get_gloss_word_cache_batch(&self, pairs: &[(String, String)]) -> Vec<GlossWordContextCache> {
        use crate::db::appdata_schema::gloss_word_context_cache::dsl::*;

        if pairs.is_empty() {
            return Vec::new();
        }

        let words: Vec<&str> = pairs.iter().map(|(w, _)| w.as_str()).collect();
        let hashes: Vec<&str> = pairs.iter().map(|(_, h)| h.as_str()).collect();

        let result = self.do_read(|db_conn| {
            gloss_word_context_cache
                .filter(word.eq_any(&words))
                .filter(context_hash.eq_any(&hashes))
                .select(GlossWordContextCache::as_select())
                .load(db_conn)
        });

        match result {
            Ok(rows) => {
                // The two eq_any filters form a cross product; keep only the
                // rows whose (word, context_hash) pair was actually requested.
                rows.into_iter()
                    .filter(|r| pairs.iter().any(|(w, h)| *w == r.word && *h == r.context_hash))
                    .collect()
            }
            Err(e) => {
                error(&format!("get_gloss_word_cache_batch(): {}", e));
                Vec::new()
            }
        }
    }

    /// Insert or update a cache row, respecting origin precedence
    /// (user > built-in > ai):
    /// - `user` overwrites anything;
    /// - `built-in` overwrites `built-in` and `ai`, never `user`;
    /// - `ai` only overwrites `ai`.
    ///
    /// Returns true when a row was written (inserted or updated).
    pub fn upsert_gloss_word_cache(
        &self,
        word_param: &str,
        context_hash_param: &str,
        context_snippet_param: &str,
        selected_uid_param: &str,
        origin_param: &str,
    ) -> Result<bool> {
        use crate::db::appdata_schema::gloss_word_context_cache::dsl::*;

        let existing = self.get_gloss_word_cache(word_param, context_hash_param);
        let now = chrono::Utc::now().naive_utc();

        match existing {
            None => {
                let new_row = NewGlossWordContextCache {
                    word: word_param,
                    context_hash: context_hash_param,
                    context_snippet: context_snippet_param,
                    selected_uid: selected_uid_param,
                    origin: origin_param,
                    created_at: Some(now),
                    updated_at: Some(now),
                };
                self.do_write(|db_conn| {
                    diesel::insert_into(gloss_word_context_cache)
                        .values(&new_row)
                        .execute(db_conn)
                })?;
                Ok(true)
            }
            Some(row) => {
                let new_rank = gloss_cache_origin_rank(origin_param);
                let old_rank = gloss_cache_origin_rank(&row.origin);
                // A lower-precedence origin never overwrites a higher one; an
                // equal-precedence write updates the row (e.g. a fresh AI
                // response refreshes an ai row, a user re-save refreshes a
                // user row).
                if new_rank < old_rank {
                    return Ok(false);
                }
                self.do_write(|db_conn| {
                    diesel::update(gloss_word_context_cache.find(row.id))
                        .set((
                            context_snippet.eq(context_snippet_param),
                            selected_uid.eq(selected_uid_param),
                            origin.eq(origin_param),
                            updated_at.eq(Some(now)),
                        ))
                        .execute(db_conn)
                })?;
                Ok(true)
            }
        }
    }

    pub fn delete_gloss_word_cache(&self, word_param: &str, context_hash_param: &str) -> Result<()> {
        use crate::db::appdata_schema::gloss_word_context_cache::dsl::*;

        self.do_write(|db_conn| {
            diesel::delete(
                gloss_word_context_cache
                    .filter(word.eq(word_param))
                    .filter(context_hash.eq(context_hash_param)),
            )
            .execute(db_conn)
            .map(|_| ())
        })
    }

    /// Count of user-clearable cache rows (`ai` + `user` origins; `built-in`
    /// rows are bootstrap-shipped and excluded).
    pub fn count_gloss_word_cache(&self) -> i64 {
        use crate::db::appdata_schema::gloss_word_context_cache::dsl::*;

        let result = self.do_read(|db_conn| {
            gloss_word_context_cache
                .filter(origin.eq_any(["ai", "user"]))
                .count()
                .get_result::<i64>(db_conn)
        });

        match result {
            Ok(n) => n,
            Err(e) => {
                error(&format!("count_gloss_word_cache(): {}", e));
                0
            }
        }
    }

    /// Bulk clear of the word-selection cache: deletes `ai` and `user` rows
    /// only. `built-in` rows and the phrase table are untouched.
    pub fn clear_gloss_word_cache(&self) -> Result<()> {
        use crate::db::appdata_schema::gloss_word_context_cache::dsl::*;

        self.do_write(|db_conn| {
            diesel::delete(gloss_word_context_cache.filter(origin.eq_any(["ai", "user"])))
                .execute(db_conn)
                .map(|_| ())
        })
    }

    // Gloss set-phrase selections.

    /// Phrase rules for one word key (`gloss_cache_word_key` form).
    pub fn get_gloss_phrase_selections(&self, word_param: &str) -> Vec<GlossPhraseSelection> {
        use crate::db::appdata_schema::gloss_phrase_selections::dsl::*;

        let result = self.do_read(|db_conn| {
            gloss_phrase_selections
                .filter(word.eq(word_param))
                .select(GlossPhraseSelection::as_select())
                .load(db_conn)
        });

        match result {
            Ok(rows) => rows,
            Err(e) => {
                error(&format!("get_gloss_phrase_selections(): {}", e));
                Vec::new()
            }
        }
    }

    /// The whole phrase table (it is tiny; pre-fetched before gloss processing).
    pub fn get_all_gloss_phrase_selections(&self) -> Vec<GlossPhraseSelection> {
        use crate::db::appdata_schema::gloss_phrase_selections::dsl::*;

        let result = self.do_read(|db_conn| {
            gloss_phrase_selections
                .select(GlossPhraseSelection::as_select())
                .load(db_conn)
        });

        match result {
            Ok(rows) => rows,
            Err(e) => {
                error(&format!("get_all_gloss_phrase_selections(): {}", e));
                Vec::new()
            }
        }
    }

    /// Seed `gloss_phrase_selections` from the embedded curated JSON
    /// (`assets/gloss-phrase-selections.json`). Idempotent upsert keyed on the
    /// normalized `(phrase, word)`: existing rows get their `selected_uid`
    /// updated, new rows are inserted. Called from the appdata bootstrap.
    ///
    /// The JSON stores the human-readable phrase; it is normalized here with
    /// the same `normalize_gloss_context` pipeline used for context windows,
    /// so phrase rows and window normalization cannot drift.
    pub fn seed_gloss_phrase_selections(&self) -> Result<usize> {
        use crate::db::appdata_schema::gloss_phrase_selections::dsl::*;
        use crate::helpers::{gloss_cache_word_key, normalize_gloss_context};

        #[derive(serde::Deserialize)]
        struct PhraseEntry {
            phrase: String,
            word: String,
            selected_uid: String,
        }

        #[derive(serde::Deserialize)]
        struct PhraseFile {
            phrases: Vec<PhraseEntry>,
        }

        let data: PhraseFile = serde_json::from_str(GLOSS_PHRASE_SELECTIONS_JSON)
            .context("Failed to parse gloss-phrase-selections.json")?;

        let mut written = 0;
        for entry in &data.phrases {
            let norm_phrase = normalize_gloss_context(&entry.phrase);
            let word_key = gloss_cache_word_key(&entry.word);
            if norm_phrase.is_empty() || word_key.is_empty() {
                continue;
            }

            let existing: Option<GlossPhraseSelection> = self.do_read(|db_conn| {
                gloss_phrase_selections
                    .filter(phrase.eq(&norm_phrase))
                    .filter(word.eq(&word_key))
                    .select(GlossPhraseSelection::as_select())
                    .first(db_conn)
                    .optional()
            })?;

            match existing {
                Some(row) => {
                    if row.selected_uid != entry.selected_uid {
                        self.do_write(|db_conn| {
                            diesel::update(gloss_phrase_selections.find(row.id))
                                .set(selected_uid.eq(&entry.selected_uid))
                                .execute(db_conn)
                        })?;
                        written += 1;
                    }
                }
                None => {
                    let new_row = NewGlossPhraseSelection {
                        phrase: &norm_phrase,
                        word: &word_key,
                        selected_uid: &entry.selected_uid,
                    };
                    self.do_write(|db_conn| {
                        diesel::insert_into(gloss_phrase_selections)
                            .values(&new_row)
                            .execute(db_conn)
                    })?;
                    written += 1;
                }
            }
        }

        Ok(written)
    }
}

#[cfg(test)]
mod history_tests {
    use super::AppdataDbHandle;
    use crate::db::{DatabaseHandle, APPDATA_MIGRATIONS};
    use crate::db::appdata_models::HistoryItemType;
    use diesel_migrations::MigrationHarness;

    // Build the appdata schema (including gloss_prompts_history) in a throwaway
    // temp SQLite DB so the CRUD tests never touch the shipped/user appdata DB
    // (clear/delete here would be destructive against the real one).
    fn setup() -> AppdataDbHandle {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "simsapa_history_test_{}_{}.sqlite3",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let url = path.to_string_lossy().to_string();
        let handle = DatabaseHandle::new(&url).expect("create temp appdata handle");
        let mut conn = handle.get_conn().expect("get temp appdata conn");
        conn.run_pending_migrations(APPDATA_MIGRATIONS)
            .expect("run appdata migrations on temp db");
        handle
    }

    #[test]
    fn save_new_returns_increasing_ids() {
        let db = setup();
        let id1 = db.save_new_history(HistoryItemType::Gloss, "{\"text\":\"a\"}").unwrap();
        let id2 = db.save_new_history(HistoryItemType::Gloss, "{\"text\":\"b\"}").unwrap();
        assert!(id1 > 0);
        assert!(id2 > id1);
    }

    #[test]
    fn update_changes_data_and_timestamp() {
        let db = setup();
        let id = db.save_new_history(HistoryItemType::Prompts, "{\"v\":1}").unwrap();
        let before = db.get_history_for_type(HistoryItemType::Prompts);
        let before_updated = before.iter().find(|r| r.id == id).unwrap().updated_at;

        std::thread::sleep(std::time::Duration::from_millis(5));
        let affected = db.update_history(id, "{\"v\":2}").unwrap();
        assert_eq!(affected, 1);

        let after = db.get_history_for_type(HistoryItemType::Prompts);
        let after_row = after.iter().find(|r| r.id == id).unwrap();
        assert_eq!(after_row.data_json, "{\"v\":2}");
        assert!(after_row.updated_at >= before_updated);
    }

    #[test]
    fn list_is_newest_first_and_type_scoped() {
        let db = setup();
        let g1 = db.save_new_history(HistoryItemType::Gloss, "g1").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let g2 = db.save_new_history(HistoryItemType::Gloss, "g2").unwrap();
        let _p1 = db.save_new_history(HistoryItemType::Prompts, "p1").unwrap();

        let gloss = db.get_history_for_type(HistoryItemType::Gloss);
        assert_eq!(gloss.len(), 2);
        // Newest-first: the more recently saved g2 comes before g1.
        assert_eq!(gloss[0].id, g2);
        assert_eq!(gloss[1].id, g1);
        assert!(gloss.iter().all(|r| r.item_type == "gloss"));

        // The prompts row must not leak into the gloss list and vice versa.
        let prompts = db.get_history_for_type(HistoryItemType::Prompts);
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].data_json, "p1");
    }

    #[test]
    fn delete_one_removes_only_that_row() {
        let db = setup();
        let id1 = db.save_new_history(HistoryItemType::Gloss, "g1").unwrap();
        let id2 = db.save_new_history(HistoryItemType::Gloss, "g2").unwrap();
        db.delete_history_item(id1).unwrap();
        let rows = db.get_history_for_type(HistoryItemType::Gloss);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id2);
    }

    #[test]
    fn clear_removes_only_matching_type() {
        let db = setup();
        db.save_new_history(HistoryItemType::Gloss, "g1").unwrap();
        db.save_new_history(HistoryItemType::Gloss, "g2").unwrap();
        db.save_new_history(HistoryItemType::Prompts, "p1").unwrap();

        db.clear_history(HistoryItemType::Gloss).unwrap();
        assert!(db.get_history_for_type(HistoryItemType::Gloss).is_empty());
        assert_eq!(db.get_history_for_type(HistoryItemType::Prompts).len(), 1);
    }

    #[test]
    fn update_nonexistent_id_affects_zero_rows() {
        // The bridge's save_history_session_impl relies on this 0-row signal to
        // fall back to an INSERT (PRD §10.1 stale-row contract), so it must hold.
        let db = setup();
        assert_eq!(db.update_history(999_999, "{\"v\":1}").unwrap(), 0);
    }

    #[test]
    fn save_after_clear_recreates_row() {
        // After clear_history removes the active session's row, an update by its
        // old id affects 0 rows; a fresh save then re-creates it (mirroring the
        // bridge INSERT-fallback) so the data is not silently lost (PRD §10.1).
        let db = setup();
        let id = db.save_new_history(HistoryItemType::Prompts, "{\"v\":1}").unwrap();
        db.clear_history(HistoryItemType::Prompts).unwrap();
        assert_eq!(db.update_history(id, "{\"v\":2}").unwrap(), 0);

        let new_id = db.save_new_history(HistoryItemType::Prompts, "{\"v\":2}").unwrap();
        let rows = db.get_history_for_type(HistoryItemType::Prompts);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, new_id);
        assert_eq!(rows[0].data_json, "{\"v\":2}");
    }
}

#[cfg(test)]
mod gloss_word_selection_tests {
    use super::AppdataDbHandle;
    use crate::db::{DatabaseHandle, APPDATA_MIGRATIONS};
    use crate::helpers::{gloss_cache_word_key, gloss_context_hash, gloss_phrase_occurs, normalize_gloss_context};
    use diesel_migrations::MigrationHarness;

    // Same throwaway temp-DB pattern as history_tests: never touch the real
    // appdata DB (clear here would be destructive against it).
    fn setup() -> AppdataDbHandle {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "simsapa_gloss_cache_test_{}_{}.sqlite3",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let url = path.to_string_lossy().to_string();
        let handle = DatabaseHandle::new(&url).expect("create temp appdata handle");
        let mut conn = handle.get_conn().expect("get temp appdata conn");
        conn.run_pending_migrations(APPDATA_MIGRATIONS)
            .expect("run appdata migrations on temp db");
        handle
    }

    #[test]
    fn upsert_and_get_round_trip() {
        let db = setup();
        assert!(db.upsert_gloss_word_cache("ārāme", "hash1", "anāthapiṇḍikassa ārāme", "ārāma-4/dpd", "ai").unwrap());
        let row = db.get_gloss_word_cache("ārāme", "hash1").expect("row exists");
        assert_eq!(row.selected_uid, "ārāma-4/dpd");
        assert_eq!(row.origin, "ai");
        // Different context hash for the same word is a miss.
        assert!(db.get_gloss_word_cache("ārāme", "hash2").is_none());
    }

    #[test]
    fn ai_never_downgrades_user_or_built_in() {
        let db = setup();
        db.upsert_gloss_word_cache("w1", "h1", "ctx", "uid-user/dpd", "user").unwrap();
        db.upsert_gloss_word_cache("w2", "h2", "ctx", "uid-builtin/dpd", "built-in").unwrap();

        assert!(!db.upsert_gloss_word_cache("w1", "h1", "ctx", "uid-ai/dpd", "ai").unwrap());
        assert!(!db.upsert_gloss_word_cache("w2", "h2", "ctx", "uid-ai/dpd", "ai").unwrap());

        assert_eq!(db.get_gloss_word_cache("w1", "h1").unwrap().selected_uid, "uid-user/dpd");
        assert_eq!(db.get_gloss_word_cache("w2", "h2").unwrap().selected_uid, "uid-builtin/dpd");
    }

    #[test]
    fn ai_refreshes_ai_and_user_overwrites_anything() {
        let db = setup();
        db.upsert_gloss_word_cache("w1", "h1", "ctx", "uid-a/dpd", "ai").unwrap();
        // A fresh AI response updates an existing ai row.
        assert!(db.upsert_gloss_word_cache("w1", "h1", "ctx", "uid-b/dpd", "ai").unwrap());
        assert_eq!(db.get_gloss_word_cache("w1", "h1").unwrap().selected_uid, "uid-b/dpd");

        // user overwrites ai...
        assert!(db.upsert_gloss_word_cache("w1", "h1", "ctx", "uid-c/dpd", "user").unwrap());
        let row = db.get_gloss_word_cache("w1", "h1").unwrap();
        assert_eq!(row.selected_uid, "uid-c/dpd");
        assert_eq!(row.origin, "user");

        // ...and built-in.
        db.upsert_gloss_word_cache("w2", "h2", "ctx", "uid-bi/dpd", "built-in").unwrap();
        assert!(db.upsert_gloss_word_cache("w2", "h2", "ctx", "uid-u/dpd", "user").unwrap());
        assert_eq!(db.get_gloss_word_cache("w2", "h2").unwrap().origin, "user");

        // built-in never overwrites user.
        assert!(!db.upsert_gloss_word_cache("w1", "h1", "ctx", "uid-d/dpd", "built-in").unwrap());
        assert_eq!(db.get_gloss_word_cache("w1", "h1").unwrap().selected_uid, "uid-c/dpd");
    }

    #[test]
    fn count_and_clear_exclude_built_in() {
        let db = setup();
        db.upsert_gloss_word_cache("w1", "h1", "ctx", "u1/dpd", "ai").unwrap();
        db.upsert_gloss_word_cache("w2", "h2", "ctx", "u2/dpd", "user").unwrap();
        db.upsert_gloss_word_cache("w3", "h3", "ctx", "u3/dpd", "built-in").unwrap();

        assert_eq!(db.count_gloss_word_cache(), 2);

        db.clear_gloss_word_cache().unwrap();
        assert_eq!(db.count_gloss_word_cache(), 0);
        // built-in row survives the bulk clear.
        assert!(db.get_gloss_word_cache("w3", "h3").is_some());
        assert!(db.get_gloss_word_cache("w1", "h1").is_none());
        assert!(db.get_gloss_word_cache("w2", "h2").is_none());
    }

    #[test]
    fn delete_removes_single_row() {
        let db = setup();
        db.upsert_gloss_word_cache("w1", "h1", "ctx", "u1/dpd", "user").unwrap();
        db.upsert_gloss_word_cache("w1", "h2", "ctx", "u1/dpd", "user").unwrap();
        db.delete_gloss_word_cache("w1", "h1").unwrap();
        assert!(db.get_gloss_word_cache("w1", "h1").is_none());
        assert!(db.get_gloss_word_cache("w1", "h2").is_some());
    }

    #[test]
    fn batch_fetch_returns_only_requested_pairs() {
        let db = setup();
        db.upsert_gloss_word_cache("w1", "h1", "ctx", "u1/dpd", "ai").unwrap();
        db.upsert_gloss_word_cache("w1", "h2", "ctx", "u2/dpd", "ai").unwrap();
        db.upsert_gloss_word_cache("w2", "h3", "ctx", "u3/dpd", "ai").unwrap();
        // w2+h2 exists only as a cross-product combination, not as a row pair
        // we request — and w1+h2 is a real row we do not request.
        let rows = db.get_gloss_word_cache_batch(&[
            ("w1".to_string(), "h1".to_string()),
            ("w2".to_string(), "h3".to_string()),
            ("missing".to_string(), "h9".to_string()),
        ]);
        let mut got: Vec<(String, String)> = rows.iter().map(|r| (r.word.clone(), r.context_hash.clone())).collect();
        got.sort();
        assert_eq!(got, vec![
            ("w1".to_string(), "h1".to_string()),
            ("w2".to_string(), "h3".to_string()),
        ]);
    }

    #[test]
    fn seed_phrases_is_idempotent_and_lookup_matches() {
        let db = setup();
        let first = db.seed_gloss_phrase_selections().unwrap();
        assert_eq!(first, 2, "both curated phrases are inserted");
        // Re-run: nothing changes.
        let second = db.seed_gloss_phrase_selections().unwrap();
        assert_eq!(second, 0);

        // Lookup by the word key.
        let rows = db.get_gloss_phrase_selections("ārāme");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].selected_uid, "ārāma-4/dpd");
        assert_eq!(rows[0].phrase, "anāthapiṇḍikassa ārāme");

        let rows = db.get_gloss_phrase_selections("bhikkhū");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].selected_uid, "bhikkhu/dpd");

        assert_eq!(db.get_all_gloss_phrase_selections().len(), 2);
    }

    #[test]
    fn phrase_occurs_in_normalized_window() {
        let db = setup();
        db.seed_gloss_phrase_selections().unwrap();

        // PRD test case 1: the seeded phrase matches inside the normalized
        // context window of `ārāme`.
        let window = "Ekaṁ samayaṁ bhagavā sāvatthiyaṁ viharati jetavane anāthapiṇḍikassa <b>ārāme</b>.";
        let norm_window = normalize_gloss_context(window);
        let rows = db.get_gloss_phrase_selections(&gloss_cache_word_key("ārāme"));
        assert!(gloss_phrase_occurs(&rows[0].phrase, &norm_window));

        // PRD test case 2: `bhikkhū` via *manobhāvanīyā bhikkhū*.
        let window2 = "Paṭisallīnā manobhāvanīyā <b>bhikkhū</b>.";
        let norm_window2 = normalize_gloss_context(window2);
        let rows2 = db.get_gloss_phrase_selections(&gloss_cache_word_key("bhikkhū"));
        assert!(gloss_phrase_occurs(&rows2[0].phrase, &norm_window2));

        // Non-matching window: same word, different context.
        let other = normalize_gloss_context("gacchati <b>ārāme</b> ramati.");
        assert!(!gloss_phrase_occurs(&rows[0].phrase, &other));

        // The niggahīta variant of the window still matches.
        let pts_window = normalize_gloss_context("jetavane anāthapiṇḍikassa <b>ārāme</b> viharati; taṃ suṇātha.");
        assert!(gloss_phrase_occurs(&rows[0].phrase, &pts_window));

        // Hash parity across ṁ/ṃ window variants.
        assert_eq!(
            gloss_context_hash(&normalize_gloss_context("anāthapiṇḍikassa ārāme viharati taṁ")),
            gloss_context_hash(&normalize_gloss_context("anāthapiṇḍikassa ārāme viharati taṃ")),
        );
    }
}


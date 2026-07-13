use std::sync::RwLock;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use diesel::prelude::*;
use regex::Regex;
use lazy_static::lazy_static;
use anyhow::{anyhow, Context, Result};

use crate::db::{appdata_models::*, DbManager};
use crate::db::appdata_schema::suttas::dsl::*;

use crate::logger::{warn, error, info, debug};
use crate::types::SuttaQuote;
use crate::app_settings::{AppSettings, ModelEntry, ModelOrigin, Provider, RepeatPali, SuttaDisplayDefaults, SuttaLayout};
use crate::sutta_display::{SuttaDisplayOptions, SuttaDisplayOverrides};
use crate::global_hotkeys::GlobalHotkeysConfig;
use crate::helpers::{bilara_text_to_segments, bilara_multi_column_html, multi_column_html_blocks, ColumnSource, bilara_content_json_to_html, thebuddhaswords_net_convert_links_in_html, word_uid_sanitize, normalize_human_word_uid};
use crate::db::dictionaries_models::DictWord;
use crate::html_content::{blank_html_page, sutta_html_page};
use crate::{get_app_globals, init_app_globals};

static DICTIONARY_JS: &str = include_str!("../../assets/js/dictionary.js");
static DICTIONARY_CSS: &str = include_str!("../../assets/css/dictionary.css");
static SIMSAPA_JS: &str = include_str!("../../assets/js/simsapa.min.js");

/// Which table a resolved word came from. DPPN entries are a `DictWord` whose
/// renderer branches on `dict_label`, not a separate kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedWordKind {
    BoldDefinition,
    DpdHeadword,
    DpdRoot,
    DictWord,
}

/// The outcome of `AppData::resolve_word_uid`: one word reached from any of the
/// tolerated uid forms, carrying both views of that word (Finding 1 /
/// `project-dpd-records-correlate-dict-words`). The `json_value` is the
/// **structured** row the JSON route returns; `dict_word` is the correlated
/// **rendered-HTML** `dict_words` row the HTML route renders (absent for a bold
/// definition, which renders via its own path). See
/// `docs/simsapa-localhost-api-search-endpoints.md`.
#[derive(Debug, Clone)]
pub struct ResolvedWord {
    canonical_uid: String,
    kind: ResolvedWordKind,
    json_value: serde_json::Value,
    dict_word: Option<DictWord>,
}

impl ResolvedWord {
    /// The stored uid this input resolved to.
    pub fn canonical_uid(&self) -> &str {
        &self.canonical_uid
    }

    pub fn kind(&self) -> ResolvedWordKind {
        self.kind
    }

    /// The structured row for the JSON route (back-compat shape preserved).
    pub fn as_json(&self) -> &serde_json::Value {
        &self.json_value
    }

    /// The correlated `dict_words` row for the HTML route, if present.
    pub fn html_dict_word(&self) -> Option<&DictWord> {
        self.dict_word.as_ref()
    }
}

/// Column header / dropdown label for a sutta text: "Pāli" for the Pāli
/// source, otherwise the translator/author uid (falling back to the language).
pub fn sutta_column_label(sutta: &Sutta) -> String {
    if sutta.language == "pli" {
        "Pāli".to_string()
    } else {
        sutta.source_uid.clone().unwrap_or_else(|| sutta.language.clone())
    }
}

/// The resolved column list as a JSON array of `{uid, label, author,
/// is_pali}` — the shape of `SUTTA_DISPLAY.columns`. Used for the page-init
/// `SUTTA_DISPLAY` injection and for the `X-SSP-Columns` response header on
/// `GET /sutta_content_block`, through which the client adopts the
/// server-resolved column state (e.g. after the Lines-mode non-segmented
/// drop). See docs/sutta-display-settings-and-multi-column-view.md.
pub fn display_columns_json(column_suttas: &[Sutta]) -> serde_json::Value {
    // Color maps in the persisted defaults are keyed by author; "pali"
    // is the key for the Pāli column (see SuttaDisplayDefaults).
    let columns: Vec<serde_json::Value> = column_suttas.iter()
        .map(|s| {
            let author = if s.language == "pli" {
                "pali".to_string()
            } else {
                s.source_uid.clone().unwrap_or_else(|| s.language.clone())
            };
            serde_json::json!({
                "uid": s.uid,
                "label": sutta_column_label(s),
                "author": author,
                "is_pali": s.language == "pli",
            })
        })
        .collect();
    serde_json::Value::Array(columns)
}

/// Represents the application data and settings
#[derive(Debug)]
pub struct AppData {
    pub dbm: DbManager,
    /// Guard-scoping rule: take a read/write guard only to copy values out
    /// (or apply a mutation) and drop it before calling any function that may
    /// lock this cache again. std::sync::RwLock read-read re-entry on one
    /// thread can deadlock against a queued writer (writer-preferring
    /// implementations, e.g. macOS pthreads), and writers are frequent at
    /// runtime (save_sutta_display_defaults is POSTed on every in-page
    /// settings change).
    pub app_settings_cache: RwLock<AppSettings>,
    pub api_url: String,
}

impl AppData {
    pub fn new() -> Self {
        init_app_globals();
        let dbm = DbManager::new().expect("Can't create DbManager");
        let app_settings_cache = RwLock::new(dbm.appdata.get_app_settings().clone());
        let g = get_app_globals();

        AppData {
            dbm,
            app_settings_cache,
            api_url: g.api_url.clone(),
        }
    }

    /// Recompute the cached shipped + commentary-definitions source-uid lists
    /// from the dictionaries / dpd DBs and persist them. Called at startup
    /// (when the cache is empty) and after user-dict import / delete / rename.
    pub fn refresh_dict_source_uid_caches(&self) {
        let shipped: Vec<String> = match self.dbm.dictionaries.list_shipped_source_uids() {
            Ok(set) => set.into_iter().collect(),
            Err(e) => {
                error(&format!("refresh_dict_source_uid_caches shipped: {:#}", e));
                return;
            }
        };
        let commentary: Vec<String> = match self.dbm.dpd.list_distinct_bold_def_ref_codes() {
            Ok(set) => set.into_iter().collect(),
            Err(e) => {
                error(&format!("refresh_dict_source_uid_caches commentary: {:#}", e));
                return;
            }
        };
        let snapshot = {
            let mut s = self.app_settings_cache.write().expect("Failed to write app settings");
            s.cached_shipped_source_uids = shipped;
            s.cached_commentary_definitions_source_uids = commentary;
            s.clone()
        };
        self.persist_app_settings(&snapshot);
    }

    /// Cached non-user-imported `dict_label` set (see PRD §8a). Reads from
    /// the in-memory `AppSettings`; populated at init.
    pub fn get_cached_shipped_source_uids(&self) -> Vec<String> {
        self.app_settings_cache.read().expect("Failed to read app settings")
            .cached_shipped_source_uids.clone()
    }

    /// Cached DPD bold-definition `ref_code` set used as commentary-definition
    /// source uids. Reads from the in-memory `AppSettings`; populated at init.
    pub fn get_cached_commentary_definitions_source_uids(&self) -> Vec<String> {
        self.app_settings_cache.read().expect("Failed to read app settings")
            .cached_commentary_definitions_source_uids.clone()
    }

    pub fn get_cached_sutta_languages(&self) -> Vec<String> {
        self.app_settings_cache.read().expect("Failed to read app settings")
            .cached_sutta_languages.clone()
    }

    pub fn get_cached_dict_languages(&self) -> Vec<String> {
        self.app_settings_cache.read().expect("Failed to read app settings")
            .cached_dict_languages.clone()
    }

    pub fn get_cached_library_languages(&self) -> Vec<String> {
        self.app_settings_cache.read().expect("Failed to read app settings")
            .cached_library_languages.clone()
    }

    /// Recompute the per-area distinct-language caches (sutta / dict / library)
    /// from the source-of-truth queries and persist them. Called at startup
    /// when caches are empty, and after sutta language download/removal and
    /// user-dict import/delete/rename.
    pub fn refresh_language_caches(&self) {
        let sutta_langs = self.dbm.appdata.get_sutta_languages();
        let dict_langs = self.dbm.dictionaries.get_distinct_languages();
        let library_langs = match crate::search::indexer::get_library_languages(&self.dbm.appdata) {
            Ok(v) => {
                let mut v = v;
                v.sort();
                v
            }
            Err(e) => {
                error(&format!("refresh_language_caches library: {}", e));
                return;
            }
        };
        let snapshot = {
            let mut s = self.app_settings_cache.write().expect("Failed to write app settings");
            s.cached_sutta_languages = sutta_langs;
            s.cached_dict_languages = dict_langs;
            s.cached_library_languages = library_langs;
            s.clone()
        };
        self.persist_app_settings(&snapshot);
    }

    /// Reset the `app_settings` row in `appdata` to `AppSettings::default()` and
    /// update the in-memory cache so subsequent reads return defaults without a restart.
    pub fn reset_app_settings_to_defaults(&self) -> Result<()> {
        {
            let mut cache = self.app_settings_cache.write().expect("Failed to write app settings");
            *cache = AppSettings::default();
        }
        self.dbm.appdata.reset_app_settings_to_defaults()
            .context("Failed to reset app_settings row in appdata")?;
        info("AppData::reset_app_settings_to_defaults() complete");
        Ok(())
    }

    /// Persist new `sutta_display` defaults (from the in-page cogwheel menu's
    /// "Save as default" scope via `POST /save_sutta_display_settings`):
    /// updates the in-memory cache and writes the settings row through.
    pub fn save_sutta_display_defaults(&self, defaults: SuttaDisplayDefaults) {
        let snapshot = {
            let mut s = self.app_settings_cache.write().expect("Failed to write app settings");
            s.sutta_display = defaults;
            s.clone()
        };
        self.persist_app_settings(&snapshot);
    }

    /// Fetches the corresponding Pali sutta for a translated sutta.
    pub fn get_pali_for_translated(&self, sutta: &Sutta) -> Result<Option<Sutta>> {
        if sutta.language == "pli" {
            return Ok(None);
        }

        // Use regex to extract the base UID part (e.g., "mn1" from "mn1/en/bodhi")
        let re = Regex::new("^([^/]+)/.*").expect("Invalid regex");
        let uid_ref = re.replace(&sutta.uid, "$1").to_string();

        let db_conn = &mut self.dbm.appdata.get_conn().expect("No appdata conn");

        // Only pli/ms suttas have bilara-format content_json needed for line-by-line rendering.
        // pli/cst suttas never have content_json.
        let res = suttas
            .select(Sutta::as_select())
            .filter(uid.eq(format!("{}/pli/ms", uid_ref)))
            .first(db_conn)
            .optional() // Makes it return Result<Option<Sutta>> instead of erroring if not found
            .context("Database query failed for Pali sutta")?;

        Ok(res)
    }

    /// Converts sutta data into an IndexMap of segments, potentially including variants, comments, glosses.
    /// Returns IndexMap to preserve JSON insertion order.
    pub fn sutta_to_segments_json(
        &self,
        sutta: &Sutta,
        use_template: bool,
        show_references: bool,
    ) -> Result<IndexMap<String, String>> {
        use crate::db::appdata_schema::{sutta_variants, sutta_comments, sutta_glosses};

        let db_conn = &mut self.dbm.appdata.get_conn().expect("No appdata conn");

        let variant_record = sutta_variants::table
            .filter(sutta_variants::sutta_uid.eq(&sutta.uid))
            .select(SuttaVariant::as_select())
            .first::<SuttaVariant>(db_conn)
            .optional()
            .context("Database query failed for SuttaVariant")?;
        // Extract the content_json string if the record was found
        let variant_json_str: Option<String> = variant_record.and_then(|v| v.content_json);

        let comment_record = sutta_comments::table
            .filter(sutta_comments::sutta_uid.eq(&sutta.uid))
            .select(SuttaComment::as_select())
            .first::<SuttaComment>(db_conn)
            .optional()
            .context("Database query failed for SuttaComment")?;
        let comment_json_str: Option<String> = comment_record.and_then(|c| c.content_json);

        let gloss_record = sutta_glosses::table
            .filter(sutta_glosses::sutta_uid.eq(&sutta.uid))
            .select(SuttaGloss::as_select())
            .first::<SuttaGloss>(db_conn)
            .optional()
            .context("Database query failed for SuttaGloss")?;
        let gloss_json_str: Option<String> = gloss_record.and_then(|g| g.content_json);

        let tmpl_str = if use_template {
            sutta.content_json_tmpl.as_deref()
        } else {
            None
        };

        let content_str = sutta.content_json.as_deref()
            .ok_or_else(|| anyhow!("Sutta {} is missing content_json", sutta.uid))?;

        // Guard scoped to the value copy — see the guard-scoping rule at the
        // app_settings_cache field.
        let (show_all_variant_readings, show_glosses) = {
            let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
            (app_settings.show_all_variant_readings, app_settings.show_glosses)
        };

        bilara_text_to_segments(
            content_str,
            tmpl_str,
            variant_json_str.as_deref(),
            comment_json_str.as_deref(),
            gloss_json_str.as_deref(),
            show_all_variant_readings,
            show_glosses,
            show_references,
        )
    }

    /// Resolve the effective display options for a sutta render: persisted
    /// defaults from the settings cache, overridden by explicit request
    /// parameters, with the default column set (opened sutta + Pāli
    /// counterpart) filled in.
    pub fn resolve_sutta_display_options(
        &self,
        sutta: &Sutta,
        show_references: bool,
        overrides: &SuttaDisplayOverrides,
    ) -> SuttaDisplayOptions {
        let pali_uid = if overrides.columns.is_none() {
            match self.get_pali_for_translated(sutta) {
                Ok(pali) => pali.map(|p| p.uid),
                Err(e) => {
                    error(&format!("resolve_sutta_display_options: {:#}", e));
                    None
                }
            }
        } else {
            None
        };
        let mut options = {
            let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
            SuttaDisplayOptions::resolve(&app_settings, &sutta.uid, pali_uid.as_deref(), show_references, overrides)
        };

        // The Pāli placement (first column; repeated per repeat_pali) is
        // applied to the resolved column set. Solo keeps the columns as
        // resolved: the renderer shows only the opened sutta, but the page
        // state (SUTTA_DISPLAY.columns) retains the full set so switching
        // back to Columns/Lines restores it.
        if options.layout != SuttaLayout::Solo {
            options.columns = self.arrange_repeat_pali(sutta, &options.columns, options.repeat_pali);
        }

        // Lines mode cannot interleave non-segmented texts, so they are
        // excluded here, at options resolution — the renderer never sees them
        // (PRD FR 8). The UI disables such entries in the bottom-bar dropdowns
        // while in Lines mode; an API request that includes one simply drops
        // that column (no error), so UI and API behave identically. When the
        // opened sutta itself is non-segmented, it renders alone via the
        // standard whole-document path.
        if options.layout == SuttaLayout::LineByLine {
            let opened_is_segmented = sutta.content_json.as_deref().map(|c| !c.is_empty()).unwrap_or(false);
            if opened_is_segmented {
                options.columns.retain(|col_uid| {
                    if col_uid == &sutta.uid {
                        return true;
                    }
                    match self.dbm.appdata.get_sutta(col_uid) {
                        Some(s) => s.content_json.as_deref().map(|c| !c.is_empty()).unwrap_or(false),
                        None => true, // unknown uids are reported by the renderer, not dropped
                    }
                });
            } else {
                options.columns = vec![sutta.uid.clone()];
            }
        }

        options
    }

    /// Applies the `repeat_pali` arrangement to a resolved column uid list.
    /// The first Pāli column (by the sutta's language) is the anchor; the
    /// translations keep their order. Off: Pāli first, once. Alternate: Pāli
    /// before each translation. AtEnd: Pāli first and once more as the last
    /// column. Without a Pāli column (or with no translations) the list is
    /// returned with the Pāli-first normalization only. Unknown uids are kept
    /// as translations — the renderer reports them, not this arrangement.
    ///
    /// The client no longer mirrors this after a swap — `fetch_content_block`
    /// adopts the resolved list from the `X-SSP-Columns` header. The
    /// client-side `arrange_display_columns` (src-ts/display_settings.ts)
    /// remains only for the column bar's base-set collapse (repeat "off"),
    /// which must keep matching this function's collapse rule.
    fn arrange_repeat_pali(&self, sutta: &Sutta, columns: &[String], repeat: RepeatPali) -> Vec<String> {
        let is_pali = |col_uid: &str| -> bool {
            if col_uid == sutta.uid {
                return sutta.language == "pli";
            }
            match self.dbm.appdata.get_sutta(col_uid) {
                Some(s) => s.language == "pli",
                None => false,
            }
        };

        let mut pali: Option<String> = None;
        let mut translations: Vec<String> = Vec::new();
        for col_uid in columns {
            if is_pali(col_uid) {
                // Repeated Pāli entries (e.g. an atend arrangement echoed
                // back by the client) collapse to one anchor.
                if pali.is_none() {
                    pali = Some(col_uid.clone());
                }
            } else {
                translations.push(col_uid.clone());
            }
        }

        match pali {
            None => translations,
            Some(p) if translations.is_empty() => vec![p],
            Some(p) => match repeat {
                RepeatPali::Off => {
                    let mut cols = vec![p];
                    cols.extend(translations);
                    cols
                }
                RepeatPali::Alternate => {
                    translations.into_iter().flat_map(|t| [p.clone(), t]).collect()
                }
                RepeatPali::AtEnd => {
                    let mut cols = vec![p.clone()];
                    cols.extend(translations);
                    cols.push(p);
                    cols
                }
            },
        }
    }

    /// Renders one sutta's standard whole-document content body: segmented
    /// texts via their own template, otherwise the stored HTML / plain text.
    /// Used for the single-column view and for each column of the unaligned
    /// block-columns fallback.
    fn render_sutta_standard_body(&self, sutta: &Sutta, show_references: bool) -> Result<String> {
        let body = if let Some(ref content_json_str) = sutta.content_json {
            if !content_json_str.is_empty() {
                // Generate standard HTML view (using template within sutta_to_segments_json)
                let segments_json = self.sutta_to_segments_json(sutta, true, show_references)
                                            .context("Failed to generate segments for standard view")?;
                bilara_content_json_to_html(&segments_json)?
            } else {
                "<div class='suttacentral bilara-text'></div>".to_string()
            }

        } else if let Some(ref html) = sutta.content_html {
            if !html.is_empty() { html.clone() }
            else { "<div class='suttacentral bilara-text'></div>".to_string() }

        } else if let Some(ref plain) = sutta.content_plain {
            if !plain.is_empty() { format!("<div class='suttacentral bilara-text'><pre>{}</pre></div>", plain) }
            else { "<div class='suttacentral bilara-text'></div>".to_string() }

        } else {
            "<div class='suttacentral bilara-text'><p>No content.</p></div>".to_string()
        };
        Ok(body)
    }

    /// Resolves each column uid in `options` to its `Sutta` record. The
    /// opened sutta is reused (not re-queried); unknown uids are an error
    /// (the API maps it to HTTP 404). An empty column set falls back to the
    /// opened sutta alone.
    fn resolve_column_suttas(&self, sutta: &Sutta, options: &SuttaDisplayOptions) -> Result<Vec<Sutta>> {
        let mut column_suttas: Vec<Sutta> = Vec::new();
        for col_uid in &options.columns {
            if col_uid == &sutta.uid {
                column_suttas.push(sutta.clone());
            } else if let Some(s) = self.dbm.appdata.get_sutta(col_uid) {
                column_suttas.push(s);
            } else {
                return Err(anyhow!("Unknown column sutta uid: {}", col_uid));
            }
        }
        if column_suttas.is_empty() {
            column_suttas.push(sutta.clone());
        }
        Ok(column_suttas)
    }

    /// Renders just the sutta content block — the
    /// `<div class='suttacentral bilara-text …'>…</div>` wrapper (including
    /// the Columns-mode header row) that fills `#ssp_content` — without any
    /// page chrome. Served standalone by `GET /sutta_content_block` for
    /// in-page layout/column re-renders; `render_sutta_content` composes it
    /// into the full page.
    pub fn render_sutta_content_block(&self, sutta: &Sutta, options: &SuttaDisplayOptions) -> Result<String> {
        self.render_sutta_content_block_with_columns(sutta, options).map(|(html, _)| html)
    }

    /// `render_sutta_content_block` plus the resolved column list (as the
    /// `SUTTA_DISPLAY.columns`-shaped JSON array), for the `X-SSP-Columns`
    /// response header: the client adopts the server-resolved columns after a
    /// block swap, so the two can't diverge (Lines-mode drop, Repeat-Pāli
    /// arrangement).
    pub fn render_sutta_content_block_with_columns(
        &self,
        sutta: &Sutta,
        options: &SuttaDisplayOptions,
    ) -> Result<(String, serde_json::Value)> {
        let column_suttas = self.resolve_column_suttas(sutta, options)?;
        let html = self.render_content_block_for_columns(sutta, &column_suttas, options)?;
        Ok((html, display_columns_json(&column_suttas)))
    }

    /// The content-block branching for already-resolved column suttas:
    /// aligned multi-column when every column is segmented, unaligned block
    /// columns otherwise, standard whole-document path for a single column.
    fn render_content_block_for_columns(
        &self,
        sutta: &Sutta,
        column_suttas: &[Sutta],
        options: &SuttaDisplayOptions,
    ) -> Result<String> {
        let show_references = options.show_references;

        let all_segmented = column_suttas.iter()
            .all(|s| s.content_json.as_deref().map(|c| !c.is_empty()).unwrap_or(false));

        let content_html_body = if options.layout == SuttaLayout::Solo {
            // Solo: only the opened translation, via the standard
            // whole-document path — the resolved columns are page state for
            // switching back to Columns/Lines, not render input.
            self.render_sutta_standard_body(sutta, show_references)?

        } else if column_suttas.len() >= 2 && all_segmented {
            // Per-segment aligned multi-column view: each column's segments
            // carry that text's own variants/comments/glosses.
            let mut columns: Vec<ColumnSource> = Vec::new();
            for col in column_suttas {
                let segments = self.sutta_to_segments_json(col, false, show_references)
                    .with_context(|| format!("Failed to generate segments for column {}", col.uid))?;
                columns.push(ColumnSource {
                    uid: col.uid.clone(),
                    label: sutta_column_label(col),
                    is_pali: col.language == "pli",
                    segments,
                });
            }

            let tmpl_str = sutta.content_json_tmpl.as_deref()
                .ok_or_else(|| anyhow!("Sutta {} requires content_json_tmpl for the multi-column view", sutta.uid))?;
            let tmpl_json: IndexMap<String, String> = serde_json::from_str(tmpl_str)
                .with_context(|| format!("Failed to parse template JSON into IndexMap for the multi-column view (Sutta: {})", sutta.uid))?;

            bilara_multi_column_html(&columns, &tmpl_json, show_references, options.layout)?

        } else if column_suttas.len() >= 2 {
            // A selected text lacks content_json: unaligned block-columns
            // fallback — each text's standard whole-document rendering in one
            // flex column.
            let mut cols: Vec<(String, String, bool, String)> = Vec::new();
            for col in column_suttas {
                let html = self.render_sutta_standard_body(col, show_references)
                    .with_context(|| format!("Failed to render block column {}", col.uid))?;
                cols.push((sutta_column_label(col), col.uid.clone(), col.language == "pli", html));
            }
            multi_column_html_blocks(&cols)

        } else {
            self.render_sutta_standard_body(sutta, show_references)?
        };

        Ok(content_html_body)
    }

    /// Builds the `SUTTA_DISPLAY` JS object injected into the page next to
    /// `SUTTA_UID`/`WINDOW_ID`: the effective render parameters (resolved
    /// layout, ordered column uids + labels, `show_references`) that the
    /// in-page display-settings menu and column bar read as their initial
    /// state and reproduce in content-block fetches.
    fn sutta_display_js(&self, column_suttas: &[Sutta], options: &SuttaDisplayOptions) -> String {
        let columns = display_columns_json(column_suttas);
        let defaults = {
            let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
            serde_json::to_value(&app_settings.sutta_display).unwrap_or(serde_json::Value::Null)
        };
        let obj = serde_json::json!({
            "layout": options.layout.as_str(),
            "repeat_pali": options.repeat_pali.as_str(),
            "columns": columns,
            "show_references": options.show_references,
            "defaults": defaults,
        });
        // Escape "</" so a value can never terminate the surrounding
        // <script> block ("<\/" is "/" in a JS string literal).
        let json = obj.to_string().replace("</", "<\\/");
        format!(" const SUTTA_DISPLAY = {}; window.SUTTA_DISPLAY = SUTTA_DISPLAY;", json)
    }

    /// Renders the complete HTML page for a sutta.
    ///
    /// See also: simsapa/simsapa/app/export_helpers.py::render_sutta_content()
    ///
    /// Layout and column choices come from `options` (resolved once at the
    /// call boundary via `resolve_sutta_display_options`, not read from the
    /// settings cache here). `options.show_references` controls whether
    /// segment reference anchors (e.g., 37.5) are rendered in the HTML;
    /// these are needed when navigating to a specific anchor in the sutta.
    pub fn render_sutta_content(
        &self,
        sutta: &Sutta,
        sutta_quote: Option<&SuttaQuote>,
        js_extra_pre: Option<String>,
        options: &SuttaDisplayOptions,
    ) -> Result<String> {
        // Copy the needed settings values out and drop the guard before any
        // call that may re-lock the cache (resolve_column_suttas →
        // sutta_to_segments_json, sutta_display_js, get_theme_name) — see the
        // guard-scoping rule at the app_settings_cache field.
        let (font_size, max_width, show_bookmarks) = {
            let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
            (app_settings.sutta_font_size, app_settings.sutta_max_width, app_settings.show_bookmarks)
        };

        let column_suttas = self.resolve_column_suttas(sutta, options)?;
        let content_html_body = self.render_content_block_for_columns(sutta, &column_suttas, options)?;

        // Format CSS and JS extras. The reading measure is passed as CSS vars,
        // not a direct body max-width: body { max-width } reads
        // --single-max-width (suttas.sass), while the side-by-side layout
        // widens the body to the viewport and instead caps each column at
        // --col-max-width (see _suttacentral.sass).
        let css_extra = format!(
            "html {{ font-size: {}px; }} body {{ --single-max-width: {}ex; --col-max-width: {}ex; }}",
            font_size, max_width, max_width,
        );

        // window.SUTTA_UID: a top-level `const` is not a globalThis property,
        // and the webpack bundle (content_reload.ts) reads it via globalThis —
        // same pattern as WINDOW_ID.
        let mut js_extra = format!("const SUTTA_UID = '{}'; window.SUTTA_UID = SUTTA_UID;", sutta.uid);

        if let Some(js_pre) = js_extra_pre {
            js_extra = format!("{}; {}", js_pre, js_extra);
        }

        js_extra.push_str(&format!(" const SHOW_BOOKMARKS = {};", show_bookmarks));
        js_extra.push_str(&self.sutta_display_js(&column_suttas, options));

        if let Some(quote) = sutta_quote {
            // Escape the quote text for JavaScript string literal
            let escaped_text = quote.quote.replace('\\', "\\\\").replace('"', "\\\"");
            js_extra.push_str(&format!(r#" document.addEventListener("DOMContentLoaded", function(event) {{ highlight_and_scroll_to("{}"); }}); const SHOW_QUOTE = "{}";"#, escaped_text, escaped_text));
        }

        // Build body_class with theme and language
        let mut body_class = self.get_theme_name();

        // Add language-specific class (e.g., lang-he for Hebrew, lang-th for Thai)
        if !sutta.language.is_empty() {
            body_class.push_str(&format!(" lang-{}", sutta.language));
        }

        // Only add navigation if sutta has range information
        let nav_html = if sutta.sutta_range_group.is_some() &&
                          sutta.sutta_range_start.is_some() &&
                          sutta.sutta_range_end.is_some() {
            debug(&format!("Sutta {} has range info: group={:?}, start={:?}, end={:?}",
                sutta.uid, sutta.sutta_range_group, sutta.sutta_range_start, sutta.sutta_range_end));

            // Query prev/next suttas to determine navigation button state
            let prev_sutta = self.dbm.appdata.get_prev_sutta(&sutta.uid).ok().flatten();
            let next_sutta = self.dbm.appdata.get_next_sutta(&sutta.uid).ok().flatten();

            let is_first_sutta = prev_sutta.is_none();
            let is_last_sutta = next_sutta.is_none();

            debug(&format!("Sutta {} navigation: has_prev={}, has_next={}",
                sutta.uid, !is_first_sutta, !is_last_sutta));

            // Build the navigation HTML by replacing placeholders
            use crate::html_content::PREV_NEXT_CHAPTER_HTML;
            PREV_NEXT_CHAPTER_HTML
                .replace("{current_spine_item_uid}", &sutta.uid)
                .replace("{current_book_uid}", &sutta.uid)  // Use sutta.uid for both since suttas don't have book_uid
                .replace("{is_first_chapter}", &is_first_sutta.to_string())
                .replace("{is_last_chapter}", &is_last_sutta.to_string())
                .replace("{api_url}", &self.api_url)
        } else {
            warn(&format!("Sutta {} missing range info - no navigation buttons", sutta.uid));
            // No range information, don't show navigation buttons
            String::new()
        };

        // Wrap content in the full HTML page structure
        use crate::html_content::sutta_html_page_with_nav;
        let final_html = sutta_html_page_with_nav(
            &content_html_body,
            Some(self.api_url.to_string()),
            Some(css_extra.to_string()),
            Some(js_extra.to_string()),
            Some(body_class),
            Some(nav_html),
            true,
        );

        Ok(final_html)
    }

    /// Renders a sutta by UID as a complete HTML page with window context.
    ///
    /// This is a convenience method that encapsulates the common pattern of:
    /// 1. Getting the theme/body_class from app settings
    /// 2. Returning a blank page for empty UIDs or missing suttas
    /// 3. Rendering the sutta content with WINDOW_ID JavaScript context
    /// 4. Handling rendering errors gracefully with an error page
    ///
    /// Used by both QML bridge (sutta_bridge.rs::get_sutta_html) and the API
    /// endpoint (api.rs::get_sutta_html_by_uid, via the shared
    /// `sutta_html_response` helper) to ensure consistent behavior.
    ///
    /// The `show_references` parameter controls whether segment reference anchors are rendered.
    /// This should be true when the sutta was requested with an anchor ID to scroll to.
    pub fn render_sutta_html_by_uid(&self, window_id: &str, sutta_uid: &str, show_references: bool) -> String {
        self.render_sutta_html_by_uid_with_overrides(window_id, sutta_uid, show_references, &SuttaDisplayOverrides::default())
    }

    /// `render_sutta_html_by_uid` with explicit display overrides (from the
    /// API routes' optional `layout` / `columns` GET parameters); absent
    /// overrides fall back to the persisted defaults. Infallible: maps a
    /// render error to a generic "Rendering error" page (the QML bridge
    /// contract). The API routes use the `try_` variant instead so they can
    /// map render errors (e.g. unknown column uid) to HTTP statuses.
    pub fn render_sutta_html_by_uid_with_overrides(
        &self,
        window_id: &str,
        sutta_uid: &str,
        show_references: bool,
        overrides: &SuttaDisplayOverrides,
    ) -> String {
        self.try_render_sutta_html_by_uid_with_overrides(window_id, sutta_uid, show_references, overrides)
            .unwrap_or_else(|_| {
                let body_class = {
                    let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
                    app_settings.theme_name_as_string()
                };
                sutta_html_page("Rendering error", None, None, None, Some(body_class))
            })
    }

    /// Fallible variant of `render_sutta_html_by_uid_with_overrides`: an
    /// empty or unknown sutta uid is still a non-error blank page (`Ok`),
    /// but a render failure (e.g. "Unknown column sutta uid" from a bad
    /// `columns` override) is returned as `Err` so the API routes can map
    /// it to an HTTP status with the message (parity with
    /// `/sutta_content_block`).
    pub fn try_render_sutta_html_by_uid_with_overrides(
        &self,
        window_id: &str,
        sutta_uid: &str,
        show_references: bool,
        overrides: &SuttaDisplayOverrides,
    ) -> Result<String> {
        // Guard scoped to the value copy: resolve_sutta_display_options and
        // render_sutta_content below re-lock the settings cache.
        let body_class = {
            let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
            app_settings.theme_name_as_string()
        };

        let blank_page_html = blank_html_page(Some(body_class));

        // Return blank page for empty UID
        if sutta_uid.is_empty() {
            return Ok(blank_page_html);
        }

        // Try to get the sutta from database
        let sutta = self.dbm.appdata.get_sutta(sutta_uid);

        match sutta {
            Some(sutta) => {
                // Render the sutta with WINDOW_ID in the JavaScript
                let js_extra = format!("const WINDOW_ID = '{}'; window.WINDOW_ID = WINDOW_ID;", window_id);
                let options = self.resolve_sutta_display_options(&sutta, show_references, overrides);
                self.render_sutta_content(&sutta, None, Some(js_extra), &options)
            },
            None => Ok(blank_page_html),
        }
    }

    /// Renders a dictionary word by UID as a complete HTML page with window context.
    ///
    /// This is a convenience method that encapsulates the common pattern of:
    /// 1. Getting the theme/body_class from app settings
    /// 2. Returning a blank page for empty UIDs or missing words
    /// 3. Rendering the word HTML with JavaScript context (API_URL, WINDOW_ID, IS_MOBILE)
    /// 4. Injecting dictionary-specific CSS and JavaScript
    /// 5. Modifying HTML tags to include theme classes
    /// 6. Updating resource links to point to API endpoints
    ///
    /// Used by both QML bridge (sutta_bridge.rs::get_word_html) and the API
    /// endpoint (api.rs::get_word_html_by_uid). Both the uid forms accepted and
    /// the dict_words row reached are determined by the shared
    /// `AppData::resolve_word_uid` resolver (which this delegates to), so the
    /// HTML and JSON (api.rs::get_word_json) word routes stay in lockstep.
    pub fn render_word_html_by_uid(&self, window_id: &str, word_uid: &str) -> String {
        // Guard scoped to the value copy — see the guard-scoping rule at the
        // app_settings_cache field.
        let body_class = {
            let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
            app_settings.theme_name_as_string()
        };

        let blank_page_html = blank_html_page(Some(body_class.clone()));

        // Return blank page for empty UID
        if word_uid.is_empty() {
            return blank_page_html;
        }

        // Resolve any tolerated uid form to one word, then dispatch by kind to
        // the matching renderer. Bold definitions use their dedicated renderer;
        // every other kind renders its correlated dict_words HTML row (DPD
        // headword/root forms reach that row via lemma_1 / root sanitize). See
        // resolve_word_uid and docs/simsapa-localhost-api-search-endpoints.md.
        let resolved = match self.resolve_word_uid(word_uid) {
            Some(r) => r,
            None => return blank_page_html,
        };

        match resolved.kind() {
            ResolvedWordKind::BoldDefinition => {
                match self.dbm.dpd.get_bold_definition_by_uid(resolved.canonical_uid()) {
                    Some(bd) => crate::html_content::render_bold_definition(&bd, window_id, Some(body_class.clone())),
                    None => blank_page_html,
                }
            }
            _ => match resolved.html_dict_word() {
                Some(word) => self.render_dict_word_html(word, window_id, &body_class),
                None => blank_page_html,
            },
        }
    }

    /// Render a resolved `dict_words` row to a full HTML page. Extracted from
    /// `render_word_html_by_uid` so the resolver can dispatch to it; the body is
    /// unchanged from the previous inline rendering.
    fn render_dict_word_html(&self, word: &DictWord, window_id: &str, body_class: &str) -> String {
        use regex::{Regex, Captures};

        lazy_static! {
            // (<link href=")(main.js)(") class="load_js" rel="preload" as="script">
            static ref RE_LINK_HREF: Regex = Regex::new(r#"(<link +[^>]*href=['"])([^'"]+)(['"])"#).unwrap();
            // Match <html> tag with optional attributes
            static ref RE_HTML_TAG: Regex = Regex::new(r#"<html[^>]*>"#).unwrap();
            // Match <body> tag with optional attributes
            static ref RE_BODY_TAG: Regex = Regex::new(r#"<body[^>]*>"#).unwrap();
            // Full <link …> tag (for neutralising/rewriting user-dict res links).
            static ref RE_LINK_FULL: Regex = Regex::new(r#"<link\b[^>]*>"#).unwrap();
            // Full <script … src=…></script> tag (for neutralising user-dict res JS).
            static ref RE_SCRIPT_SRC_FULL: Regex = Regex::new(r#"(?is)<script\b[^>]*\bsrc=['"][^'"]+['"][^>]*>.*?</script>"#).unwrap();
            // Extract the href/src attribute value from a tag.
            static ref RE_HREF_ATTR: Regex = Regex::new(r#"href=['"]([^'"]+)['"]"#).unwrap();
            static ref RE_SRC_ATTR: Regex = Regex::new(r#"src=['"]([^'"]+)['"]"#).unwrap();
        }

        match word.definition_html {
                Some(ref definition_html) => {
                    // DPPN entries are stored as fragments wrapped in
                    // `<div class="dppn">…</div>` (see bootstrap/dppn.rs). They
                    // do not have `<html>/<head>/<body>` for the regex rewrite
                    // path below to operate on, so route them through the
                    // dedicated full-page wrapper instead.
                    if word.dict_label == "dppn" {
                        return crate::html_content::render_dppn_entry(
                            word,
                            window_id,
                            Some(body_class.to_string()),
                        );
                    }
                    // DPD ships its resources as files under assets/dpd-res/ and
                    // references them with bare <link href="…"> tags that get
                    // rewritten to that folder below. User-imported dictionaries
                    // instead carry their res/ files in the dict_resources table:
                    // CSS/JS are injected inline here, images/fonts are served via
                    // the /dict_resources/<id>/… API route.
                    let is_dpd = word.dict_label == "dpd";

                    let dict_resources: Vec<crate::db::dictionaries_models::DictResource> = if is_dpd {
                        Vec::new()
                    } else {
                        self.dbm.dictionaries.list_dict_resources(word.dictionary_id).unwrap_or_default()
                    };

                    // Split resources into inline-injected CSS/JS and servable
                    // (images/fonts/etc.). `resource_path` is relative to res/.
                    use std::collections::HashSet;
                    let mut dict_css = String::new();
                    let mut dict_js = String::new();
                    let mut css_paths: HashSet<String> = HashSet::new();
                    let mut js_paths: HashSet<String> = HashSet::new();
                    let mut servable_paths: HashSet<String> = HashSet::new();
                    for r in &dict_resources {
                        match r.mime_type.as_deref().unwrap_or("") {
                            "text/css" => {
                                if let Some(ref data) = r.content_data {
                                    dict_css.push_str(&String::from_utf8_lossy(data));
                                    dict_css.push('\n');
                                }
                                css_paths.insert(r.resource_path.clone());
                            }
                            "application/javascript" | "text/javascript" => {
                                if let Some(ref data) = r.content_data {
                                    dict_js.push_str(&String::from_utf8_lossy(data));
                                    dict_js.push('\n');
                                }
                                js_paths.insert(r.resource_path.clone());
                            }
                            _ => {
                                servable_paths.insert(r.resource_path.clone());
                            }
                        }
                    }

                    let mut js_extra = "".to_string();
                    js_extra.push_str(&format!(" const API_URL = '{}'; window.API_URL = API_URL;", &self.api_url));
                    js_extra.push_str(&format!(" const WINDOW_ID = '{}'; window.WINDOW_ID = WINDOW_ID;", window_id));
                    js_extra.push_str(&format!(" const IS_MOBILE = {};", crate::is_mobile()));
                    js_extra.push_str(DICTIONARY_JS);
                    js_extra.push_str(SIMSAPA_JS);

                    let mut word_html = definition_html.clone();

                    // Normalise a res reference to its dict_resources path: strip
                    // a leading "./" and an optional "res/" prefix (mw-gd refers
                    // to `mw.css` while the file lives at `res/mw.css`).
                    let normalize_res = |href: &str| -> String {
                        let h = href.trim_start_matches("./");
                        h.strip_prefix("res/").unwrap_or(h).to_string()
                    };

                    if is_dpd {
                        // DPD-only: rewrite every <link href> to the dpd-res folder.
                        word_html = RE_LINK_HREF.replace_all(&word_html, |caps: &Captures| {
                            format!("{}{}{}{}",
                                    &caps[1],
                                    &format!("{}/assets/dpd-res/", &self.api_url),
                                    &caps[2],
                                    &caps[3])
                        }).to_string();
                    } else {
                        // Neutralise <link> tags whose href is an injected CSS
                        // resource; rewrite <link> tags pointing at a servable
                        // resource to the API route; leave the rest untouched.
                        word_html = RE_LINK_FULL.replace_all(&word_html, |caps: &Captures| {
                            let tag = &caps[0];
                            if let Some(hc) = RE_HREF_ATTR.captures(tag) {
                                let norm = normalize_res(&hc[1]);
                                if css_paths.contains(&norm) {
                                    return String::new();
                                }
                                if servable_paths.contains(&norm) {
                                    let new_attr = format!(r#"href="{}/dict_resources/{}/{}""#, &self.api_url, word.dictionary_id, norm);
                                    return tag.replace(&hc[0], &new_attr);
                                }
                            }
                            tag.to_string()
                        }).to_string();

                        // Neutralise <script src=…> tags whose src is an injected
                        // JS resource (the contents are injected inline below).
                        if !js_paths.is_empty() {
                            word_html = RE_SCRIPT_SRC_FULL.replace_all(&word_html, |caps: &Captures| {
                                let tag = &caps[0];
                                if let Some(sc) = RE_SRC_ATTR.captures(tag) {
                                    if js_paths.contains(&normalize_res(&sc[1])) {
                                        return String::new();
                                    }
                                }
                                tag.to_string()
                            }).to_string();
                        }

                        // Rewrite remaining src="…" references to servable
                        // resources (images/fonts) to the id-keyed API route.
                        if !servable_paths.is_empty() {
                            word_html = RE_SRC_ATTR.replace_all(&word_html, |caps: &Captures| {
                                let norm = normalize_res(&caps[1]);
                                if servable_paths.contains(&norm) {
                                    format!(r#"src="{}/dict_resources/{}/{}""#, &self.api_url, word.dictionary_id, norm)
                                } else {
                                    caps[0].to_string()
                                }
                            }).to_string();
                        }
                    }

                    // Inject the shared dictionary CSS/JS plus any per-dictionary
                    // CSS/JS captured from res/ into <head>. Done after the res
                    // rewriting above so the injected <script> bodies are not
                    // themselves rewritten.
                    let head_style = if dict_css.is_empty() {
                        format!("<style>{}</style>", DICTIONARY_CSS)
                    } else {
                        format!("<style>{}</style><style>{}</style>", DICTIONARY_CSS, dict_css)
                    };
                    if !dict_js.is_empty() {
                        js_extra.push('\n');
                        js_extra.push_str(&dict_js);
                    }
                    word_html = word_html.replace(
                        "</head>",
                        &format!(r#"{}<script>{}</script></head>"#, head_style, js_extra));

                    // Replace <html> tag to include dark mode class
                    word_html = RE_HTML_TAG.replace(&word_html, &format!(r#"<html class="{}">"#, body_class)).to_string();

                    // Replace <body> tag to include dark mode class and word heading
                    word_html = RE_BODY_TAG.replace(&word_html, &format!(r#"
<body class="{}">
    <div class='word-heading'>
        <div class='word-title'>
            <h1>{}</h1>
        </div>
    </div>"#, body_class, word.word())).to_string();

                    // Convert thebuddhaswords.net links to ssp:// internal links
                    word_html = thebuddhaswords_net_convert_links_in_html(&word_html);

                    word_html
                },
                None => blank_html_page(Some(body_class.to_string())),
        }
    }

    /// Renders a book spine item by UID as a complete HTML page with window context.
    ///
    /// This is a convenience method that encapsulates the common pattern of:
    /// 1. Getting the theme/body_class from app settings
    /// 2. Returning a blank page for empty UIDs or missing spine items
    /// 3. Rendering the spine item HTML with WINDOW_ID JavaScript context
    /// 4. Handling rendering errors gracefully with an error page
    ///
    /// Used by both QML bridge (sutta_bridge.rs::get_book_spine_html) and
    /// API endpoint (api.rs::get_book_spine_item_html_by_uid) to ensure consistent behavior.
    pub fn render_book_spine_html_by_uid(&self, window_id: &str, spine_item_uid: &str) -> String {
        // Guard scoped to the value copy: render_book_spine_item_html below
        // re-locks the settings cache.
        let body_class = {
            let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
            app_settings.theme_name_as_string()
        };

        let blank_page_html = blank_html_page(Some(body_class.clone()));

        // Return blank page for empty UID
        if spine_item_uid.is_empty() {
            return blank_page_html;
        }

        // Try to get the spine item from database
        let spine_item = match self.dbm.appdata.get_book_spine_item(spine_item_uid) {
            Ok(Some(item)) => item,
            Ok(None) => {
                info(&format!("Book spine item not found: {}", spine_item_uid));
                return blank_page_html;
            }
            Err(e) => {
                error(&format!("Failed to get spine item {}: {}", spine_item_uid, e));
                return blank_page_html;
            }
        };

        // Render the spine item (WINDOW_ID is added by render_book_spine_item_html)
        self.render_book_spine_item_html(&spine_item, Some(window_id.to_string()), None)
            .unwrap_or_else(|_| sutta_html_page("Rendering error", None, None, None, Some(body_class)))
    }

    /// Render a book spine item as complete HTML page
    ///
    /// Similar to render_sutta_content, but for book spine items (only for Epub chapters and HTML, PDFs are shown with .url instead of .loadHtml()).
    pub fn render_book_spine_item_html(
        &self,
        spine_item: &BookSpineItem,
        window_id: Option<String>,
        js_extra_pre: Option<String>,
    ) -> Result<String> {
        // Copy the needed settings values out and drop the guard before
        // get_theme_name() below re-locks the cache — see the guard-scoping
        // rule at the app_settings_cache field.
        let (font_size, max_width, show_bookmarks) = {
            let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
            (app_settings.sutta_font_size, app_settings.sutta_max_width, app_settings.show_bookmarks)
        };

        // Get book information to check enable_embedded_css flag
        let book_enable_embedded_css = if let Ok(Some(book)) = self.dbm.appdata.get_book_by_uid(&spine_item.book_uid) {
            book.enable_embedded_css
        } else {
            true // Default to true if book not found
        };

        // Use content_html if available, otherwise fall back to plain text
        let mut content_html_body = if let Some(ref html) = spine_item.content_html {
            if !html.is_empty() {
                html.clone()
            } else {
                "<div class='book-content'><p>No content.</p></div>".to_string()
            }
        } else if let Some(ref plain) = spine_item.content_plain {
            if !plain.is_empty() {
                format!("<div class='book-content'><pre>{}</pre></div>", plain)
            } else {
                "<div class='book-content'><p>No content.</p></div>".to_string()
            }
        } else {
            "<div class='book-content'><p>No content.</p></div>".to_string()
        };

        // Remove embedded CSS if enable_embedded_css is false
        if !book_enable_embedded_css {
            use regex::Regex;
            lazy_static! {
                static ref CSS_LINK_RE: Regex = Regex::new(r"<link[^>]*>").unwrap();
                static ref CSS_STYLE_RE: Regex = Regex::new(r"<style[^>]*>.*?</style>").unwrap();
            }

            content_html_body = CSS_LINK_RE.replace_all(&content_html_body, "").into_owned();
            content_html_body = CSS_STYLE_RE.replace_all(&content_html_body, "").into_owned();
        }

        // Format CSS and JS extras
        // --single-max-width: body { max-width } in suttas.sass reads this var
        // (an inline/direct max-width would fight the layout rules there).
        let css_extra = format!("html {{ font-size: {}px; }} body {{ --single-max-width: {}ex; }}", font_size, max_width);

        let mut js_extra = format!("const BOOK_SPINE_ITEM_UID = '{}';", spine_item.spine_item_uid);

        if let Some(window_id_value) = window_id {
            js_extra.push_str(&format!(" const WINDOW_ID = '{}'; window.WINDOW_ID = WINDOW_ID;", window_id_value));
        }

        if let Some(js_pre) = js_extra_pre {
            js_extra = format!("{}; {}", js_pre, js_extra);
        }

        js_extra.push_str(&format!(" const SHOW_BOOKMARKS = {};", show_bookmarks));

        // Build body_class with theme and language
        let mut body_class = self.get_theme_name();

        // Add language-specific class if available
        if let Some(ref lang) = spine_item.language
            && !lang.is_empty() {
                body_class.push_str(&format!(" lang-{}", lang));
            }

        // Query prev/next spine items to determine navigation button state
        let prev_item = self.dbm.appdata.get_prev_book_spine_item(&spine_item.spine_item_uid).ok().flatten();
        let next_item = self.dbm.appdata.get_next_book_spine_item(&spine_item.spine_item_uid).ok().flatten();

        let is_first_chapter = prev_item.is_none();
        let is_last_chapter = next_item.is_none();

        // Build the navigation HTML by replacing placeholders
        use crate::html_content::PREV_NEXT_CHAPTER_HTML;
        let nav_html = PREV_NEXT_CHAPTER_HTML
            .replace("{current_spine_item_uid}", &spine_item.spine_item_uid)
            .replace("{current_book_uid}", &spine_item.book_uid)
            .replace("{is_first_chapter}", &is_first_chapter.to_string())
            .replace("{is_last_chapter}", &is_last_chapter.to_string())
            .replace("{api_url}", &self.api_url);

        // Wrap content in the full HTML page structure
        use crate::html_content::sutta_html_page_with_nav;
        let final_html = sutta_html_page_with_nav(
            &content_html_body,
            Some(self.api_url.to_string()),
            Some(css_extra.to_string()),
            Some(js_extra.to_string()),
            Some(body_class),
            Some(nav_html),
            false,
        );

        Ok(final_html)
    }

    pub fn get_theme_name(&self) -> String {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.theme_name_as_string()
    }

    pub fn set_theme_name(&self, theme_name: &str) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.set_theme_name_from_str(theme_name);

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => {}
            Err(e) => error(&format!("{}", e))
        };
    }

    pub fn set_ai_models_auto_retry(&self, auto_retry: bool) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.ai_models_auto_retry = auto_retry;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => {}
            Err(e) => error(&format!("{}", e))
        };
    }

    pub fn get_api_key(&self, key_name: &str) -> String {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.api_keys.get(key_name).cloned().unwrap_or_default()
    }

    pub fn set_api_keys(&self, api_keys_json: &str) {
        use crate::db::appdata_schema::app_settings;

        let api_keys_map: IndexMap<String, String> = match serde_json::from_str(api_keys_json) {
            Ok(keys) => keys,
            Err(e) => {
                error(&format!("Failed to parse API keys JSON: {}", e));
                return;
            }
        };

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.api_keys = api_keys_map;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => {}
            Err(e) => error(&format!("{}", e))
        };
    }

    pub fn get_system_prompt(&self, prompt_name: &str) -> String {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.system_prompts.get(prompt_name).cloned().unwrap_or_default()
    }

    pub fn set_system_prompts_json(&self, prompts_json: &str) {
        use crate::db::appdata_schema::app_settings;

        let prompts_map: IndexMap<String, String> = match serde_json::from_str(prompts_json) {
            Ok(prompts) => prompts,
            Err(e) => {
                error(&format!("Failed to parse system prompts JSON: {}", e));
                return;
            }
        };

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.system_prompts = prompts_map;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => {}
            Err(e) => error(&format!("{}", e))
        };
    }

    pub fn get_system_prompts_json(&self) -> String {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        serde_json::to_string(&app_settings.system_prompts).unwrap_or_default()
    }

    /// The Gloss tab's AI word-selection settings as
    /// `{"enabled": bool, "provider": "...", "model": "..."}`.
    pub fn get_gloss_word_selection_settings_json(&self) -> String {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        serde_json::json!({
            "enabled": app_settings.gloss_word_selection_enabled,
            "provider": app_settings.gloss_word_selection_provider,
            "model": app_settings.gloss_word_selection_model,
        }).to_string()
    }

    /// Update the Gloss tab's AI word-selection settings from the same JSON
    /// shape `get_gloss_word_selection_settings_json` returns.
    pub fn set_gloss_word_selection_settings_json(&self, settings_json: &str) {
        #[derive(serde::Deserialize)]
        struct WordSelectionSettings {
            enabled: bool,
            provider: String,
            model: String,
        }

        let parsed: WordSelectionSettings = match serde_json::from_str(settings_json) {
            Ok(s) => s,
            Err(e) => {
                error(&format!("Failed to parse gloss word selection settings JSON: {}", e));
                return;
            }
        };

        let snapshot = {
            let mut s = self.app_settings_cache.write().expect("Failed to write app settings");
            s.gloss_word_selection_enabled = parsed.enabled;
            s.gloss_word_selection_provider = parsed.provider;
            s.gloss_word_selection_model = parsed.model;
            s.clone()
        };
        self.persist_app_settings(&snapshot);
    }

    pub fn get_providers_json(&self) -> String {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        serde_json::to_string(&app_settings.providers).unwrap_or_default()
    }

    /// A copy of the providers configuration, for callers which work with the
    /// typed structs (the model-list updater) rather than the QML JSON.
    pub fn get_providers(&self) -> Vec<Provider> {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.providers.clone()
    }

    /// Replace the whole providers configuration (the model-list updater's save
    /// path). Bypasses the per-model sync hooks, so the model-usage lists are
    /// reconciled separately by the caller.
    pub fn set_providers(&self, providers: Vec<Provider>) {
        self.mutate_providers(|current| {
            *current = providers;
            true
        });
    }

    /// Mutate the providers configuration in the cache and persist the whole
    /// settings row. The closure receives the providers vector; returning `false`
    /// means "nothing changed", so nothing is written.
    ///
    /// All provider/model mutations go through here (the bridge fns in
    /// `sutta_bridge.rs` are thin wrappers) so the model-usage list sync hooks
    /// have one place to live.
    fn mutate_providers<F>(&self, f: F)
    where
        F: FnOnce(&mut Vec<Provider>) -> bool,
    {
        let snapshot = {
            let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
            if !f(&mut app_settings.providers) {
                return;
            }
            app_settings.clone()
        };
        self.persist_app_settings(&snapshot);
    }

    /// The API key for a provider: the environment variable named by the provider
    /// config takes precedence over the value stored in the settings.
    pub fn get_provider_api_key(&self, provider_name: &str) -> String {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        let Some(provider) = app_settings.providers.iter().find(|p| p.name.as_str() == provider_name) else {
            return String::new();
        };
        if let Ok(env_key) = std::env::var(&provider.api_key_env_var_name) {
            return env_key;
        }
        provider.api_key_value.clone().unwrap_or_default()
    }

    pub fn set_provider_api_key(&self, provider_name: &str, api_key: &str) {
        self.mutate_providers(|providers| {
            match providers.iter_mut().find(|p| p.name.as_str() == provider_name) {
                Some(provider) => {
                    provider.api_key_value = if api_key.is_empty() { None } else { Some(api_key.to_string()) };
                    true
                }
                None => false,
            }
        });
    }

    pub fn set_provider_enabled(&self, provider_name: &str, enabled: bool) {
        self.mutate_providers(|providers| {
            match providers.iter_mut().find(|p| p.name.as_str() == provider_name) {
                Some(provider) => {
                    provider.enabled = enabled;
                    true
                }
                None => false,
            }
        });
    }

    /// Add a model the user typed in the models dialog. It is enabled, has
    /// `origin: user` and so is never removed by the model-list updater.
    pub fn add_provider_model(&self, provider_name: &str, model_name: &str) {
        self.mutate_providers(|providers| {
            let Some(provider) = providers.iter_mut().find(|p| p.name.as_str() == provider_name) else {
                return false;
            };
            if provider.models.iter().any(|m| m.model_name == model_name) {
                return false;
            }
            // Add to the top of the list, where the user can more easily see it.
            provider.models.insert(0, ModelEntry {
                model_name: model_name.to_string(),
                enabled: true,
                origin: ModelOrigin::User,
                stale: false,
            });
            true
        });
    }

    /// Remove a user-added model. `fetched` models are owned by the model-list
    /// updater and are not removable by hand.
    pub fn remove_provider_model(&self, provider_name: &str, model_name: &str) {
        self.mutate_providers(|providers| {
            let Some(provider) = providers.iter_mut().find(|p| p.name.as_str() == provider_name) else {
                return false;
            };
            let before = provider.models.len();
            provider.models.retain(|m| {
                !(m.model_name == model_name && m.origin == ModelOrigin::User)
            });
            provider.models.len() != before
        });
    }

    pub fn set_provider_model_enabled(&self, provider_name: &str, model_name: &str, enabled: bool) {
        self.mutate_providers(|providers| {
            let Some(provider) = providers.iter_mut().find(|p| p.name.as_str() == provider_name) else {
                return false;
            };
            match provider.models.iter_mut().find(|m| m.model_name == model_name) {
                Some(model) => {
                    model.enabled = enabled;
                    true
                }
                None => false,
            }
        });
    }

    /// The provider owning a model id. Model ids are provider-specific enough in
    /// practice that a first match is unambiguous.
    pub fn get_provider_for_model(&self, model_name: &str) -> String {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.providers.iter()
            .find(|p| p.models.iter().any(|m| m.model_name == model_name))
            .map(|p| p.name.as_str().to_string())
            .unwrap_or_default()
    }

    pub fn is_provider_enabled(&self, provider_name: &str) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.providers.iter()
            .find(|p| p.name.as_str() == provider_name)
            .map(|p| p.enabled)
            .unwrap_or(false)
    }

    pub fn set_providers_json(&self, providers_json: &str) {
        use crate::db::appdata_schema::app_settings;

        let providers_vec: Vec<Provider> = match serde_json::from_str(providers_json) {
            Ok(providers) => providers,
            Err(e) => {
                error(&format!("Failed to parse providers JSON: {}", e));
                return;
            }
        };

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.providers = providers_vec;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    /// Resolve any tolerated word-uid form onto the one canonical word, returning
    /// both its structured row (for JSON) and its correlated `dict_words` HTML row
    /// (for rendering). Shared by `get_word_json` (api.rs) and
    /// `render_word_html_by_uid` so the two routes resolve the same set of forms.
    ///
    /// Two correlated lanes are preserved (Finding 8 / the two-lane invariant):
    /// a numeric `<id>/dpd` keeps resolving to the **dpd_headwords** structured
    /// row, while the human/lemma forms (`dhamma 1.01`, `dhamma 1.01/dpd`,
    /// `dhamma-1-01/dpd`) resolve to the **dict_words** row `dhamma-1-01/dpd`.
    /// The dpd -> dict_words correlation is via `lemma_1` / root sanitize, not a
    /// uid string-equality join (there is no `dict_words` row with the numeric
    /// uid). See `docs/simsapa-localhost-api-search-endpoints.md`.
    pub fn resolve_word_uid(&self, input_uid: &str) -> Option<ResolvedWord> {
        let input = input_uid.trim().trim_end_matches(".json").trim();
        if input.is_empty() {
            return None;
        }

        // DPD bold-definitions use uids of the form "{bold_lc}/{ref_code_lc}"
        // (with collision disambiguation) which overlap the dict_word uid
        // namespace. Check bold_definitions first and route to the dedicated
        // renderer if matched.
        //
        // a) Bold definitions first: their uids ("{bold_lc}/{ref_code_lc}")
        //    overlap the dict_word namespace, so they must win the same way the
        //    HTML renderer checks them first.
        if let Some(bd) = self.dbm.dpd.get_bold_definition_by_uid(input) {
            if let Ok(value) = serde_json::to_value(&bd) {
                return Some(ResolvedWord {
                    canonical_uid: input.to_string(),
                    kind: ResolvedWordKind::BoldDefinition,
                    json_value: value,
                    dict_word: None,
                });
            }
        }

        // Try to get the word from database. The uid in dict_words is sanitized
        // (spaces replaced with dashes). Callers in QML often build the uid by
        // concatenating a DPD lemma_1 (which contains spaces, e.g. "gacchati 1")
        // with "/dpd", producing "gacchati 1/dpd". Fall back to a sanitized uid
        // so those reconstructions still resolve to the dict_words row.

        // b) DPD structured rows (only forms ending in "/dpd"), gated by the
        //    shape of the key so we only run the query that can actually match.
        //    Headword uids are always numeric ("34626/dpd") and root keys always
        //    start with "√" ("√kar/dpd"); the common sanitized dict_words form
        //    ("atthi-1.1/dpd") is neither, so it skips both and falls straight
        //    through to the dict_words lookup in (c) — avoiding two guaranteed
        //    "Record not found" misses on the hot path.
        if input.ends_with("/dpd") {
            // Headword by uid (numeric "<id>/dpd"). Correlate to its dict_words
            // HTML row via lemma_1 sanitize. A leading digit is enough to
            // distinguish it: headword uids are always numeric, while the
            // sanitized dict_words and root forms never start with a digit.
            if input.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                if let Some(json_str) = self.get_dpd_headword_by_uid(input) {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_str) {
                        let dict_word = value
                            .get("lemma_1")
                            .and_then(|v| v.as_str())
                            .and_then(|lemma| {
                                let canonical = format!("{}/dpd", word_uid_sanitize(lemma).to_lowercase());
                                self.dbm.dictionaries.get_word(&canonical)
                            })
                            .or_else(|| self.dbm.dictionaries.get_word(input));
                        return Some(ResolvedWord {
                            canonical_uid: input.to_string(),
                            kind: ResolvedWordKind::DpdHeadword,
                            json_value: value,
                            dict_word,
                        });
                    }
                }
            }

            // Root by root key ("√kar/dpd" -> root key "√kar").
            let root_key = input.trim_end_matches("/dpd");
            if root_key.starts_with('√') {
                if let Some(json_str) = self.get_dpd_root_by_root_key(root_key) {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_str) {
                        // Undisambiguated roots have a dict_words row at the same uid;
                        // disambiguated ones ("√path 1") do not, so this is best-effort
                        // (matches the pre-existing HTML behaviour).
                        let dict_word = self.dbm.dictionaries.get_word(input).or_else(|| {
                            let sanitized = word_uid_sanitize(input).to_lowercase();
                            if sanitized != input {
                                self.dbm.dictionaries.get_word(&sanitized)
                            } else {
                                None
                            }
                        });
                        return Some(ResolvedWord {
                            canonical_uid: input.to_string(),
                            kind: ResolvedWordKind::DpdRoot,
                            json_value: value,
                            dict_word,
                        });
                    }
                }
            }
        }

        // c) dict_words direct, with the existing sanitize retry (preserves the
        //    forms that already resolved, e.g. "dhamma-1-01/dpd", "dhamma/ncped").
        if let Some(dw) = self.dbm.dictionaries.get_word(input).or_else(|| {
            let sanitized = word_uid_sanitize(input).to_lowercase();
            if sanitized != input {
                self.dbm.dictionaries.get_word(&sanitized)
            } else {
                None
            }
        }) {
            return self.dict_word_resolved(dw);
        }

        // d) Normalized human form (e.g. bare "dhamma 1.01" -> "dhamma-1-01/dpd").
        let normalized = normalize_human_word_uid(input);
        if !normalized.is_empty() && normalized != input {
            if let Some(dw) = self.dbm.dictionaries.get_word(&normalized) {
                return self.dict_word_resolved(dw);
            }
        }

        None
    }

    /// Wrap a found `dict_words` row as a `ResolvedWord` (structured value =
    /// rendered-HTML row, both lanes the same record for a plain dict word).
    fn dict_word_resolved(&self, dw: DictWord) -> Option<ResolvedWord> {
        let value = serde_json::to_value(&dw).ok()?;
        Some(ResolvedWord {
            canonical_uid: dw.uid.clone(),
            kind: ResolvedWordKind::DictWord,
            json_value: value,
            dict_word: Some(dw),
        })
    }

    pub fn get_dpd_headword_by_uid(&self, uid_str: &str) -> Option<String> {
        use diesel::prelude::*;
        use crate::db::dpd_schema::dpd_headwords::dsl::*;

        let mut conn = match self.dbm.dpd.get_conn() {
            Ok(c) => c,
            Err(e) => {
                error(&format!("Failed to get DPD connection: {}", e));
                return None;
            }
        };

        let result = dpd_headwords
            .filter(uid.eq(uid_str))
            .first::<crate::db::dpd_models::DpdHeadword>(&mut conn);

        match result {
            Ok(headword) => {
                match serde_json::to_string(&headword) {
                    Ok(json) => Some(json),
                    Err(e) => {
                        error(&format!("Failed to serialize DPD headword: {}", e));
                        None
                    }
                }
            }
            // NotFound is expected: resolve_word_uid probes this for every
            // "/dpd" uid, including sanitized dict_words forms ("atthi-1.1/dpd")
            // and root keys ("√kar/dpd") that never have a headword row. Log it
            // at warn level; only genuine query errors are logged as errors.
            Err(diesel::result::Error::NotFound) => {
                warn(&format!("No DPD headword for uid {}", uid_str));
                None
            }
            Err(e) => {
                error(&format!("Failed to query DPD headword for uid {}: {}", uid_str, e));
                None
            }
        }
    }

    pub fn get_dpd_root_by_root_key(&self, root_key_str: &str) -> Option<String> {
        use diesel::prelude::*;
        use crate::db::dpd_schema::dpd_roots::dsl::*;

        let mut conn = match self.dbm.dpd.get_conn() {
            Ok(c) => c,
            Err(e) => {
                error(&format!("Failed to get DPD connection: {}", e));
                return None;
            }
        };

        let result = dpd_roots
            .filter(root.eq(root_key_str))
            .first::<crate::db::dpd_models::DpdRoot>(&mut conn);

        match result {
            Ok(dpd_root) => {
                match serde_json::to_string(&dpd_root) {
                    Ok(json) => Some(json),
                    Err(e) => {
                        error(&format!("Failed to serialize DPD root: {}", e));
                        None
                    }
                }
            }
            // NotFound is expected: resolve_word_uid probes this for every
            // "/dpd" uid (sanitized headword forms like "atthi-1.1/dpd" are not
            // root keys). Log it at warn level; only genuine query errors are
            // logged as errors.
            Err(diesel::result::Error::NotFound) => {
                warn(&format!("No DPD root for root_key {}", root_key_str));
                None
            }
            Err(e) => {
                error(&format!("Failed to query DPD root for root_key {}: {}", root_key_str, e));
                None
            }
        }
    }

    pub fn get_anki_template_front(&self) -> String {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.anki_template_front.clone()
    }

    pub fn set_anki_template_front(&self, template: &str) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.anki_template_front = template.to_string();

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_anki_template_back(&self) -> String {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.anki_template_back.clone()
    }

    pub fn set_anki_template_back(&self, template: &str) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.anki_template_back = template.to_string();

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_anki_template_cloze_front(&self) -> String {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.anki_template_cloze_front.clone()
    }

    pub fn set_anki_template_cloze_front(&self, template: &str) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.anki_template_cloze_front = template.to_string();

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_anki_template_cloze_back(&self) -> String {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.anki_template_cloze_back.clone()
    }

    pub fn set_anki_template_cloze_back(&self, template: &str) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.anki_template_cloze_back = template.to_string();

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_anki_export_format(&self) -> String {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        match app_settings.anki_export_format {
            crate::app_settings::AnkiExportFormat::Simple => "Simple".to_string(),
            crate::app_settings::AnkiExportFormat::Templated => "Templated".to_string(),
            crate::app_settings::AnkiExportFormat::DataCsv => "DataCsv".to_string(),
        }
    }

    pub fn set_anki_export_format(&self, format: &str) {
        use crate::db::appdata_schema::app_settings;
        use crate::app_settings::AnkiExportFormat;

        let export_format = match format {
            "Simple" => AnkiExportFormat::Simple,
            "Templated" => AnkiExportFormat::Templated,
            "DataCsv" => AnkiExportFormat::DataCsv,
            _ => {
                error(&format!("Unknown Anki export format: {}", format));
                return;
            }
        };

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.anki_export_format = export_format;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_anki_include_cloze(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.anki_include_cloze
    }

    pub fn set_anki_include_cloze(&self, include: bool) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.anki_include_cloze = include;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_search_as_you_type(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.search_as_you_type
    }

    pub fn set_search_as_you_type(&self, enabled: bool) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.search_as_you_type = enabled;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_include_cst_commentary_in_translations(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.include_cst_commentary_in_translations
    }

    pub fn set_include_cst_commentary_in_translations(&self, enabled: bool) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.include_cst_commentary_in_translations = enabled;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_include_cst_mula_in_search_results(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.include_cst_mula_in_search_results
    }

    pub fn set_include_cst_mula_in_search_results(&self, enabled: bool) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.include_cst_mula_in_search_results = enabled;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_include_ms_mula_in_search_results(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.include_ms_mula_in_search_results
    }

    pub fn get_include_comm_bold_definitions_in_search_results(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.include_comm_bold_definitions_in_search_results
    }

    pub fn set_include_comm_bold_definitions_in_search_results(&self, enabled: bool) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.include_comm_bold_definitions_in_search_results = enabled;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn set_include_ms_mula_in_search_results(&self, enabled: bool) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.include_ms_mula_in_search_results = enabled;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_include_cst_commentary_in_search_results(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.include_cst_commentary_in_search_results
    }

    pub fn set_include_cst_commentary_in_search_results(&self, enabled: bool) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.include_cst_commentary_in_search_results = enabled;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_include_cst_mula_in_translations(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.include_cst_mula_in_translations
    }

    pub fn set_include_cst_mula_in_translations(&self, enabled: bool) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.include_cst_mula_in_translations = enabled;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_open_find_in_sutta_results(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.open_find_in_sutta_results
    }

    pub fn set_open_find_in_sutta_results(&self, enabled: bool) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.open_find_in_sutta_results = enabled;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_show_bottom_footnotes(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.show_bottom_footnotes
    }

    pub fn set_show_bottom_footnotes(&self, enabled: bool) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.show_bottom_footnotes = enabled;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    /// Last-used language filter key for the given area (`"Suttas"`,
    /// `"Dictionary"`, `"Library"`). Returns an empty string when unset, which
    /// the UI and query layer both treat as "no language filter" (equivalent to
    /// the `"Language"` sentinel). Mirrors `get_last_search_mode`.
    pub fn get_language_filter_key(&self, area: &str) -> String {
        let s = self.app_settings_cache.read().expect("Failed to read app settings");
        s.search_last_language.get(area).cloned().unwrap_or_default()
    }

    /// Persist the language filter key for the given area. Mirrors
    /// `set_last_search_mode`: the in-memory cache is updated synchronously and
    /// the disk write happens off the calling (UI) thread.
    pub fn set_language_filter_key(&self, area: &str, key: &str) {
        {
            let mut s = self.app_settings_cache.write().expect("Failed to write app settings");
            s.search_last_language.insert(area.to_string(), key.to_string());
        }
        std::thread::spawn(|| {
            let app_data = crate::get_app_data();
            let snapshot = app_data
                .app_settings_cache
                .read()
                .expect("Failed to read app settings")
                .clone();
            app_data.persist_app_settings(&snapshot);
        });
    }

    pub fn set_mobile_top_bar_margin_system(&self) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.set_mobile_top_bar_margin_system();

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn set_mobile_top_bar_margin_custom(&self, value: u32) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.set_mobile_top_bar_margin_custom(value);

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    /// Import an EPUB document into the database
    #[allow(clippy::too_many_arguments)]
    pub fn import_epub_to_db(&self, epub_path: &std::path::Path, book_uid: &str, custom_title: Option<&str>, custom_author: Option<&str>, custom_language: Option<&str>, custom_enable_embedded_css: Option<bool>, is_user_added: bool) -> Result<()> {
        {
            let db_conn = &mut self.dbm.appdata.get_conn()
                .context("Failed to get database connection")?;
            crate::epub_import::import_epub_to_db(db_conn, epub_path, book_uid, custom_title, custom_author, custom_language, custom_enable_embedded_css, is_user_added)?;
        }
        // Refresh stats: a new book adds rows to books / book_spine_items /
        // book_resources / books_fts. See docs/user-data-and-sqlite-analyze.md.
        self.dbm.appdata.analyze("appdata");
        Ok(())
    }

    /// Import a PDF document into the database
    #[allow(clippy::too_many_arguments)]
    pub fn import_pdf_to_db(&self, pdf_path: &std::path::Path, book_uid: &str, custom_title: Option<&str>, custom_author: Option<&str>, custom_language: Option<&str>, custom_enable_embedded_css: Option<bool>, is_user_added: bool) -> Result<()> {
        {
            let db_conn = &mut self.dbm.appdata.get_conn()
                .context("Failed to get database connection")?;
            crate::pdf_import::import_pdf_to_db(db_conn, pdf_path, book_uid, custom_title, custom_author, custom_language, custom_enable_embedded_css, is_user_added)?;
        }
        self.dbm.appdata.analyze("appdata");
        Ok(())
    }

    /// Import an HTML document into the database
    #[allow(clippy::too_many_arguments)]
    pub fn import_html_to_db(&self, html_path: &std::path::Path, book_uid: &str, custom_title: Option<&str>, custom_author: Option<&str>, custom_language: Option<&str>, custom_enable_embedded_css: Option<bool>, is_user_added: bool) -> Result<()> {
        {
            let db_conn = &mut self.dbm.appdata.get_conn()
                .context("Failed to get database connection")?;
            crate::html_import::import_html_to_db(db_conn, html_path, book_uid, custom_title, custom_author, custom_language, custom_enable_embedded_css, is_user_added)?;
        }
        self.dbm.appdata.analyze("appdata");
        Ok(())
    }

    pub fn get_first_time_start(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.first_time_start
    }

    pub fn set_first_time_start(&self, is_first_time: bool) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.first_time_start = is_first_time;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn check_and_configure_for_first_start(&self) {
        if !self.get_first_time_start() {
            return;
        }

        // Use 6 MB as the low memory threshold. Phones with 8 MB RAM can handle incremental fulltext search fine.
        if let Some(total_memory_gb) = self.get_system_memory_gb()
            && total_memory_gb <= 6 {
                self.set_search_as_you_type(false);
            }

        self.set_first_time_start(false);
    }

    /// Get the database version from the appdata database.
    ///
    /// Queries the `app_settings` table for the 'db_version' key.
    ///
    /// # Returns
    ///
    /// * `Some(String)` - The database version if found
    /// * `None` - If the database doesn't exist or version not found
    pub fn get_db_version(&self) -> Option<String> {
        use crate::db::appdata_schema::app_settings;

        let db_conn = &mut self.dbm.appdata.get_conn().ok()?;

        app_settings::table
            .filter(app_settings::key.eq("db_version"))
            .select(app_settings::value)
            .first::<Option<String>>(db_conn)
            .ok()?
    }

    /// Get the release channel from app settings.
    ///
    /// # Returns
    ///
    /// * `Some(String)` - The release channel if configured
    /// * `None` - If not configured (will default to "simsapa-ng")
    pub fn get_release_channel(&self) -> Option<String> {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.release_channel.clone()
    }

    /// Get whether to notify about Simsapa updates.
    ///
    /// # Returns
    ///
    /// `true` if update notifications are enabled (default), `false` otherwise
    pub fn get_notify_about_simsapa_updates(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.notify_about_simsapa_updates
    }

    /// Set whether to notify about Simsapa updates.
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether to show update notifications on startup
    pub fn set_notify_about_simsapa_updates(&self, enabled: bool) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.notify_about_simsapa_updates = enabled;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_restore_last_session(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.restore_last_session
    }

    pub fn set_restore_last_session(&self, enabled: bool) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.restore_last_session = enabled;

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    pub fn get_render_use_flat_results_background(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.render_use_flat_results_background
    }

    pub fn set_render_use_flat_results_background(&self, enabled: bool) {
        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.render_use_flat_results_background = enabled;
        self.persist_app_settings(&app_settings);
    }

    pub fn get_render_disable_results_clip(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.render_disable_results_clip
    }

    pub fn set_render_disable_results_clip(&self, enabled: bool) {
        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.render_disable_results_clip = enabled;
        self.persist_app_settings(&app_settings);
    }

    pub fn get_render_loop_basic(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.render_loop_basic
    }

    pub fn set_render_loop_basic(&self, enabled: bool) {
        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.render_loop_basic = enabled;
        self.persist_app_settings(&app_settings);
    }

    // --- Snippet display settings ---

    pub fn get_snippet_chars_before(&self) -> usize {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.snippet_chars_before
    }

    pub fn set_snippet_chars_before(&self, value: usize) {
        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.snippet_chars_before = value;
        self.persist_app_settings(&app_settings);
    }

    pub fn get_snippet_chars_after(&self) -> usize {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.snippet_chars_after
    }

    pub fn set_snippet_chars_after(&self, value: usize) {
        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.snippet_chars_after = value;
        self.persist_app_settings(&app_settings);
    }

    pub fn get_snippet_all_chars_before(&self) -> usize {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.snippet_all_chars_before
    }

    pub fn set_snippet_all_chars_before(&self, value: usize) {
        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.snippet_all_chars_before = value;
        self.persist_app_settings(&app_settings);
    }

    pub fn get_snippet_all_chars_after(&self) -> usize {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.snippet_all_chars_after
    }

    pub fn set_snippet_all_chars_after(&self, value: usize) {
        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.snippet_all_chars_after = value;
        self.persist_app_settings(&app_settings);
    }

    pub fn get_item_height_use_default(&self) -> bool {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.item_height_use_default
    }

    pub fn set_item_height_use_default(&self, enabled: bool) {
        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.item_height_use_default = enabled;
        self.persist_app_settings(&app_settings);
    }

    pub fn get_item_height_fixed(&self) -> usize {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.item_height_fixed
    }

    pub fn set_item_height_fixed(&self, value: usize) {
        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.item_height_fixed = value;
        self.persist_app_settings(&app_settings);
    }

    /// Get the current keybindings as a JSON string.
    pub fn get_keybindings_json(&self) -> String {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        serde_json::to_string(&app_settings.app_keybindings.bindings).expect("Failed to serialize keybindings")
    }

    /// Get the default keybindings as a JSON string.
    pub fn get_default_keybindings_json(&self) -> String {
        let default_keybindings = crate::app_settings::AppKeybindings::default();
        serde_json::to_string(&default_keybindings.bindings).expect("Failed to serialize default keybindings")
    }

    /// Get the action names mapping as a JSON string.
    pub fn get_action_names_json(&self) -> String {
        let action_names = crate::app_settings::AppKeybindings::get_action_names();
        serde_json::to_string(&action_names).expect("Failed to serialize action names")
    }

    /// Get the action descriptions mapping as a JSON string.
    pub fn get_action_descriptions_json(&self) -> String {
        let action_descriptions = crate::app_settings::AppKeybindings::get_action_descriptions();
        serde_json::to_string(&action_descriptions).expect("Failed to serialize action descriptions")
    }

    /// Set the shortcuts for a specific action.
    ///
    /// # Arguments
    ///
    /// * `action_id` - The action identifier (e.g., "focus_search")
    /// * `shortcuts` - List of keyboard shortcuts (e.g., ["Ctrl+L"])
    pub fn set_keybinding(&self, action_id: &str, shortcuts: Vec<String>) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.app_keybindings.bindings.insert(action_id.to_string(), shortcuts);

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    /// Reset a single action's keybindings to default.
    ///
    /// # Arguments
    ///
    /// * `action_id` - The action identifier to reset
    pub fn reset_keybinding(&self, action_id: &str) {
        use crate::db::appdata_schema::app_settings;

        let default_keybindings = crate::app_settings::AppKeybindings::default();
        if let Some(default_shortcuts) = default_keybindings.bindings.get(action_id) {
            let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
            app_settings.app_keybindings.bindings.insert(action_id.to_string(), default_shortcuts.clone());

            let a = app_settings.clone();
            let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

            let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

            match diesel::update(app_settings::table)
                .filter(app_settings::key.eq("app_settings"))
                .set(app_settings::value.eq(Some(settings_json)))
                .execute(db_conn)
            {
                Ok(_) => (),
                Err(e) => error(&format!("Failed to update app settings: {}", e)),
            }
        }
    }

    /// Reset all keybindings to their defaults.
    pub fn reset_all_keybindings(&self) {
        use crate::db::appdata_schema::app_settings;

        let mut app_settings = self.app_settings_cache.write().expect("Failed to write app settings");
        app_settings.app_keybindings = crate::app_settings::AppKeybindings::default();

        let a = app_settings.clone();
        let settings_json = serde_json::to_string(&a).expect("Can't encode JSON");

        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");

        match diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(settings_json)))
            .execute(db_conn)
        {
            Ok(_) => (),
            Err(e) => error(&format!("Failed to update app settings: {}", e)),
        }
    }

    fn get_system_memory_gb(&self) -> Option<u64> {
        get_system_memory_bytes().map(|bytes| bytes / 1024 / 1024 / 1024)
    }

    /// Export user data to the import-me folder for database upgrade.
    ///
    /// This creates an "import-me" folder in the simsapa directory and exports:
    /// - app_settings.json: The current application settings
    /// - download_languages.txt: CSV list of languages in the database (except 'san', 'en', 'pli')
    /// - download_select_sanskrit_bundle.txt: If 'san' language is present
    /// - appdata.sqlite3: A database with user-imported books and their related data
    ///
    /// Per-category failures do not short-circuit the remaining exports — every
    /// category is attempted and errors are collected. Returns `Err(Vec<(category,
    /// message)>)` if any category failed, `Ok(())` otherwise.
    pub fn export_user_data_to_assets(&self) -> std::result::Result<(), Vec<(String, String)>> {
        let mut errors: Vec<(String, String)> = Vec::new();

        let globals = get_app_globals();
        let app_assets_dir = &globals.paths.app_assets_dir;
        let import_dir = app_assets_dir.join("import-me");

        info(&format!("export_user_data_to_assets(): Creating import-me folder at {}", import_dir.display()));

        // 1.6: clear any pre-existing import-me/ artifacts from a previous
        // cancelled upgrade attempt before writing anything new.
        //
        // Exception: `user_dictionaries.sqlite3` (PRD task 5.6) — this snapshot
        // is consumed at startup by the dictionary reconciliation pass, not
        // during the upgrade-export phase. If a prior upgrade left one behind,
        // it must survive until the next startup consumes it; the dictionary
        // export step (`export_user_dictionaries`) refuses to overwrite it.
        if let Ok(true) = import_dir.try_exists() {
            info(&format!(
                "Clearing pre-existing import-me directory entries (preserving user_dictionaries.sqlite3): {}",
                import_dir.display()
            ));
            match std::fs::read_dir(&import_dir) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.file_name().is_some_and(|n| n == "user_dictionaries.sqlite3") {
                            continue;
                        }
                        let res = if p.is_dir() {
                            std::fs::remove_dir_all(&p)
                        } else {
                            std::fs::remove_file(&p)
                        };
                        if let Err(e) = res {
                            warn(&format!(
                                "Failed to remove pre-existing import-me entry {}: {}",
                                p.display(), e
                            ));
                        }
                    }
                }
                Err(e) => {
                    warn(&format!(
                        "Failed to read import-me directory {}: {}",
                        import_dir.display(), e
                    ));
                }
            }
        }

        if let Err(e) = std::fs::create_dir_all(&import_dir) {
            // Without an import-me directory we cannot run any exports — this
            // is a fatal setup error, not a per-category failure.
            errors.push((
                "setup".to_string(),
                format!("Failed to create import-me directory {}: {}", import_dir.display(), e),
            ));
            return Err(errors);
        }

        if let Err(e) = self.export_app_settings_json(&import_dir) {
            errors.push(("app_settings".to_string(), format!("{:#}", e)));
        }

        if let Err(e) = self.export_download_languages() {
            errors.push(("download_languages".to_string(), format!("{:#}", e)));
        }

        if let Err(e) = self.export_user_books(&import_dir) {
            errors.push(("books".to_string(), format!("{:#}", e)));
        }

        if let Err(e) = self.export_user_bookmarks(&import_dir) {
            errors.push(("bookmarks".to_string(), format!("{:#}", e)));
        }

        if let Err(e) = self.export_user_chanting_data(&import_dir) {
            errors.push(("chanting".to_string(), format!("{:#}", e)));
        }

        if let Err(e) = self.export_user_dictionaries(&import_dir) {
            errors.push(("user_dictionaries".to_string(), format!("{:#}", e)));
        }

        // One-shot legacy bridge: if userdata.sqlite3 still exists (alpha testers
        // upgrading from the pre-consolidation two-DB layout), pull its user data
        // into the standard per-table import files and keep a safety-net copy.
        if self.legacy_userdata_exists() {
            info("export_user_data_to_assets(): legacy userdata.sqlite3 detected — running one-shot bridge");
            if let Err(e) = self.export_from_legacy_userdata(&import_dir) {
                error(&format!("Legacy userdata export failed: {}", e));
                errors.push(("legacy_bridge".to_string(), format!("{:#}", e)));
            }
        }

        if errors.is_empty() {
            info("export_user_data_to_assets(): Export completed successfully");
            Ok(())
        } else {
            for (cat, msg) in &errors {
                error(&format!("export_user_data_to_assets(): {}: {}", cat, msg));
            }
            Err(errors)
        }
    }

    /// Returns true if the legacy `userdata.sqlite3` file exists in the app assets dir.
    /// Used only by the one-shot alpha-upgrade bridge.
    fn legacy_userdata_exists(&self) -> bool {
        let g = get_app_globals();
        let path = g.paths.app_assets_dir.join("userdata.sqlite3");
        matches!(path.try_exists(), Ok(true))
    }

    fn legacy_userdata_path(&self) -> PathBuf {
        let g = get_app_globals();
        g.paths.app_assets_dir.join("userdata.sqlite3")
    }

    /// One-shot legacy bridge: copies `userdata.sqlite3` into the import-me folder,
    /// applies idempotent schema upgrades to that copy, extracts `app_settings.json`,
    /// and aliases the copy under the per-table export filenames so the standard
    /// importer will pick them up — but only for tables not already exported.
    fn export_from_legacy_userdata(&self, import_dir: &Path) -> Result<()> {
        use diesel::sqlite::SqliteConnection;
        use crate::db::upgrade_appdata_schema;
        use crate::db::appdata_schema::app_settings;

        let legacy_path = self.legacy_userdata_path();

        // 5.3: safety-net full copy
        let safety_copy = import_dir.join("legacy-userdata.sqlite3");
        std::fs::copy(&legacy_path, &safety_copy)
            .with_context(|| format!("Failed to copy legacy userdata to {}", safety_copy.display()))?;
        info(&format!("Legacy bridge: copied userdata.sqlite3 to {}", safety_copy.display()));

        // Apply idempotent schema upgrades to the copy so Diesel models can load from it.
        let copy_url = format!("sqlite://{}", safety_copy.display());
        let mut legacy_conn = SqliteConnection::establish(&copy_url)
            .with_context(|| format!("Failed to open legacy userdata copy: {}", safety_copy.display()))?;
        upgrade_appdata_schema(&mut legacy_conn);

        // Extract app_settings.json from the legacy DB. Overwrites any standard export so
        // the alpha user's pre-upgrade settings take precedence.
        let app_settings_out = import_dir.join("app_settings.json");
        let row: Option<AppSetting> = app_settings::table
            .filter(app_settings::key.eq("app_settings"))
            .select(AppSetting::as_select())
            .first(&mut legacy_conn)
            .optional()
            .context("Failed to read legacy app_settings")?;
        if let Some(setting) = row
            && let Some(val) = setting.value {
                std::fs::write(&app_settings_out, &val)
                    .context("Failed to write app_settings.json from legacy")?;
                info("Legacy bridge: exported app_settings.json from legacy userdata");
            }

        // Alias the migrated copy under the per-table filenames the standard importer expects,
        // but only when those files are missing — the standard export from appdata wins.
        for target_name in ["appdata-bookmarks.sqlite3", "appdata-books.sqlite3", "appdata-chanting.sqlite3"] {
            let target_path = import_dir.join(target_name);
            match target_path.try_exists() {
                Ok(true) => {
                    info(&format!("Legacy bridge: {} already present — skipping", target_name));
                }
                _ => {
                    std::fs::copy(&safety_copy, &target_path)
                        .with_context(|| format!("Failed to copy legacy into {}", target_name))?;
                    info(&format!("Legacy bridge: aliased legacy into {}", target_name));
                }
            }
        }

        Ok(())
    }

    /// Export app settings to JSON file.
    fn export_app_settings_json(&self, import_dir: &Path) -> Result<()> {
        let app_settings = self.app_settings_cache.read().expect("Failed to read app settings");
        let settings_json = serde_json::to_string_pretty(&*app_settings)
            .context("Failed to serialize app settings to JSON")?;

        let settings_path = import_dir.join("app_settings.json");
        std::fs::write(&settings_path, settings_json)
            .with_context(|| format!("Failed to write app_settings.json to {}", settings_path.display()))?;

        info(&format!("Exported app_settings.json to {}", settings_path.display()));
        Ok(())
    }

    /// Export download languages to marker files.
    ///
    /// Creates download_languages.txt with a CSV list of languages (except 'san', 'en', 'pli').
    /// If 'san' is present, creates download_select_sanskrit_bundle.txt.
    ///
    /// These files are written to app_assets_dir and are read by DownloadAppdataWindow
    /// to auto-fill the language selection during database upgrades.
    fn export_download_languages(&self) -> Result<()> {
        let globals = get_app_globals();
        let languages = self.dbm.get_sutta_languages();

        // Check for Sanskrit bundle
        if languages.contains(&"san".to_string()) {
            let sanskrit_path = &globals.paths.download_select_sanskrit_bundle_marker;
            std::fs::write(sanskrit_path, "True")
                .with_context(|| format!("Failed to write download_select_sanskrit_bundle.txt to {}", sanskrit_path.display()))?;
            info(&format!("Created download_select_sanskrit_bundle.txt at {}", sanskrit_path.display()));
        }

        // Filter out default languages
        let filtered_languages: Vec<String> = languages
            .into_iter()
            .filter(|lang| lang != "san" && lang != "en" && lang != "pli")
            .collect();

        if !filtered_languages.is_empty() {
            let languages_csv = filtered_languages.join(", ");
            let languages_path = &globals.paths.download_languages_marker;
            std::fs::write(languages_path, &languages_csv)
                .with_context(|| format!("Failed to write download_languages.txt to {}", languages_path.display()))?;
            info(&format!("Exported download_languages.txt with: {}", languages_csv));
        }

        Ok(())
    }

    /// Export user-imported books to a new appdata.sqlite3 database.
    ///
    /// User-imported books are those not in the original dataset (identified by their UIDs).
    fn export_user_books(&self, import_dir: &Path) -> Result<()> {
        use crate::db::appdata_schema::{books, book_spine_items, book_resources};
        use crate::db::APPDATA_MIGRATIONS;
        use diesel::sqlite::SqliteConnection;
        use diesel_migrations::MigrationHarness;

        // Get user-imported books (filtered by is_user_added)
        let db_conn = &mut self.dbm.appdata.get_conn()
            .context("Failed to get appdata connection")?;

        let user_books: Vec<Book> = books::table
            .filter(books::is_user_added.eq(true))
            .load::<Book>(db_conn)
            .context("Failed to load user books")?;

        if user_books.is_empty() {
            info("No user-imported books to export");
            return Ok(());
        }

        info(&format!("Found {} user-imported books to export", user_books.len()));

        // Create export database
        let export_db_path = import_dir.join("appdata-books.sqlite3");
        if export_db_path.exists() {
            std::fs::remove_file(&export_db_path)
                .with_context(|| format!("Failed to remove existing export database: {}", export_db_path.display()))?;
        }

        let export_db_url = format!("sqlite://{}", export_db_path.display());
        let mut export_conn = SqliteConnection::establish(&export_db_url)
            .with_context(|| format!("Failed to create export database: {}", export_db_path.display()))?;

        // Create tables using diesel migrations to ensure schema stays in sync
        export_conn.run_pending_migrations(APPDATA_MIGRATIONS)
            .map_err(|e| anyhow!("Failed to run migrations on export database: {}", e))?;

        // Export each user book with its related data
        for book in &user_books {
            // Insert book using diesel model struct
            let new_book = NewBook {
                uid: &book.uid,
                document_type: &book.document_type,
                title: book.title.as_deref(),
                author: book.author.as_deref(),
                language: book.language.as_deref(),
                file_path: book.file_path.as_deref(),
                metadata_json: book.metadata_json.as_deref(),
                enable_embedded_css: book.enable_embedded_css,
                toc_json: book.toc_json.as_deref(),
                is_user_added: book.is_user_added,
            };

            diesel::insert_into(books::table)
                .values(&new_book)
                .execute(&mut export_conn)
                .with_context(|| format!("Failed to insert book: {}", book.uid))?;

            // Get the inserted book's ID (it will be different from the source ID)
            let exported_book_id: i32 = books::table
                .filter(books::uid.eq(&book.uid))
                .select(books::id)
                .first(&mut export_conn)
                .with_context(|| format!("Failed to get exported book ID for: {}", book.uid))?;

            // Get and insert spine items for this book
            let spine_items: Vec<BookSpineItem> = book_spine_items::table
                .filter(book_spine_items::book_id.eq(book.id))
                .load::<BookSpineItem>(db_conn)
                .with_context(|| format!("Failed to load spine items for book: {}", book.uid))?;

            for spine_item in &spine_items {
                let new_spine_item = NewBookSpineItem {
                    book_id: exported_book_id,
                    book_uid: &spine_item.book_uid,
                    spine_item_uid: &spine_item.spine_item_uid,
                    spine_index: spine_item.spine_index,
                    resource_path: &spine_item.resource_path,
                    title: spine_item.title.as_deref(),
                    language: spine_item.language.as_deref(),
                    content_html: spine_item.content_html.as_deref(),
                    content_plain: spine_item.content_plain.as_deref(),
                };

                diesel::insert_into(book_spine_items::table)
                    .values(&new_spine_item)
                    .execute(&mut export_conn)
                    .with_context(|| format!("Failed to insert spine item: {}", spine_item.spine_item_uid))?;
            }

            // Get and insert resources for this book
            let resources: Vec<BookResource> = book_resources::table
                .filter(book_resources::book_id.eq(book.id))
                .load::<BookResource>(db_conn)
                .with_context(|| format!("Failed to load resources for book: {}", book.uid))?;

            for resource in &resources {
                let new_resource = NewBookResource {
                    book_id: exported_book_id,
                    book_uid: &resource.book_uid,
                    resource_path: &resource.resource_path,
                    mime_type: resource.mime_type.as_deref(),
                    content_data: resource.content_data.as_deref(),
                };

                diesel::insert_into(book_resources::table)
                    .values(&new_resource)
                    .execute(&mut export_conn)
                    .with_context(|| format!("Failed to insert resource: {}", resource.resource_path))?;
            }

            info(&format!("Exported book: {} with {} spine items and {} resources",
                         book.uid, spine_items.len(), resources.len()));
        }

        info(&format!("Exported {} user books to {}", user_books.len(), export_db_path.display()));
        Ok(())
    }

    /// Import user data from the import-me folder after database upgrade.
    ///
    /// This reads from the "import-me" folder in the simsapa directory and imports:
    /// - app_settings.json: Restores the application settings
    /// - appdata-books.sqlite3: Imports user books back into the new database
    ///
    /// After successful import, the import-me folder is deleted.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If import was successful (or nothing to import)
    /// * `Err` - If any error occurred during import
    pub fn import_user_data_from_assets(&self) -> Result<()> {
        let globals = get_app_globals();
        let import_dir = globals.paths.app_assets_dir.join("import-me");

        if !import_dir.exists() {
            info("import_user_data_from_assets(): No import-me folder found, skipping import");
            return Ok(());
        }

        info(&format!("import_user_data_from_assets(): Found import-me folder at {}", import_dir.display()));

        // Import app_settings.json
        if let Err(e) = self.import_app_settings_json(&import_dir) {
            error(&format!("Failed to import app settings: {}", e));
            // Continue with other imports even if settings fail
        }

        // Import user books from appdata.sqlite3
        if let Err(e) = self.import_user_books(&import_dir) {
            error(&format!("Failed to import user books: {}", e));
            // Continue with cleanup even if books import fails
        }

        // Import user bookmarks
        if let Err(e) = self.import_user_bookmarks(&import_dir) {
            error(&format!("Failed to import user bookmarks: {}", e));
        }

        // Import user chanting data and recordings
        if let Err(e) = self.import_user_chanting_data(&import_dir) {
            error(&format!("Failed to import user chanting data: {}", e));
        }

        // Import user-imported dictionaries snapshot. Failure here must NOT
        // wipe the snapshot — the file remains in import-me/ for the next
        // startup attempt (the cleanup step below excludes it).
        if let Err(e) = self.import_user_dictionaries(&import_dir) {
            error(&format!("Failed to import user dictionaries: {}", e));
        }

        // Defensive tail pass for the one-shot legacy bridge: if legacy-userdata.sqlite3
        // is present and the current app_settings in appdata still looks like defaults,
        // re-apply the legacy app_settings. (Bookmark/book/chanting tables already got
        // aliased copies during export, so the standard importer handled those.)
        let legacy_copy = import_dir.join("legacy-userdata.sqlite3");
        if matches!(legacy_copy.try_exists(), Ok(true)) {
            info("import_user_data_from_assets(): legacy-userdata.sqlite3 present — running defensive tail pass");
            if let Err(e) = self.defensive_reapply_legacy_app_settings(&legacy_copy) {
                error(&format!("Defensive legacy app_settings re-apply failed: {}", e));
            }
        }

        // Clean up: remove the import-me folder, but preserve
        // `user_dictionaries.sqlite3` if it is still present (PRD task 5.6).
        // If the dictionary import succeeded the importer already deleted
        // the file; if it failed, the file must survive so the next startup
        // can retry.
        let dict_snapshot = import_dir.join("user_dictionaries.sqlite3");
        let preserve_dict_snapshot = matches!(dict_snapshot.try_exists(), Ok(true));
        if preserve_dict_snapshot {
            // Remove every entry except the snapshot.
            match std::fs::read_dir(&import_dir) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p == dict_snapshot {
                            continue;
                        }
                        let res = if p.is_dir() {
                            std::fs::remove_dir_all(&p)
                        } else {
                            std::fs::remove_file(&p)
                        };
                        if let Err(e) = res {
                            error(&format!("Failed to remove import-me entry {}: {}", p.display(), e));
                        }
                    }
                    info("import_user_data_from_assets(): cleared import-me folder, preserving user_dictionaries.sqlite3 for next startup");
                }
                Err(e) => {
                    error(&format!("Failed to read import-me folder for selective cleanup: {}", e));
                }
            }
        } else if let Err(e) = std::fs::remove_dir_all(&import_dir) {
            error(&format!("Failed to remove import-me folder: {}", e));
        } else {
            info("import_user_data_from_assets(): Removed import-me folder after import");
        }

        info("import_user_data_from_assets(): Import completed");
        Ok(())
    }

    /// Defensive tail pass for the one-shot legacy bridge.
    ///
    /// Re-reads `app_settings` from the legacy-userdata copy and applies it into appdata
    /// unconditionally. The standard importer's `app_settings.json` step is usually the
    /// source of truth, but this pass guards against the JSON extraction failing silently.
    fn defensive_reapply_legacy_app_settings(&self, legacy_copy: &Path) -> Result<()> {
        use diesel::sqlite::SqliteConnection;
        use crate::db::appdata_schema::app_settings;

        let url = format!("sqlite://{}", legacy_copy.display());
        let mut conn = SqliteConnection::establish(&url)
            .with_context(|| format!("Failed to open legacy userdata copy: {}", legacy_copy.display()))?;

        let row: Option<AppSetting> = app_settings::table
            .filter(app_settings::key.eq("app_settings"))
            .select(AppSetting::as_select())
            .first(&mut conn)
            .optional()
            .context("Failed to read legacy app_settings")?;

        let Some(setting) = row else {
            info("Defensive tail: legacy app_settings row not found");
            return Ok(());
        };
        let Some(val) = setting.value else {
            info("Defensive tail: legacy app_settings value is NULL");
            return Ok(());
        };

        // Parse once to validate the JSON and to seed the in-memory cache, but
        // write the original bytes back to the DB — re-serializing would only
        // reproduce the same content (modulo field ordering) and could drop
        // any future fields we haven't taught `AppSettings` about yet.
        let imported: AppSettings = serde_json::from_str(&val)
            .context("Failed to parse legacy app_settings JSON")?;

        {
            let mut cache = self.app_settings_cache.write().expect("Failed to write app settings");
            *cache = imported;
        }

        let db_conn = &mut self.dbm.appdata.get_conn()
            .context("Failed to get appdata connection")?;
        diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(val.as_str())))
            .execute(db_conn)
            .context("Failed to update app_settings from legacy tail pass")?;

        info("Defensive tail: legacy app_settings re-applied to appdata");
        Ok(())
    }

    /// Import app settings from JSON file.
    fn import_app_settings_json(&self, import_dir: &Path) -> Result<()> {
        use crate::db::appdata_schema::app_settings;

        let settings_path = import_dir.join("app_settings.json");

        if !settings_path.exists() {
            info("No app_settings.json found in import-me folder");
            return Ok(());
        }

        let settings_json = std::fs::read_to_string(&settings_path)
            .with_context(|| format!("Failed to read app_settings.json from {}", settings_path.display()))?;

        let imported_settings: AppSettings = serde_json::from_str(&settings_json)
            .with_context(|| "Failed to parse app_settings.json")?;

        // Update the app settings cache with imported settings
        {
            let mut cache = self.app_settings_cache.write().expect("Failed to write app settings");
            *cache = imported_settings;
        }

        // Save the imported settings to the database
        let cache = self.app_settings_cache.read().expect("Failed to read app settings");
        let serialized_json = serde_json::to_string(&*cache)
            .context("Failed to serialize app settings to JSON")?;

        let db_conn = &mut self.dbm.appdata.get_conn()
            .context("Failed to get appdata connection")?;

        diesel::update(app_settings::table)
            .filter(app_settings::key.eq("app_settings"))
            .set(app_settings::value.eq(Some(serialized_json)))
            .execute(db_conn)
            .context("Failed to update app settings in database")?;

        info(&format!("Imported app settings from {}", settings_path.display()));
        Ok(())
    }

    /// Import user books from the export database.
    ///
    /// Reads books from the import-me/appdata-books.sqlite3 and inserts them into the new database.
    fn import_user_books(&self, import_dir: &Path) -> Result<()> {
        use crate::db::appdata_schema::{books, book_spine_items, book_resources};
        use diesel::sqlite::SqliteConnection;

        let import_db_path = import_dir.join("appdata-books.sqlite3");

        if !import_db_path.exists() {
            info("No appdata-books.sqlite3 found in import-me folder");
            return Ok(());
        }

        info(&format!("Importing user books from {}", import_db_path.display()));

        // Open the import database
        let import_db_url = format!("sqlite://{}", import_db_path.display());
        let mut import_conn = SqliteConnection::establish(&import_db_url)
            .with_context(|| format!("Failed to open import database: {}", import_db_path.display()))?;

        // Filter to user-added rows only. The legacy one-shot bridge aliases
        // the full userdata.sqlite3 copy as appdata-books.sqlite3, which could
        // contain bootstrap-seeded rows copied around in earlier alpha builds;
        // UID collisions would skip them anyway, but the filter keeps the
        // import contract explicit and avoids touching seeded rows.
        let import_books: Vec<Book> = books::table
            .filter(books::is_user_added.eq(true))
            .load::<Book>(&mut import_conn)
            .context("Failed to load books from import database")?;

        if import_books.is_empty() {
            info("No books to import from appdata-books.sqlite3");
            return Ok(());
        }

        info(&format!("Found {} books to import", import_books.len()));

        // Get connection to the current appdata database
        let db_conn = &mut self.dbm.appdata.get_conn()
            .context("Failed to get appdata connection")?;

        // Import each book with its related data
        for book in &import_books {
            // Check if book already exists in the current database (by UID)
            let existing_book: Option<i32> = books::table
                .filter(books::uid.eq(&book.uid))
                .select(books::id)
                .first(db_conn)
                .optional()
                .context("Failed to check for existing book")?;

            if existing_book.is_some() {
                info(&format!("Book {} already exists, skipping", book.uid));
                continue;
            }

            // Insert the book
            let new_book = NewBook {
                uid: &book.uid,
                document_type: &book.document_type,
                title: book.title.as_deref(),
                author: book.author.as_deref(),
                language: book.language.as_deref(),
                file_path: book.file_path.as_deref(),
                metadata_json: book.metadata_json.as_deref(),
                enable_embedded_css: book.enable_embedded_css,
                toc_json: book.toc_json.as_deref(),
                is_user_added: book.is_user_added,
            };

            diesel::insert_into(books::table)
                .values(&new_book)
                .execute(db_conn)
                .with_context(|| format!("Failed to insert book: {}", book.uid))?;

            // Get the inserted book's ID
            let new_book_id: i32 = books::table
                .filter(books::uid.eq(&book.uid))
                .select(books::id)
                .first(db_conn)
                .with_context(|| format!("Failed to get new book ID for: {}", book.uid))?;

            // Get and insert spine items from import database
            let spine_items: Vec<BookSpineItem> = book_spine_items::table
                .filter(book_spine_items::book_id.eq(book.id))
                .load::<BookSpineItem>(&mut import_conn)
                .with_context(|| format!("Failed to load spine items for book: {}", book.uid))?;

            for spine_item in &spine_items {
                let new_spine_item = NewBookSpineItem {
                    book_id: new_book_id,
                    book_uid: &spine_item.book_uid,
                    spine_item_uid: &spine_item.spine_item_uid,
                    spine_index: spine_item.spine_index,
                    resource_path: &spine_item.resource_path,
                    title: spine_item.title.as_deref(),
                    language: spine_item.language.as_deref(),
                    content_html: spine_item.content_html.as_deref(),
                    content_plain: spine_item.content_plain.as_deref(),
                };

                diesel::insert_into(book_spine_items::table)
                    .values(&new_spine_item)
                    .execute(db_conn)
                    .with_context(|| format!("Failed to insert spine item: {}", spine_item.spine_item_uid))?;
            }

            // Get and insert resources from import database
            let resources: Vec<BookResource> = book_resources::table
                .filter(book_resources::book_id.eq(book.id))
                .load::<BookResource>(&mut import_conn)
                .with_context(|| format!("Failed to load resources for book: {}", book.uid))?;

            for resource in &resources {
                let new_resource = NewBookResource {
                    book_id: new_book_id,
                    book_uid: &resource.book_uid,
                    resource_path: &resource.resource_path,
                    mime_type: resource.mime_type.as_deref(),
                    content_data: resource.content_data.as_deref(),
                };

                diesel::insert_into(book_resources::table)
                    .values(&new_resource)
                    .execute(db_conn)
                    .with_context(|| format!("Failed to insert resource: {}", resource.resource_path))?;
            }

            info(&format!("Imported book: {} with {} spine items and {} resources",
                         book.uid, spine_items.len(), resources.len()));
        }

        info(&format!("Successfully imported {} user books", import_books.len()));
        Ok(())
    }

    /// Export user bookmarks to a SQLite database in the import-me folder.
    ///
    /// Skips the "Last Session" folder (is_last_session = true) since it is
    /// transient state that should not survive a database upgrade.
    fn export_user_bookmarks(&self, import_dir: &Path) -> Result<()> {
        use crate::db::appdata_schema::{bookmark_folders, bookmark_items};
        use crate::db::APPDATA_MIGRATIONS;
        use diesel::sqlite::SqliteConnection;
        use diesel_migrations::MigrationHarness;

        let db_conn = &mut self.dbm.appdata.get_conn()
            .context("Failed to get appdata connection for bookmark export")?;

        // Load all non-transient, user-added bookmark folders
        let folders: Vec<BookmarkFolder> = bookmark_folders::table
            .filter(bookmark_folders::is_last_session.eq(false))
            .filter(bookmark_folders::is_user_added.eq(true))
            .load::<BookmarkFolder>(db_conn)
            .context("Failed to load bookmark folders")?;

        if folders.is_empty() {
            info("No bookmark folders to export");
            return Ok(());
        }

        info(&format!("Exporting {} bookmark folders", folders.len()));

        let sqlite_path = import_dir.join("appdata-bookmarks.sqlite3");
        if let Ok(true) = sqlite_path.try_exists() {
            std::fs::remove_file(&sqlite_path)
                .with_context(|| format!("Failed to remove existing bookmark export database: {}", sqlite_path.display()))?;
        }

        let db_url = format!("sqlite://{}", sqlite_path.display());
        let mut export_conn = SqliteConnection::establish(&db_url)
            .with_context(|| format!("Failed to create bookmark export database: {}", sqlite_path.display()))?;

        export_conn.run_pending_migrations(APPDATA_MIGRATIONS)
            .map_err(|e| anyhow!("Failed to run migrations on bookmark export database: {}", e))?;

        let mut total_items = 0usize;

        for folder in &folders {
            let new_folder = NewBookmarkFolder {
                name: &folder.name,
                sort_order: folder.sort_order,
                is_last_session: false,
                is_user_added: folder.is_user_added,
            };

            diesel::insert_into(bookmark_folders::table)
                .values(&new_folder)
                .execute(&mut export_conn)
                .with_context(|| format!("Failed to insert bookmark folder: {}", folder.name))?;

            // Retrieve the newly inserted folder's id (may differ from source)
            let exported_folder_id: i32 = bookmark_folders::table
                .order(bookmark_folders::id.desc())
                .select(bookmark_folders::id)
                .first(&mut export_conn)
                .context("Failed to get exported bookmark folder id")?;

            // Load user-added items for this folder
            let items: Vec<BookmarkItem> = bookmark_items::table
                .filter(bookmark_items::folder_id.eq(folder.id))
                .filter(bookmark_items::is_user_added.eq(true))
                .load::<BookmarkItem>(db_conn)
                .with_context(|| format!("Failed to load bookmark items for folder: {}", folder.name))?;

            total_items += items.len();

            for item in &items {
                let new_item = NewBookmarkItem {
                    folder_id: exported_folder_id,
                    item_uid: item.item_uid.clone(),
                    table_name: item.table_name.clone(),
                    title: item.title.clone(),
                    tab_group: item.tab_group.clone(),
                    scroll_position: item.scroll_position,
                    find_query: item.find_query.clone(),
                    find_match_index: item.find_match_index,
                    sort_order: item.sort_order,
                    is_user_added: item.is_user_added,
                };

                diesel::insert_into(bookmark_items::table)
                    .values(&new_item)
                    .execute(&mut export_conn)
                    .context("Failed to insert bookmark item")?;
            }
        }

        info(&format!(
            "Exported {} bookmark folders with {} items to {}",
            folders.len(), total_items, sqlite_path.display()
        ));

        Ok(())
    }

    /// Import user bookmarks from the import-me folder after database upgrade.
    ///
    /// Reads `appdata-bookmarks.sqlite3` and inserts all folders and items into the
    /// new database, remapping folder ids as needed.
    fn import_user_bookmarks(&self, import_dir: &Path) -> Result<()> {
        use crate::db::appdata_schema::{bookmark_folders, bookmark_items};
        use diesel::sqlite::SqliteConnection;

        let sqlite_path = import_dir.join("appdata-bookmarks.sqlite3");
        match sqlite_path.try_exists() {
            Ok(true) => {}
            _ => {
                info("No appdata-bookmarks.sqlite3 found in import-me folder");
                return Ok(());
            }
        }

        info(&format!("Importing user bookmarks from {}", sqlite_path.display()));

        let db_url = format!("sqlite://{}", sqlite_path.display());
        let mut import_conn = SqliteConnection::establish(&db_url)
            .with_context(|| format!("Failed to open bookmark import database: {}", sqlite_path.display()))?;

        // Filter defensively: the legacy one-shot bridge aliases the full
        // userdata.sqlite3 copy as appdata-bookmarks.sqlite3, so the import DB
        // may contain transient `is_last_session = true` folders and rows that
        // predate the `is_user_added` column. Mirror the export contract here.
        let import_folders: Vec<BookmarkFolder> = bookmark_folders::table
            .filter(bookmark_folders::is_last_session.eq(false))
            .filter(bookmark_folders::is_user_added.eq(true))
            .load::<BookmarkFolder>(&mut import_conn)
            .context("Failed to load bookmark folders from import database")?;

        if import_folders.is_empty() {
            info("No bookmark folders to import");
            return Ok(());
        }

        let db_conn = &mut self.dbm.appdata.get_conn()
            .context("Failed to get appdata connection for bookmark import")?;

        let mut total_items = 0usize;

        for folder in &import_folders {
            // Skip if a folder with this name already exists to avoid duplicates
            let existing: Option<i32> = bookmark_folders::table
                .filter(bookmark_folders::name.eq(&folder.name))
                .select(bookmark_folders::id)
                .first(db_conn)
                .optional()
                .context("Failed to check for existing bookmark folder")?;

            let new_folder_id: i32 = if let Some(existing_id) = existing {
                info(&format!("Bookmark folder '{}' already exists, merging items into it", folder.name));
                existing_id
            } else {
                let new_folder = NewBookmarkFolder {
                    name: &folder.name,
                    sort_order: folder.sort_order,
                    is_last_session: false,
                    is_user_added: folder.is_user_added,
                };

                diesel::insert_into(bookmark_folders::table)
                    .values(&new_folder)
                    .execute(db_conn)
                    .with_context(|| format!("Failed to insert bookmark folder: {}", folder.name))?;

                bookmark_folders::table
                    .order(bookmark_folders::id.desc())
                    .select(bookmark_folders::id)
                    .first::<i32>(db_conn)
                    .context("Failed to get inserted bookmark folder id")?
            };

            // Load items from the import database for this source folder.
            // Filter by `is_user_added` to match the export contract — see the
            // folder-load site above for the legacy-bridge aliasing reason.
            let items: Vec<BookmarkItem> = bookmark_items::table
                .filter(bookmark_items::folder_id.eq(folder.id))
                .filter(bookmark_items::is_user_added.eq(true))
                .load::<BookmarkItem>(&mut import_conn)
                .with_context(|| format!("Failed to load bookmark items for folder: {}", folder.name))?;

            total_items += items.len();

            for item in &items {
                // Skip duplicate items (same item_uid in the same folder)
                let item_exists: bool = bookmark_items::table
                    .filter(bookmark_items::folder_id.eq(new_folder_id))
                    .filter(bookmark_items::item_uid.eq(&item.item_uid))
                    .select(bookmark_items::id)
                    .first::<i32>(db_conn)
                    .optional()
                    .unwrap_or(None)
                    .is_some();

                if item_exists {
                    continue;
                }

                let new_item = NewBookmarkItem {
                    folder_id: new_folder_id,
                    item_uid: item.item_uid.clone(),
                    table_name: item.table_name.clone(),
                    title: item.title.clone(),
                    tab_group: item.tab_group.clone(),
                    scroll_position: item.scroll_position,
                    find_query: item.find_query.clone(),
                    find_match_index: item.find_match_index,
                    sort_order: item.sort_order,
                    is_user_added: item.is_user_added,
                };

                diesel::insert_into(bookmark_items::table)
                    .values(&new_item)
                    .execute(db_conn)
                    .context("Failed to insert bookmark item")?;
            }
        }

        info(&format!(
            "Imported {} bookmark folders with {} items",
            import_folders.len(), total_items
        ));

        Ok(())
    }

    /// Export user chanting data and all recordings to the import-me folder.
    ///
    /// Creates `appdata-chanting.sqlite3` containing user-added collections/chants/sections
    /// and ALL recordings (including those on pre-shipped sections).
    /// Copies the entire `chanting-recordings/` directory into the import folder.
    fn export_user_chanting_data(&self, import_dir: &Path) -> Result<()> {
        use crate::db::appdata_schema::chanting_collections::dsl as col_dsl;
        use crate::db::appdata_schema::chanting_chants::dsl as chant_dsl;
        use crate::db::appdata_schema::chanting_sections::dsl as sec_dsl;
        use crate::db::appdata_schema::chanting_recordings::dsl as rec_dsl;
        use crate::db::chanting_export::create_chanting_sqlite;

        let db_conn = &mut self.dbm.appdata.get_conn()
            .context("Failed to get appdata connection for chanting export")?;

        // Query user-added collections, chants, sections
        let user_collections: Vec<ChantingCollection> = col_dsl::chanting_collections
            .filter(col_dsl::is_user_added.eq(true))
            .select(ChantingCollection::as_select())
            .load(db_conn)
            .context("Failed to load user chanting collections")?;

        let user_chants: Vec<ChantingChant> = chant_dsl::chanting_chants
            .filter(chant_dsl::is_user_added.eq(true))
            .select(ChantingChant::as_select())
            .load(db_conn)
            .context("Failed to load user chanting chants")?;

        let user_sections: Vec<ChantingSection> = sec_dsl::chanting_sections
            .filter(sec_dsl::is_user_added.eq(true))
            .select(ChantingSection::as_select())
            .load(db_conn)
            .context("Failed to load user chanting sections")?;

        // Query user-added recordings only (preserves recording_type: user-added
        // rows with recording_type='reference' are also carried across).
        let user_recordings: Vec<ChantingRecording> = rec_dsl::chanting_recordings
            .filter(rec_dsl::is_user_added.eq(true))
            .select(ChantingRecording::as_select())
            .load(db_conn)
            .context("Failed to load user chanting recordings")?;

        // Collect distinct section_uids referenced by user recordings, and load
        // the matching sections from appdata regardless of is_user_added. Seeded
        // ancestors must travel alongside user recordings so that FK targets in
        // the exported sqlite resolve on a fresh post-upgrade database.
        let recording_section_uids: std::collections::BTreeSet<String> = user_recordings
            .iter()
            .map(|r| r.section_uid.clone())
            .collect();

        let ancestor_sections: Vec<ChantingSection> = if recording_section_uids.is_empty() {
            Vec::new()
        } else {
            let uid_list: Vec<String> = recording_section_uids.iter().cloned().collect();
            sec_dsl::chanting_sections
                .filter(sec_dsl::uid.eq_any(&uid_list))
                .select(ChantingSection::as_select())
                .load(db_conn)
                .context("Failed to load ancestor chanting sections for export")?
        };

        // 1.2: collect distinct chant_uids from the ancestor sections (plus
        // user-added sections, since their chants may also be seeded) and load
        // matching chants regardless of is_user_added.
        let ancestor_chant_uids: std::collections::BTreeSet<String> = ancestor_sections
            .iter()
            .chain(user_sections.iter())
            .map(|s| s.chant_uid.clone())
            .collect();

        let ancestor_chants: Vec<ChantingChant> = if ancestor_chant_uids.is_empty() {
            Vec::new()
        } else {
            let uid_list: Vec<String> = ancestor_chant_uids.iter().cloned().collect();
            chant_dsl::chanting_chants
                .filter(chant_dsl::uid.eq_any(&uid_list))
                .select(ChantingChant::as_select())
                .load(db_conn)
                .context("Failed to load ancestor chanting chants for export")?
        };

        // 1.3: collect distinct collection_uids from the ancestor chants (plus
        // user-added chants) and load matching collections regardless of
        // is_user_added.
        let ancestor_collection_uids: std::collections::BTreeSet<String> = ancestor_chants
            .iter()
            .chain(user_chants.iter())
            .map(|c| c.collection_uid.clone())
            .collect();

        let ancestor_collections: Vec<ChantingCollection> = if ancestor_collection_uids.is_empty() {
            Vec::new()
        } else {
            let uid_list: Vec<String> = ancestor_collection_uids.iter().cloned().collect();
            col_dsl::chanting_collections
                .filter(col_dsl::uid.eq_any(&uid_list))
                .select(ChantingCollection::as_select())
                .load(db_conn)
                .context("Failed to load ancestor chanting collections for export")?
        };

        // 1.4: merge ancestor rows with the user_* vectors, deduplicating by
        // uid. Keep each row's original is_user_added flag (seeded ancestors
        // remain false). The user_* rows take precedence when a uid collides.
        let user_collection_uids: std::collections::HashSet<String> =
            user_collections.iter().map(|c| c.uid.clone()).collect();
        let user_chant_uids: std::collections::HashSet<String> =
            user_chants.iter().map(|c| c.uid.clone()).collect();
        let user_section_uids: std::collections::HashSet<String> =
            user_sections.iter().map(|s| s.uid.clone()).collect();

        let seeded_collection_count = ancestor_collections
            .iter()
            .filter(|c| !user_collection_uids.contains(&c.uid))
            .count();
        let seeded_chant_count = ancestor_chants
            .iter()
            .filter(|c| !user_chant_uids.contains(&c.uid))
            .count();
        let seeded_section_count = ancestor_sections
            .iter()
            .filter(|s| !user_section_uids.contains(&s.uid))
            .count();

        let mut export_collections: Vec<ChantingCollection> = user_collections.clone();
        export_collections.extend(
            ancestor_collections
                .into_iter()
                .filter(|c| !user_collection_uids.contains(&c.uid)),
        );

        let mut export_chants: Vec<ChantingChant> = user_chants.clone();
        export_chants.extend(
            ancestor_chants
                .into_iter()
                .filter(|c| !user_chant_uids.contains(&c.uid)),
        );

        let mut export_sections: Vec<ChantingSection> = user_sections.clone();
        export_sections.extend(
            ancestor_sections
                .into_iter()
                .filter(|s| !user_section_uids.contains(&s.uid)),
        );

        // 6.5.1: relaxed early-return — a user with user-added chants or
        // sections but no user-added collections/recordings must still have
        // their data exported.
        if user_collections.is_empty()
            && user_chants.is_empty()
            && user_sections.is_empty()
            && user_recordings.is_empty()
        {
            info("No user chanting data or recordings to export");
            return Ok(());
        }

        // 6.5.3 / PRD §11.5: synthesise placeholders for any ancestor uid
        // referenced by a user-added row but missing from the live DB.
        // Without this step the exported sqlite would have dangling FKs and
        // the import side would have to drop rows.
        let have_collection_uids: std::collections::HashSet<String> =
            export_collections.iter().map(|c| c.uid.clone()).collect();
        let have_chant_uids: std::collections::HashSet<String> =
            export_chants.iter().map(|c| c.uid.clone()).collect();
        let have_section_uids: std::collections::HashSet<String> =
            export_sections.iter().map(|s| s.uid.clone()).collect();

        let missing_collection_uids: Vec<String> = ancestor_collection_uids
            .iter()
            .filter(|u| !have_collection_uids.contains(*u))
            .cloned()
            .collect();
        let missing_chant_uids: Vec<String> = ancestor_chant_uids
            .iter()
            .filter(|u| !have_chant_uids.contains(*u))
            .cloned()
            .collect();
        let missing_section_uids: Vec<String> = recording_section_uids
            .iter()
            .filter(|u| !have_section_uids.contains(*u))
            .cloned()
            .collect();

        let mut synthesised_ancestors = 0usize;
        let mut need_recovery_collection = false;
        let mut need_recovery_chant = false;

        for missing_uid in &missing_collection_uids {
            warn(&format!(
                "export_user_chanting_data(): synthesising placeholder collection for missing ancestor uid {}",
                missing_uid
            ));
            need_recovery_collection = true;
            synthesised_ancestors += 1;
        }
        for missing_uid in &missing_chant_uids {
            warn(&format!(
                "export_user_chanting_data(): synthesising placeholder chant for missing ancestor uid {}",
                missing_uid
            ));
            need_recovery_chant = true;
            need_recovery_collection = true;
            synthesised_ancestors += 1;
        }
        for missing_uid in &missing_section_uids {
            warn(&format!(
                "export_user_chanting_data(): synthesising placeholder section for missing section uid {} referenced by user recording(s)",
                missing_uid
            ));
            need_recovery_chant = true;
            need_recovery_collection = true;
            synthesised_ancestors += 1;
        }

        if need_recovery_collection
            && !have_collection_uids.contains(crate::db::chanting_export::ORPHAN_RECOVERY_COLLECTION_UID)
        {
            export_collections.push(crate::db::chanting_export::make_orphan_recovery_collection());
        }
        if need_recovery_chant
            && !have_chant_uids.contains(crate::db::chanting_export::ORPHAN_RECOVERY_CHANT_UID)
        {
            export_chants.push(crate::db::chanting_export::make_orphan_recovery_chant());
        }
        // Missing sections become placeholder sections attached to the
        // recovery chant — their uids are preserved so the recording FK
        // continues to resolve.
        for missing_uid in &missing_section_uids {
            export_sections.push(crate::db::chanting_export::make_orphan_recovery_section(
                missing_uid,
                crate::db::chanting_export::ORPHAN_RECOVERY_CHANT_UID,
            ));
        }
        // Missing chants: we don't know which sections belong to them, so a
        // generic recovery-chant placeholder is used and sections whose
        // parent chant was missing will be re-pointed in the import step.
        // We only need to guarantee the chant_uid exists in the export file;
        // for chants referenced by a user_section whose chant_uid is missing,
        // fix up the user_section's chant_uid on-the-fly when writing.
        if !missing_chant_uids.is_empty() {
            // Ensure any section whose chant_uid is in missing_chant_uids
            // is re-pointed at the recovery chant so the exported sqlite is
            // FK-consistent.
            for sec in export_sections.iter_mut() {
                if missing_chant_uids.contains(&sec.chant_uid) {
                    sec.chant_uid =
                        crate::db::chanting_export::ORPHAN_RECOVERY_CHANT_UID.to_string();
                }
            }
        }
        // Same trick for chants whose collection_uid is missing.
        if !missing_collection_uids.is_empty() {
            for chant in export_chants.iter_mut() {
                if missing_collection_uids.contains(&chant.collection_uid) {
                    chant.collection_uid =
                        crate::db::chanting_export::ORPHAN_RECOVERY_COLLECTION_UID.to_string();
                }
            }
        }

        // 1.5 / 6.5.4: per-table counts + any synthesised orphan placeholders.
        info(&format!(
            "Exporting chanting data: collections user_added={} seeded_ancestors={} total={}; \
             chants user_added={} seeded_ancestors={} total={}; \
             sections user_added={} seeded_ancestors={} total={}; \
             recordings user_added={} total={}; orphan_placeholders_synthesised={}",
            user_collections.len(), seeded_collection_count, export_collections.len(),
            user_chants.len(), seeded_chant_count, export_chants.len(),
            user_sections.len(), seeded_section_count, export_sections.len(),
            user_recordings.len(), user_recordings.len(),
            synthesised_ancestors
        ));

        // 1.7: copy audio files BEFORE building the sqlite file, so that a
        // failure during file copy does not leave an orphaned sqlite index
        // referencing missing audio.
        let recordings_src = crate::get_chanting_recordings_dir();
        let recordings_dest = import_dir.join("chanting-recordings");

        match recordings_src.try_exists() {
            Ok(true) => {
                std::fs::create_dir_all(&recordings_dest)
                    .context("Failed to create chanting-recordings dir in import-me")?;

                for rec in &user_recordings {
                    let src_file = recordings_src.join(&rec.file_name);
                    match src_file.try_exists() {
                        Ok(true) => {
                            let dest_file = recordings_dest.join(&rec.file_name);
                            if let Err(e) = std::fs::copy(&src_file, &dest_file) {
                                warn(&format!(
                                    "Failed to copy recording file {}: {}",
                                    src_file.display(), e
                                ));
                            }
                        }
                        _ => {
                            warn(&format!(
                                "Recording file missing for user recording {}: {}",
                                rec.uid, rec.file_name
                            ));
                        }
                    }
                }

                info(&format!("Copied {} user chanting recordings to {}", user_recordings.len(), recordings_dest.display()));
            }
            _ => {
                info("No chanting-recordings directory to copy");
            }
        }

        // Now build the sqlite file from the merged rows.
        let sqlite_path = import_dir.join("appdata-chanting.sqlite3");
        create_chanting_sqlite(
            &sqlite_path,
            &export_collections,
            &export_chants,
            &export_sections,
            &user_recordings,
        )?;

        info("export_user_chanting_data(): completed");
        Ok(())
    }

    /// Import user chanting data and recordings from the import-me folder.
    ///
    /// Reads `appdata-chanting.sqlite3`, inserts user-added items with original UIDs preserved
    /// (since the target DB is fresh after upgrade), and copies audio files back.
    pub fn import_user_chanting_data(&self, import_dir: &Path) -> Result<()> {
        use crate::db::chanting_export::read_chanting_from_sqlite;

        let sqlite_path = import_dir.join("appdata-chanting.sqlite3");
        match sqlite_path.try_exists() {
            Ok(true) => {}
            _ => {
                info("No appdata-chanting.sqlite3 found in import-me folder");
                return Ok(());
            }
        }

        info(&format!("Importing user chanting data from {}", sqlite_path.display()));

        let (collections, chants, sections, recordings) =
            read_chanting_from_sqlite(&sqlite_path)?;

        info(&format!(
            "Found chanting data: {} collections, {} chants, {} sections, {} recordings",
            collections.len(), chants.len(), sections.len(), recordings.len()
        ));

        // 2.1: per-table counters.
        let mut col_inserted: usize = 0;
        let mut col_skipped: usize = 0;
        let mut chant_inserted: usize = 0;
        let mut chant_skipped: usize = 0;
        let mut sec_inserted: usize = 0;
        let mut sec_skipped: usize = 0;
        let mut rec_inserted: usize = 0;
        let mut rec_skipped: usize = 0;
        let mut rec_orphan_parents_created: usize = 0;

        // Build lookup maps over the exported rows so that we can synthesize
        // missing ancestors when a recording's section (or its chant /
        // collection) is absent from the live DB.
        let exported_sections: std::collections::HashMap<String, &ChantingSection> =
            sections.iter().map(|s| (s.uid.clone(), s)).collect();
        let exported_chants: std::collections::HashMap<String, &ChantingChant> =
            chants.iter().map(|c| (c.uid.clone(), c)).collect();
        let exported_collections: std::collections::HashMap<String, &ChantingCollection> =
            collections.iter().map(|c| (c.uid.clone(), c)).collect();

        // 2.2: collections — skip if the uid already exists in the live DB.
        for col in &collections {
            match self.dbm.appdata.chanting_collection_exists_by_uid(&col.uid) {
                Ok(true) => {
                    info(&format!("skipped existing seeded collection {}", col.uid));
                    col_skipped += 1;
                }
                Ok(false) => {
                    let data = ChantingCollectionJson {
                        uid: col.uid.clone(),
                        title: col.title.clone(),
                        description: col.description.clone(),
                        language: col.language.clone(),
                        sort_index: col.sort_index,
                        is_user_added: col.is_user_added,
                        metadata_json: col.metadata_json.clone(),
                        chants: Vec::new(),
                    };
                    match self.dbm.appdata.create_chanting_collection(&data) {
                        Ok(_) => col_inserted += 1,
                        Err(e) => warn(&format!("Failed to import collection {}: {}", col.uid, e)),
                    }
                }
                Err(e) => {
                    warn(&format!(
                        "Existence check failed for collection {}: {}",
                        col.uid, e
                    ));
                }
            }
        }

        // 2.3: chants — skip if uid already exists.
        for chant in &chants {
            match self.dbm.appdata.chanting_chant_exists_by_uid(&chant.uid) {
                Ok(true) => {
                    info(&format!("skipped existing seeded chant {}", chant.uid));
                    chant_skipped += 1;
                }
                Ok(false) => {
                    let data = ChantingChantJson {
                        uid: chant.uid.clone(),
                        collection_uid: chant.collection_uid.clone(),
                        title: chant.title.clone(),
                        description: chant.description.clone(),
                        sort_index: chant.sort_index,
                        is_user_added: chant.is_user_added,
                        metadata_json: chant.metadata_json.clone(),
                        sections: Vec::new(),
                    };
                    match self.dbm.appdata.create_chanting_chant(&data) {
                        Ok(_) => chant_inserted += 1,
                        Err(e) => warn(&format!("Failed to import chant {}: {}", chant.uid, e)),
                    }
                }
                Err(e) => {
                    warn(&format!(
                        "Existence check failed for chant {}: {}",
                        chant.uid, e
                    ));
                }
            }
        }

        // 2.4: sections — skip if uid already exists.
        for sec in &sections {
            match self.dbm.appdata.chanting_section_exists_by_uid(&sec.uid) {
                Ok(true) => {
                    info(&format!("skipped existing seeded section {}", sec.uid));
                    sec_skipped += 1;
                }
                Ok(false) => {
                    let data = ChantingSectionJson {
                        uid: sec.uid.clone(),
                        chant_uid: sec.chant_uid.clone(),
                        title: sec.title.clone(),
                        content_pali: sec.content_pali.clone(),
                        sort_index: sec.sort_index,
                        is_user_added: sec.is_user_added,
                        metadata_json: sec.metadata_json.clone(),
                        recordings: Vec::new(),
                    };
                    match self.dbm.appdata.create_chanting_section(&data) {
                        Ok(_) => sec_inserted += 1,
                        Err(e) => warn(&format!("Failed to import section {}: {}", sec.uid, e)),
                    }
                }
                Err(e) => {
                    warn(&format!(
                        "Existence check failed for section {}: {}",
                        sec.uid, e
                    ));
                }
            }
        }

        // 2.5: recordings — skip duplicates; create missing parent ancestors
        // from the exported metadata so no recording is ever dropped.
        for rec in &recordings {
            match self.dbm.appdata.chanting_recording_exists_by_uid(&rec.uid) {
                Ok(true) => {
                    info(&format!("skipped existing recording {}", rec.uid));
                    rec_skipped += 1;
                    continue;
                }
                Ok(false) => {}
                Err(e) => {
                    warn(&format!(
                        "Existence check failed for recording {}: {}",
                        rec.uid, e
                    ));
                    continue;
                }
            }

            // Ensure the recording's parent section exists in the live DB,
            // creating any missing collection/chant/section ancestors first.
            // Prefer exported metadata, then fall back to synthetic
            // placeholders so no recording is ever dropped (PRD §10.2 /
            // §11.4).
            self.ensure_recording_ancestors(
                rec,
                &exported_sections,
                &exported_chants,
                &exported_collections,
                &mut rec_orphan_parents_created,
            );

            let data = ChantingRecordingJson {
                uid: rec.uid.clone(),
                section_uid: rec.section_uid.clone(),
                file_name: rec.file_name.clone(),
                recording_type: rec.recording_type.clone(),
                label: rec.label.clone(),
                duration_ms: rec.duration_ms,
                markers_json: rec.markers_json.clone(),
                volume: rec.volume,
                playback_position_ms: rec.playback_position_ms,
                waveform_json: rec.waveform_json.clone(),
                is_user_added: rec.is_user_added,
            };
            match self.dbm.appdata.create_chanting_recording(&data) {
                Ok(_) => rec_inserted += 1,
                Err(e) => warn(&format!("Failed to import recording {}: {}", rec.uid, e)),
            }
        }

        // 2.7: final per-table summary.
        info(&format!(
            "Chanting import summary: \
             collections inserted={} skipped_existing={}; \
             chants inserted={} skipped_existing={}; \
             sections inserted={} skipped_existing={}; \
             recordings inserted={} skipped_existing={} parents_created_for_orphans={}",
            col_inserted, col_skipped,
            chant_inserted, chant_skipped,
            sec_inserted, sec_skipped,
            rec_inserted, rec_skipped, rec_orphan_parents_created
        ));

        // Copy audio files from import-me/chanting-recordings/ back to the app's recordings dir
        let recordings_src = import_dir.join("chanting-recordings");
        match recordings_src.try_exists() {
            Ok(true) => {
                let recordings_dest = crate::get_chanting_recordings_dir();

                if let Ok(entries) = std::fs::read_dir(&recordings_src) {
                    for entry in entries.flatten() {
                        let src_file = entry.path();
                        if src_file.is_file() {
                            let dest_file = recordings_dest.join(entry.file_name());
                            if let Err(e) = std::fs::copy(&src_file, &dest_file) {
                                warn(&format!(
                                    "Failed to copy recording file {}: {}",
                                    src_file.display(), e
                                ));
                            }
                        }
                    }
                }

                info(&format!("Copied chanting recordings from {}", recordings_src.display()));
            }
            _ => {
                info("No chanting-recordings directory found in import-me");
            }
        }

        info("import_user_chanting_data(): completed");
        Ok(())
    }

    /// Ensure the parent-collection/chant/section chain of a recording
    /// exists in the live appdata DB.
    ///
    /// PRD §10.2 / §11.4 require that no recording is ever dropped during
    /// import. When the exported DB lacks a needed ancestor, a deterministic
    /// synthetic placeholder (`col-orphan-recovery` / `chant-orphan-recovery`
    /// / a section whose uid is the recording's original `section_uid`) is
    /// created so the recording's FK still resolves and its audio stays
    /// linked to a visible section in the new DB.
    fn ensure_recording_ancestors(
        &self,
        rec: &ChantingRecording,
        exported_sections: &std::collections::HashMap<String, &ChantingSection>,
        exported_chants: &std::collections::HashMap<String, &ChantingChant>,
        exported_collections: &std::collections::HashMap<String, &ChantingCollection>,
        counter: &mut usize,
    ) {
        use crate::db::chanting_export::{
            make_orphan_recovery_chant, make_orphan_recovery_collection,
            make_orphan_recovery_section, ORPHAN_RECOVERY_CHANT_UID,
            ORPHAN_RECOVERY_COLLECTION_UID,
        };

        // Fast path: parent section already present.
        if self
            .dbm
            .appdata
            .chanting_section_exists_by_uid(&rec.section_uid)
            .unwrap_or(false)
        {
            return;
        }

        // Pick the chant/collection we will attach the (possibly-missing)
        // parent section to. Prefer exported metadata when available;
        // otherwise fall back to the synthetic recovery placeholders.
        let (chant_uid, col_uid, sec_from_export) =
            match exported_sections.get(&rec.section_uid) {
                Some(sec) => {
                    let chant_uid = sec.chant_uid.clone();
                    let col_uid = match exported_chants.get(&chant_uid) {
                        Some(c) => c.collection_uid.clone(),
                        None => ORPHAN_RECOVERY_COLLECTION_UID.to_string(),
                    };
                    (chant_uid, col_uid, Some(*sec))
                }
                None => (
                    ORPHAN_RECOVERY_CHANT_UID.to_string(),
                    ORPHAN_RECOVERY_COLLECTION_UID.to_string(),
                    None,
                ),
            };

        // --- Ensure collection ---
        if !self
            .dbm
            .appdata
            .chanting_collection_exists_by_uid(&col_uid)
            .unwrap_or(false)
        {
            let col_src = exported_collections.get(&col_uid).copied();
            let col_owned: ChantingCollection = match col_src {
                Some(c) => c.clone(),
                None => make_orphan_recovery_collection(),
            };
            let data = ChantingCollectionJson {
                uid: col_owned.uid.clone(),
                title: col_owned.title.clone(),
                description: col_owned.description.clone(),
                language: col_owned.language.clone(),
                sort_index: col_owned.sort_index,
                is_user_added: col_owned.is_user_added,
                metadata_json: col_owned.metadata_json.clone(),
                chants: Vec::new(),
            };
            match self.dbm.appdata.create_chanting_collection(&data) {
                Ok(()) => {
                    warn(&format!(
                        "Created missing parent collection {} for orphan recording {} (source: {})",
                        col_owned.uid,
                        rec.uid,
                        if col_src.is_some() { "exported DB" } else { "synthetic placeholder" }
                    ));
                    *counter += 1;
                }
                Err(e) => {
                    warn(&format!(
                        "Failed to create parent collection {} for orphan recording {}: {}",
                        col_owned.uid, rec.uid, e
                    ));
                }
            }
        }

        // --- Ensure chant ---
        if !self
            .dbm
            .appdata
            .chanting_chant_exists_by_uid(&chant_uid)
            .unwrap_or(false)
        {
            let chant_src = exported_chants.get(&chant_uid).copied();
            let mut chant_owned: ChantingChant = match chant_src {
                Some(c) => c.clone(),
                None => make_orphan_recovery_chant(),
            };
            // Re-point chant to the collection we just ensured exists, in
            // case the exported chant referred to a collection uid that
            // neither the live DB nor the exported DB had.
            chant_owned.collection_uid = col_uid.clone();
            let data = ChantingChantJson {
                uid: chant_owned.uid.clone(),
                collection_uid: chant_owned.collection_uid.clone(),
                title: chant_owned.title.clone(),
                description: chant_owned.description.clone(),
                sort_index: chant_owned.sort_index,
                is_user_added: chant_owned.is_user_added,
                metadata_json: chant_owned.metadata_json.clone(),
                sections: Vec::new(),
            };
            match self.dbm.appdata.create_chanting_chant(&data) {
                Ok(()) => {
                    warn(&format!(
                        "Created missing parent chant {} for orphan recording {} (source: {})",
                        chant_owned.uid,
                        rec.uid,
                        if chant_src.is_some() { "exported DB" } else { "synthetic placeholder" }
                    ));
                    *counter += 1;
                }
                Err(e) => {
                    warn(&format!(
                        "Failed to create parent chant {} for orphan recording {}: {}",
                        chant_owned.uid, rec.uid, e
                    ));
                }
            }
        }

        // --- Ensure section (uid == rec.section_uid so FK resolves) ---
        let mut sec_owned: ChantingSection = match sec_from_export {
            Some(sec) => sec.clone(),
            None => make_orphan_recovery_section(&rec.section_uid, &chant_uid),
        };
        sec_owned.chant_uid = chant_uid.clone();
        let data = ChantingSectionJson {
            uid: sec_owned.uid.clone(),
            chant_uid: sec_owned.chant_uid.clone(),
            title: sec_owned.title.clone(),
            content_pali: sec_owned.content_pali.clone(),
            sort_index: sec_owned.sort_index,
            is_user_added: sec_owned.is_user_added,
            metadata_json: sec_owned.metadata_json.clone(),
            recordings: Vec::new(),
        };
        match self.dbm.appdata.create_chanting_section(&data) {
            Ok(()) => {
                warn(&format!(
                    "Created missing parent section {} for orphan recording {} (source: {})",
                    sec_owned.uid,
                    rec.uid,
                    if sec_from_export.is_some() { "exported DB" } else { "synthetic placeholder" }
                ));
                *counter += 1;
            }
            Err(e) => {
                warn(&format!(
                    "Failed to create parent section {} for orphan recording {}: {}",
                    sec_owned.uid, rec.uid, e
                ));
            }
        }
    }
}

impl AppData {
    /// Persist the in-memory AppSettings cache back to the appdata DB.
    fn persist_app_settings(&self, settings: &AppSettings) {
        use crate::db::appdata_schema::app_settings::dsl::*;
        let settings_json = serde_json::to_string(settings).expect("Can't encode JSON");
        let db_conn = &mut self.dbm.appdata.get_conn().expect("Can't get db conn");
        let existing = app_settings
            .filter(key.eq("app_settings"))
            .first::<AppSetting>(db_conn)
            .optional();
        let res = match existing {
            Ok(Some(setting)) => diesel::update(app_settings.find(setting.id))
                .set(value.eq(Some(settings_json.as_str())))
                .execute(db_conn),
            Ok(None) => diesel::insert_into(app_settings)
                .values(NewAppSetting {
                    key: "app_settings",
                    value: Some(settings_json.as_str()),
                })
                .execute(db_conn),
            Err(e) => {
                error(&format!("Failed to read app settings row: {}", e));
                return;
            }
        };
        if let Err(e) = res {
            error(&format!("Failed to update app settings: {}", e));
        }
    }

    /// Snapshot of the in-memory global hotkeys config (a sub-struct of
    /// `AppSettings`, persisted alongside `app_keybindings`).
    pub fn get_global_hotkeys(&self) -> GlobalHotkeysConfig {
        self.app_settings_cache
            .read()
            .expect("Failed to read app settings")
            .global_hotkeys
            .clone()
    }

    pub fn set_global_hotkeys_enabled(&self, enabled: bool) {
        let snapshot = {
            let mut s = self
                .app_settings_cache
                .write()
                .expect("Failed to write app settings");
            s.global_hotkeys.set_enabled(enabled);
            s.clone()
        };
        self.persist_app_settings(&snapshot);
    }

    pub fn set_global_hotkey_binding(&self, action_id: &str, sequence: &str) {
        let snapshot = {
            let mut s = self
                .app_settings_cache
                .write()
                .expect("Failed to write app settings");
            s.global_hotkeys.set_binding(action_id, sequence);
            s.clone()
        };
        self.persist_app_settings(&snapshot);
    }

    pub fn get_dpd_enabled(&self) -> bool {
        self.app_settings_cache.read().expect("Failed to read app settings").dict_search_dpd_enabled
    }

    pub fn set_dpd_enabled(&self, enabled: bool) {
        let snapshot = {
            let mut s = self.app_settings_cache.write().expect("Failed to write app settings");
            s.dict_search_dpd_enabled = enabled;
            s.clone()
        };
        self.persist_app_settings(&snapshot);
    }

    pub fn get_commentary_definitions_enabled(&self) -> bool {
        self.app_settings_cache.read().expect("Failed to read app settings").include_comm_bold_definitions_in_search_results
    }

    pub fn set_commentary_definitions_enabled(&self, enabled: bool) {
        let snapshot = {
            let mut s = self.app_settings_cache.write().expect("Failed to write app settings");
            s.include_comm_bold_definitions_in_search_results = enabled;
            s.clone()
        };
        self.persist_app_settings(&snapshot);
    }

    pub fn get_dict_enabled(&self, label: &str) -> bool {
        let s = self.app_settings_cache.read().expect("Failed to read app settings");
        s.dict_search_dict_enabled.get(label).copied().unwrap_or(true)
    }

    pub fn set_dict_enabled(&self, label: &str, enabled: bool) {
        let snapshot = {
            let mut s = self.app_settings_cache.write().expect("Failed to write app settings");
            s.dict_search_dict_enabled.insert(label.to_string(), enabled);
            s.clone()
        };
        self.persist_app_settings(&snapshot);
    }

    pub fn list_dict_enabled(&self) -> IndexMap<String, bool> {
        self.app_settings_cache.read().expect("Failed to read app settings").dict_search_dict_enabled.clone()
    }

    /// Last-used search mode for the given area (`"Suttas"`, `"Dictionary"`,
    /// `"Library"`). Returns the per-area default when unset: `"Combined"`
    /// for Dictionary, `"Fulltext Match"` for Suttas/Library.
    pub fn get_last_search_mode(&self, area: &str) -> String {
        let s = self.app_settings_cache.read().expect("Failed to read app settings");
        if let Some(v) = s.search_last_mode.get(area) {
            return v.clone();
        }
        match area {
            "Dictionary" => "Combined".to_string(),
            _ => "Fulltext Match".to_string(),
        }
    }

    pub fn set_last_search_mode(&self, area: &str, mode: &str) {
        {
            let mut s = self.app_settings_cache.write().expect("Failed to write app settings");
            s.search_last_mode.insert(area.to_string(), mode.to_string());
        }
        // Persist to disk off the calling (UI) thread. The in-memory cache is
        // already updated, so any subsequent get_last_search_mode reads the
        // new value immediately.
        std::thread::spawn(|| {
            let app_data = crate::get_app_data();
            let snapshot = app_data
                .app_settings_cache
                .read()
                .expect("Failed to read app settings")
                .clone();
            app_data.persist_app_settings(&snapshot);
        });
    }

    /// Export user-imported dictionaries to a self-contained SQLite snapshot
    /// at `import_dir/user_dictionaries.sqlite3` (PRD §4.8 task 5.1).
    ///
    /// `indexed_at` is intentionally cleared on the exported rows so the
    /// reconciliation pass re-indexes them on the next startup. `description`
    /// IS carried over.
    ///
    /// **Double-upgrade safety (task 5.6):** if a snapshot already exists in
    /// `import_dir`, this function refuses to overwrite it and returns an
    /// error so the user knows to restart and complete the previous upgrade
    /// first. The caller (`export_user_data_to_assets`) treats this as a
    /// per-category error.
    pub fn export_user_dictionaries(&self, import_dir: &Path) -> Result<()> {
        use crate::db::dictionaries_schema::{dictionaries, dict_words};
        use crate::db::dictionaries_models::{Dictionary, DictWord};
        use crate::db::run_dictionaries_migrations;
        use diesel::sqlite::SqliteConnection;

        let dest_path = import_dir.join("user_dictionaries.sqlite3");

        // Task 5.6: refuse to overwrite an existing snapshot.
        if matches!(dest_path.try_exists(), Ok(true)) {
            return Err(anyhow!(
                "A previous dictionary upgrade hasn't completed; please restart the app to finish it before upgrading again. (Pending: {})",
                dest_path.display()
            ));
        }

        // Pull user-imported dictionaries + their dict_words from the live DB.
        let user_dicts: Vec<Dictionary> = {
            let db_conn = &mut self.dbm.dictionaries.get_conn()
                .context("Failed to get dictionaries DB connection for export")?;
            dictionaries::table
                .filter(dictionaries::is_user_imported.eq(true))
                .select(Dictionary::as_select())
                .load::<Dictionary>(db_conn)
                .context("Failed to load user dictionaries for export")?
        };

        if user_dicts.is_empty() {
            info("export_user_dictionaries(): no user-imported dictionaries to export");
            return Ok(());
        }

        let dict_ids: Vec<i32> = user_dicts.iter().map(|d| d.id).collect();
        let user_words: Vec<DictWord> = {
            let db_conn = &mut self.dbm.dictionaries.get_conn()
                .context("Failed to get dictionaries DB connection for dict_words export")?;
            dict_words::table
                .filter(dict_words::dictionary_id.eq_any(&dict_ids))
                .select(DictWord::as_select())
                .load::<DictWord>(db_conn)
                .context("Failed to load dict_words for export")?
        };

        info(&format!(
            "export_user_dictionaries(): exporting {} dictionaries / {} words",
            user_dicts.len(), user_words.len()
        ));

        // Create the snapshot DB and run dictionaries migrations on it.
        let dest_url = format!("sqlite://{}", dest_path.display());
        let mut snap_conn = SqliteConnection::establish(&dest_url)
            .with_context(|| format!("Failed to create snapshot DB {}", dest_path.display()))?;
        run_dictionaries_migrations(&mut snap_conn)
            .context("Failed to run migrations on user_dictionaries.sqlite3")?;

        // Insert dictionaries (preserving id) so dict_words.dictionary_id
        // continues to resolve inside the snapshot. The importer side will
        // re-key on the way back into the live DB.
        snap_conn.transaction::<_, anyhow::Error, _>(|tx| {
            for d in &user_dicts {
                diesel::insert_into(dictionaries::table)
                    .values((
                        dictionaries::id.eq(d.id),
                        dictionaries::label.eq(&d.label),
                        dictionaries::title.eq(&d.title),
                        dictionaries::dict_type.eq(&d.dict_type),
                        dictionaries::creator.eq(d.creator.as_deref()),
                        dictionaries::description.eq(d.description.as_deref()),
                        dictionaries::feedback_email.eq(d.feedback_email.as_deref()),
                        dictionaries::feedback_url.eq(d.feedback_url.as_deref()),
                        dictionaries::version.eq(d.version.as_deref()),
                        dictionaries::is_user_imported.eq(true),
                        dictionaries::language.eq(d.language.as_deref()),
                        // Task 5.1: indexed_at is intentionally NOT carried over.
                        dictionaries::indexed_at.eq::<Option<chrono::NaiveDateTime>>(None),
                    ))
                    .execute(tx)
                    .with_context(|| format!("Insert dictionary {} into snapshot failed", d.label))?;
            }

            for w in &user_words {
                diesel::insert_into(dict_words::table)
                    .values((
                        dict_words::id.eq(w.id),
                        dict_words::dictionary_id.eq(w.dictionary_id),
                        dict_words::dict_label.eq(&w.dict_label),
                        dict_words::uid.eq(&w.uid),
                        dict_words::word.eq(&w.word),
                        dict_words::word_ascii.eq(&w.word_ascii),
                        dict_words::language.eq(w.language.as_deref()),
                        dict_words::word_nom_sg.eq(w.word_nom_sg.as_deref()),
                        dict_words::inflections.eq(w.inflections.as_deref()),
                        dict_words::phonetic.eq(w.phonetic.as_deref()),
                        dict_words::transliteration.eq(w.transliteration.as_deref()),
                        dict_words::meaning_order.eq(w.meaning_order),
                        dict_words::definition_plain.eq(w.definition_plain.as_deref()),
                        dict_words::definition_html.eq(w.definition_html.as_deref()),
                        dict_words::summary.eq(w.summary.as_deref()),
                        dict_words::synonyms.eq(w.synonyms.as_deref()),
                        dict_words::antonyms.eq(w.antonyms.as_deref()),
                        dict_words::homonyms.eq(w.homonyms.as_deref()),
                        dict_words::also_written_as.eq(w.also_written_as.as_deref()),
                        dict_words::see_also.eq(w.see_also.as_deref()),
                    ))
                    .execute(tx)
                    .with_context(|| format!("Insert dict_word {} into snapshot failed", w.uid))?;
            }
            Ok(())
        })?;

        info(&format!("export_user_dictionaries(): wrote snapshot to {}", dest_path.display()));
        Ok(())
    }

    /// Import user-imported dictionaries from `import_dir/user_dictionaries.sqlite3`
    /// into the live `dictionaries.sqlite3` (PRD task 5.3).
    ///
    /// Each `dictionaries` row is INSERTed into the live DB with a fresh
    /// autoincrement id; the old→new id map is then used to rewrite
    /// `dict_words.dictionary_id` on insert. `indexed_at` is left NULL so the
    /// reconciliation pass re-indexes on the next startup.
    ///
    /// All inserts run in a single transaction. On failure, the snapshot is
    /// left in place for the next startup to retry.
    ///
    /// On success, the snapshot file is removed.
    ///
    /// Schema-drift safety (task 5.5): the importer reads explicit columns
    /// via Diesel's typed schema. If a future build adds a `dict_words`
    /// column with a NULL default the existing snapshots remain importable
    /// because they will simply lack that column (and Diesel will reject
    /// only if the new column is NOT NULL without default). If a future
    /// build removes a column, the importer must be updated alongside.
    pub fn import_user_dictionaries(&self, import_dir: &Path) -> Result<()> {
        use crate::db::dictionaries_schema::{dictionaries, dict_words};
        use crate::db::dictionaries_models::{Dictionary, DictWord};
        use diesel::sqlite::SqliteConnection;
        use std::collections::HashMap;

        let snapshot_path = import_dir.join("user_dictionaries.sqlite3");
        if !matches!(snapshot_path.try_exists(), Ok(true)) {
            info("import_user_dictionaries(): no user_dictionaries.sqlite3 snapshot, skipping");
            return Ok(());
        }

        info(&format!(
            "import_user_dictionaries(): consuming snapshot {}",
            snapshot_path.display()
        ));

        let snap_url = format!("sqlite://{}", snapshot_path.display());
        let mut snap_conn = SqliteConnection::establish(&snap_url)
            .with_context(|| format!("Failed to open snapshot {}", snapshot_path.display()))?;

        let snap_dicts: Vec<Dictionary> = dictionaries::table
            .select(Dictionary::as_select())
            .load::<Dictionary>(&mut snap_conn)
            .context("Failed to read dictionaries from snapshot")?;

        if snap_dicts.is_empty() {
            info("import_user_dictionaries(): snapshot has no dictionaries, removing");
            let _ = std::fs::remove_file(&snapshot_path);
            return Ok(());
        }

        let snap_dict_ids: Vec<i32> = snap_dicts.iter().map(|d| d.id).collect();
        let snap_words: Vec<DictWord> = dict_words::table
            .filter(dict_words::dictionary_id.eq_any(&snap_dict_ids))
            .select(DictWord::as_select())
            .load::<DictWord>(&mut snap_conn)
            .context("Failed to read dict_words from snapshot")?;

        info(&format!(
            "import_user_dictionaries(): re-keying {} dictionaries / {} words",
            snap_dicts.len(), snap_words.len()
        ));

        // Insert into the live DB inside a single transaction.
        self.dbm.dictionaries.do_write(|tx| {
            tx.transaction::<_, diesel::result::Error, _>(|tx| {
                let mut id_map: HashMap<i32, i32> = HashMap::with_capacity(snap_dicts.len());
                for d in &snap_dicts {
                    // Skip if a dictionary with this label already exists in the
                    // live DB (e.g. if the user re-installed the same archive
                    // before the upgrade). Defensive — should be rare.
                    let existing: Option<i32> = dictionaries::table
                        .filter(dictionaries::label.eq(&d.label))
                        .select(dictionaries::id)
                        .first(tx)
                        .optional()?;
                    if let Some(eid) = existing {
                        warn(&format!(
                            "import_user_dictionaries(): live DB already has dictionary '{}' (id {}); skipping snapshot row",
                            d.label, eid
                        ));
                        id_map.insert(d.id, eid);
                        continue;
                    }

                    diesel::insert_into(dictionaries::table)
                        .values((
                            dictionaries::label.eq(&d.label),
                            dictionaries::title.eq(&d.title),
                            dictionaries::dict_type.eq(&d.dict_type),
                            dictionaries::creator.eq(d.creator.as_deref()),
                            dictionaries::description.eq(d.description.as_deref()),
                            dictionaries::feedback_email.eq(d.feedback_email.as_deref()),
                            dictionaries::feedback_url.eq(d.feedback_url.as_deref()),
                            dictionaries::version.eq(d.version.as_deref()),
                            dictionaries::is_user_imported.eq(true),
                            dictionaries::language.eq(d.language.as_deref()),
                            // Force NULL so the reconciliation pass re-indexes.
                            dictionaries::indexed_at.eq::<Option<chrono::NaiveDateTime>>(None),
                        ))
                        .execute(tx)?;

                    let new_id: i32 = dictionaries::table
                        .filter(dictionaries::label.eq(&d.label))
                        .select(dictionaries::id)
                        .first(tx)?;
                    id_map.insert(d.id, new_id);
                }

                for w in &snap_words {
                    let new_dict_id = match id_map.get(&w.dictionary_id) {
                        Some(nid) => *nid,
                        None => {
                            warn(&format!(
                                "import_user_dictionaries(): dict_word {} references missing dictionary_id {} in snapshot; skipping",
                                w.uid, w.dictionary_id
                            ));
                            continue;
                        }
                    };

                    diesel::insert_into(dict_words::table)
                        .values((
                            dict_words::dictionary_id.eq(new_dict_id),
                            dict_words::dict_label.eq(&w.dict_label),
                            dict_words::uid.eq(&w.uid),
                            dict_words::word.eq(&w.word),
                            dict_words::word_ascii.eq(&w.word_ascii),
                            dict_words::language.eq(w.language.as_deref()),
                            dict_words::word_nom_sg.eq(w.word_nom_sg.as_deref()),
                            dict_words::inflections.eq(w.inflections.as_deref()),
                            dict_words::phonetic.eq(w.phonetic.as_deref()),
                            dict_words::transliteration.eq(w.transliteration.as_deref()),
                            dict_words::meaning_order.eq(w.meaning_order),
                            dict_words::definition_plain.eq(w.definition_plain.as_deref()),
                            dict_words::definition_html.eq(w.definition_html.as_deref()),
                            dict_words::summary.eq(w.summary.as_deref()),
                            dict_words::synonyms.eq(w.synonyms.as_deref()),
                            dict_words::antonyms.eq(w.antonyms.as_deref()),
                            dict_words::homonyms.eq(w.homonyms.as_deref()),
                            dict_words::also_written_as.eq(w.also_written_as.as_deref()),
                            dict_words::see_also.eq(w.see_also.as_deref()),
                        ))
                        .execute(tx)?;
                }

                Ok(())
            })
        }).context("import_user_dictionaries transaction failed")?;

        // Success — remove the snapshot.
        if let Err(e) = std::fs::remove_file(&snapshot_path) {
            warn(&format!(
                "Failed to remove consumed snapshot {}: {}",
                snapshot_path.display(), e
            ));
        } else {
            info(&format!(
                "import_user_dictionaries(): consumed and removed snapshot {}",
                snapshot_path.display()
            ));
        }
        Ok(())
    }
}

impl Default for AppData {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the total system memory in bytes.
///
/// NOTE: Cannot use sysinfo::System because it requires higher Android API levels.
/// Therefore, we use platform-specific implementations.
pub fn get_system_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "android")]
    {
        get_android_memory_bytes()
    }

    #[cfg(target_os = "linux")]
    {
        get_linux_memory_bytes()
    }

    #[cfg(target_os = "macos")]
    {
        get_macos_memory_bytes()
    }

    #[cfg(target_os = "ios")]
    {
        get_ios_memory_bytes()
    }

    #[cfg(target_os = "windows")]
    {
        get_windows_memory_bytes()
    }
}

/// Get the number of CPU cores.
///
/// NOTE: Cannot use sysinfo::System because it requires higher Android API levels.
/// Therefore, we use platform-specific implementations.
pub fn get_cpu_cores() -> Option<u32> {
    #[cfg(any(target_os = "android", target_os = "linux"))]
    {
        get_linux_cpu_cores()
    }

    #[cfg(target_os = "macos")]
    {
        get_macos_cpu_cores()
    }

    #[cfg(target_os = "ios")]
    {
        get_ios_cpu_cores()
    }

    #[cfg(target_os = "windows")]
    {
        get_windows_cpu_cores()
    }
}

/// Get the maximum CPU frequency in MHz.
///
/// NOTE: Cannot use sysinfo::System because it requires higher Android API levels.
/// Therefore, we use platform-specific implementations.
pub fn get_cpu_max_frequency_mhz() -> Option<u64> {
    #[cfg(any(target_os = "android", target_os = "linux"))]
    {
        get_linux_cpu_max_frequency_mhz()
    }

    #[cfg(target_os = "macos")]
    {
        get_macos_cpu_max_frequency_mhz()
    }

    #[cfg(target_os = "ios")]
    {
        get_ios_cpu_max_frequency_mhz()
    }

    #[cfg(target_os = "windows")]
    {
        get_windows_cpu_max_frequency_mhz()
    }
}

#[cfg(target_os = "android")]
fn get_android_memory_bytes() -> Option<u64> {
    use std::fs;

    // Read /proc/meminfo which is available on Android
    let meminfo = fs::read_to_string("/proc/meminfo").ok()?;

    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let kb = parts[1].parse::<u64>().ok()?;
                return Some(kb * 1024); // Convert KB to bytes
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn get_linux_memory_bytes() -> Option<u64> {
    use std::fs;

    let meminfo = fs::read_to_string("/proc/meminfo").ok()?;

    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let kb = parts[1].parse::<u64>().ok()?;
                return Some(kb * 1024); // Convert KB to bytes
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn get_macos_memory_bytes() -> Option<u64> {
    use std::process::Command;

    let output = Command::new("sysctl")
        .arg("-n")
        .arg("hw.memsize")
        .output()
        .ok()?;

    let bytes_str = String::from_utf8(output.stdout).ok()?;
    bytes_str.trim().parse::<u64>().ok()
}

#[cfg(target_os = "windows")]
fn get_windows_memory_bytes() -> Option<u64> {
    use std::mem;

    #[repr(C)]
    struct MEMORYSTATUSEX {
        dw_length: u32,
        dw_memory_load: u32,
        ull_total_phys: u64,
        ull_avail_phys: u64,
        ull_total_page_file: u64,
        ull_avail_page_file: u64,
        ull_total_virtual: u64,
        ull_avail_virtual: u64,
        ull_avail_extended_virtual: u64,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(lpbuffer: *mut MEMORYSTATUSEX) -> i32;
    }

    unsafe {
        let mut status: MEMORYSTATUSEX = mem::zeroed();
        status.dw_length = mem::size_of::<MEMORYSTATUSEX>() as u32;

        if GlobalMemoryStatusEx(&mut status) != 0 {
            Some(status.ull_total_phys)
        } else {
            None
        }
    }
}

// CPU cores implementations

#[cfg(any(target_os = "android", target_os = "linux"))]
fn get_linux_cpu_cores() -> Option<u32> {
    use std::fs;

    // Try reading from /proc/cpuinfo
    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        let count = cpuinfo.lines()
            .filter(|line| line.starts_with("processor"))
            .count();
        if count > 0 {
            return Some(count as u32);
        }
    }

    // Fallback: try reading from /sys/devices/system/cpu/present
    if let Ok(present) = fs::read_to_string("/sys/devices/system/cpu/present") {
        // Format is like "0-7" for 8 cores
        let trimmed = present.trim();
        if let Some(pos) = trimmed.rfind('-')
            && let Ok(max) = trimmed[pos + 1..].parse::<u32>() {
                return Some(max + 1);
            }
    }

    None
}

#[cfg(target_os = "macos")]
fn get_macos_cpu_cores() -> Option<u32> {
    use std::process::Command;

    let output = Command::new("sysctl")
        .arg("-n")
        .arg("hw.ncpu")
        .output()
        .ok()?;

    let cores_str = String::from_utf8(output.stdout).ok()?;
    cores_str.trim().parse::<u32>().ok()
}

#[cfg(target_os = "windows")]
fn get_windows_cpu_cores() -> Option<u32> {
    use std::env;

    // Use NUMBER_OF_PROCESSORS environment variable
    env::var("NUMBER_OF_PROCESSORS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
}

// CPU frequency implementations

#[cfg(any(target_os = "android", target_os = "linux"))]
fn get_linux_cpu_max_frequency_mhz() -> Option<u64> {
    use std::fs;

    // Try reading from scaling_max_freq (in KHz)
    if let Ok(freq_str) = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq")
        && let Ok(khz) = freq_str.trim().parse::<u64>() {
            return Some(khz / 1000); // Convert KHz to MHz
        }

    // Fallback: try cpuinfo_max_freq
    if let Ok(freq_str) = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq")
        && let Ok(khz) = freq_str.trim().parse::<u64>() {
            return Some(khz / 1000);
        }

    // Fallback: try parsing /proc/cpuinfo for "cpu MHz"
    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        for line in cpuinfo.lines() {
            if line.starts_with("cpu MHz")
                && let Some(pos) = line.find(':') {
                    let mhz_str = line[pos + 1..].trim();
                    if let Ok(mhz) = mhz_str.parse::<f64>() {
                        return Some(mhz as u64);
                    }
                }
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn get_macos_cpu_max_frequency_mhz() -> Option<u64> {
    use std::process::Command;

    // sysctl hw.cpufrequency returns frequency in Hz
    let output = Command::new("sysctl")
        .arg("-n")
        .arg("hw.cpufrequency")
        .output()
        .ok()?;

    let hz_str = String::from_utf8(output.stdout).ok()?;
    let hz = hz_str.trim().parse::<u64>().ok()?;
    Some(hz / 1_000_000) // Convert Hz to MHz
}

#[cfg(target_os = "windows")]
fn get_windows_cpu_max_frequency_mhz() -> Option<u64> {
    use std::process::Command;

    // Use WMIC to get max clock speed
    let output = Command::new("wmic")
        .args(["cpu", "get", "MaxClockSpeed", "/value"])
        .output()
        .ok()?;

    let output_str = String::from_utf8(output.stdout).ok()?;
    for line in output_str.lines() {
        if line.starts_with("MaxClockSpeed=") {
            let mhz_str = line.trim_start_matches("MaxClockSpeed=");
            return mhz_str.trim().parse::<u64>().ok();
        }
    }

    None
}

// iOS implementations using sysctlbyname FFI
// Note: iOS uses the same sysctl API as macOS but we can't spawn processes,
// so we use direct FFI calls to sysctlbyname.

#[cfg(target_os = "ios")]
fn get_ios_memory_bytes() -> Option<u64> {
    use std::ffi::CStr;
    use std::mem;
    use std::ptr;

    #[link(name = "System")]
    unsafe extern "C" {
        fn sysctlbyname(
            name: *const i8,
            oldp: *mut std::ffi::c_void,
            oldlenp: *mut usize,
            newp: *mut std::ffi::c_void,
            newlen: usize,
        ) -> i32;
    }

    let name = CStr::from_bytes_with_nul(b"hw.memsize\0").ok()?;
    let mut value: u64 = 0;
    let mut size = mem::size_of::<u64>();

    unsafe {
        let result = sysctlbyname(
            name.as_ptr(),
            &mut value as *mut u64 as *mut std::ffi::c_void,
            &mut size,
            ptr::null_mut(),
            0,
        );

        if result == 0 {
            Some(value)
        } else {
            None
        }
    }
}

#[cfg(target_os = "ios")]
fn get_ios_cpu_cores() -> Option<u32> {
    use std::ffi::CStr;
    use std::mem;
    use std::ptr;

    #[link(name = "System")]
    unsafe extern "C" {
        fn sysctlbyname(
            name: *const i8,
            oldp: *mut std::ffi::c_void,
            oldlenp: *mut usize,
            newp: *mut std::ffi::c_void,
            newlen: usize,
        ) -> i32;
    }

    // Try hw.ncpu first (total CPUs including logical)
    let name = CStr::from_bytes_with_nul(b"hw.ncpu\0").ok()?;
    let mut value: i32 = 0;
    let mut size = mem::size_of::<i32>();

    unsafe {
        let result = sysctlbyname(
            name.as_ptr(),
            &mut value as *mut i32 as *mut std::ffi::c_void,
            &mut size,
            ptr::null_mut(),
            0,
        );

        if result == 0 && value > 0 {
            return Some(value as u32);
        }
    }

    // Fallback to hw.physicalcpu
    let name = CStr::from_bytes_with_nul(b"hw.physicalcpu\0").ok()?;
    let mut value: i32 = 0;
    let mut size = mem::size_of::<i32>();

    unsafe {
        let result = sysctlbyname(
            name.as_ptr(),
            &mut value as *mut i32 as *mut std::ffi::c_void,
            &mut size,
            ptr::null_mut(),
            0,
        );

        if result == 0 && value > 0 {
            Some(value as u32)
        } else {
            None
        }
    }
}

#[cfg(target_os = "ios")]
fn get_ios_cpu_max_frequency_mhz() -> Option<u64> {
    use std::ffi::CStr;
    use std::mem;
    use std::ptr;

    #[link(name = "System")]
    unsafe extern "C" {
        fn sysctlbyname(
            name: *const i8,
            oldp: *mut std::ffi::c_void,
            oldlenp: *mut usize,
            newp: *mut std::ffi::c_void,
            newlen: usize,
        ) -> i32;
    }

    // Try hw.cpufrequency (returns Hz)
    let name = CStr::from_bytes_with_nul(b"hw.cpufrequency\0").ok()?;
    let mut value: u64 = 0;
    let mut size = mem::size_of::<u64>();

    unsafe {
        let result = sysctlbyname(
            name.as_ptr(),
            &mut value as *mut u64 as *mut std::ffi::c_void,
            &mut size,
            ptr::null_mut(),
            0,
        );

        if result == 0 && value > 0 {
            return Some(value / 1_000_000); // Convert Hz to MHz
        }
    }

    // Fallback to hw.cpufrequency_max
    let name = CStr::from_bytes_with_nul(b"hw.cpufrequency_max\0").ok()?;
    let mut value: u64 = 0;
    let mut size = mem::size_of::<u64>();

    unsafe {
        let result = sysctlbyname(
            name.as_ptr(),
            &mut value as *mut u64 as *mut std::ffi::c_void,
            &mut size,
            ptr::null_mut(),
            0,
        );

        if result == 0 && value > 0 {
            Some(value / 1_000_000) // Convert Hz to MHz
        } else {
            // On modern iOS devices, CPU frequency info may not be available via sysctl
            // Return a reasonable default for modern iOS devices
            Some(2400) // 2.4 GHz - typical for modern iPhones
        }
    }
}


/// Free-standing umbrella helper: spawn a background thread that refreshes
/// both the dict source-uid caches and the per-area language caches. Used by
/// the user-dict mutation call sites in the bridges so the calling thread
/// (GUI or bridge worker) never blocks on the DISTINCT scans.
pub fn refresh_all_dict_caches() {
    std::thread::spawn(|| {
        let app_data = crate::get_app_data();
        app_data.refresh_dict_source_uid_caches();
        app_data.refresh_language_caches();
    });
}

/// Synchronous variant used at bootstrap (cli) to pre-warm all five
/// `AppSettings::cached_*` fields in `appdata.sqlite3` so a freshly
/// bootstrapped DB ships with the caches populated and first launch never
/// needs to run the underlying `SELECT DISTINCT` scans on the GUI thread.
pub fn warm_caches_into_appdata() {
    crate::logger::info("=== warm_caches_into_appdata() ===");
    crate::init_app_data();
    let app_data = crate::get_app_data();
    app_data.refresh_dict_source_uid_caches();
    app_data.refresh_language_caches();
    crate::logger::info("warm_caches_into_appdata: done");
}

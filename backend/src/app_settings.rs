use indexmap::IndexMap;
use serde::{Serialize, Deserialize};

use crate::logger::error;

static PROVIDERS_JSON: &str = include_str!("../../assets/providers.json");
pub static LANGUAGES_JSON: &str = include_str!("../../assets/languages.json");
pub static SUTTA_REFERENCE_CONVERTER_JSON: &str = include_str!("../../assets/sutta-reference-converter.json");
pub static CIPS_GENERAL_INDEX_JSON: &str = include_str!("../../assets/general-index.json");
static KEYBINDINGS_JSON: &str = include_str!("../../assets/keybindings.json");

/// Where a model entry came from. `Fetched` entries are owned by the model-list
/// updater (it adds and removes them); `User` entries were added by hand in the
/// models dialog and are never removed by the updater, only flagged `stale`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelOrigin {
    #[default]
    Fetched,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelEntry {
    pub model_name: String,
    pub enabled: bool,
    #[serde(default)]
    pub origin: ModelOrigin,
    /// A `User` model which the updater no longer finds upstream. Shown with a
    /// "not found upstream" marker; never auto-removed.
    #[serde(default)]
    pub stale: bool,
    /// Whether this is a reasoning/thinking model, when the source publishes
    /// it (models.dev `reasoning`, OpenRouter `supported_parameters`).
    /// `None` = unknown (SambaNova's bare id list, hand-added models).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
}

/// One entry of a global model-usage list ("Fallback sequence" or "Parallel
/// prompts"). The provider is stored as its canonical string form
/// (`ProviderName::as_str()`), the model as the provider's model id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelUsageEntry {
    pub provider: String,
    pub model_name: String,
    pub enabled: bool,
}

/// How an AI feature dispatches its requests. `SequentialRetry` walks the
/// "Fallback sequence" list and produces one result; `Parallel` fans out to
/// every enabled "Parallel prompts" model (each branch stays on its own
/// model). See docs/ai-model-management-and-fallback.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AiRequestMode {
    #[default]
    #[serde(rename = "sequential_retry")]
    SequentialRetry,
    #[serde(rename = "parallel")]
    Parallel,
}

impl AiRequestMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            AiRequestMode::SequentialRetry => "sequential_retry",
            AiRequestMode::Parallel => "parallel",
        }
    }

    /// Parse the canonical string form; unknown values fall back to the default.
    pub fn from_str_or_default(s: &str) -> Self {
        match s {
            "parallel" => AiRequestMode::Parallel,
            _ => AiRequestMode::SequentialRetry,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provider {
    pub name: ProviderName,
    pub description: String,
    pub enabled: bool,
    /// e.g. OPENROUTER_API_KEY, DEEPSEEK_API_KEY, etc. which may be present as env variables.
    pub api_key_env_var_name: String,
    pub api_key_value: Option<String>,
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderName {
    Gemini,
    OpenRouter,
    Anthropic,
    OpenAI,
    DeepSeek,
    #[serde(rename = "xAI")]
    XAI,
    Mistral,
    HuggingFace,
    Perplexity,
    NvidiaNim,
    SambaNova,
}

impl ProviderName {
    /// The canonical string form of a provider name: the serde spelling, which is
    /// what `providers.json` carries and what QML passes back to the bridge.
    ///
    /// Note this is *not* the `Debug` form for every variant (`XAI` debugs as
    /// `"XAI"` but is canonically `"xAI"`), so never produce a provider name with
    /// `format!("{:?}", …)`.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderName::Gemini => "Gemini",
            ProviderName::OpenRouter => "OpenRouter",
            ProviderName::Anthropic => "Anthropic",
            ProviderName::OpenAI => "OpenAI",
            ProviderName::DeepSeek => "DeepSeek",
            ProviderName::XAI => "xAI",
            ProviderName::Mistral => "Mistral",
            ProviderName::HuggingFace => "HuggingFace",
            ProviderName::Perplexity => "Perplexity",
            ProviderName::NvidiaNim => "NvidiaNim",
            ProviderName::SambaNova => "SambaNova",
        }
    }

    pub const ALL: [ProviderName; 11] = [
        ProviderName::Gemini,
        ProviderName::OpenRouter,
        ProviderName::Anthropic,
        ProviderName::OpenAI,
        ProviderName::DeepSeek,
        ProviderName::XAI,
        ProviderName::Mistral,
        ProviderName::HuggingFace,
        ProviderName::Perplexity,
        ProviderName::NvidiaNim,
        ProviderName::SambaNova,
    ];

    /// Parse a canonical (or legacy `Debug`-form) provider name. Legacy settings
    /// may carry `"XAI"`, produced by the old `format!("{:?}")` call sites.
    pub fn from_canonical_or_legacy(s: &str) -> Option<ProviderName> {
        ProviderName::ALL
            .iter()
            .find(|p| p.as_str() == s || format!("{:?}", p) == s)
            .copied()
    }
}

impl std::fmt::Display for ProviderName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub sutta_font_size: usize,
    pub sutta_max_width: usize,
    pub show_bookmarks: bool,
    pub show_all_variant_readings: bool,
    pub show_glosses: bool,
    pub theme_name: ThemeName,
    pub api_keys: IndexMap<String, String>,
    pub system_prompts: IndexMap<String, String>,
    pub providers: Vec<Provider>,
    pub ai_models_auto_retry: bool,
    pub anki_template_front: String,
    pub anki_template_back: String,
    pub anki_template_cloze_front: String,
    pub anki_template_cloze_back: String,
    pub anki_export_format: AnkiExportFormat,
    pub anki_include_cloze: bool,
    pub search_as_you_type: bool,
    pub open_find_in_sutta_results: bool,
    pub show_bottom_footnotes: bool,
    pub first_time_start: bool,
    pub mobile_top_bar_margin: MobileTopBarMargin,
    /// Whether to show update notifications on startup
    pub notify_about_simsapa_updates: bool,
    /// Release channel for updates (e.g., "main" or "development")
    /// None means use default "main"
    pub release_channel: Option<String>,
    /// Custom keyboard shortcuts for application actions
    pub app_keybindings: AppKeybindings,
    /// OS-level global hotkeys (separate from `app_keybindings`; carries an
    /// extra `enabled` flag and is delivered by `GlobalHotkeyManager`).
    #[serde(default)]
    pub global_hotkeys: crate::global_hotkeys::GlobalHotkeysConfig,
    /// Include commentary (Aṭṭhakathā .att, Ṭīkā .tik) in translation tab
    pub include_cst_commentary_in_translations: bool,
    /// Include CST Mūla Pāli texts in search results
    pub include_cst_mula_in_search_results: bool,
    /// Include commentary records in sutta search results
    pub include_cst_commentary_in_search_results: bool,
    /// Include CST Pāli version in translation tab
    pub include_cst_mula_in_translations: bool,
    /// Include Mahāsaṅgīti (MS) Mūla Pāli texts in search results
    pub include_ms_mula_in_search_results: bool,
    /// Include DPD bold-definition (commentary) entries in Dictionary search results.
    /// (Originally backed the "Commentary Definitions in Search" checkbox in the
    /// advanced options; relocated to the Dictionaries panel in the StarDict
    /// import feature. Key preserved so user preferences carry over.)
    #[serde(default = "default_true")]
    pub include_comm_bold_definitions_in_search_results: bool,
    /// DPD enabled in the Dictionaries panel of the dictionary search.
    #[serde(default = "default_true")]
    pub dict_search_dpd_enabled: bool,
    /// Per-dictionary enabled state, keyed by dictionary label (other than DPD and Bold Commentary Definitions).
    #[serde(default)]
    pub dict_search_dict_enabled: IndexMap<String, bool>,
    /// Last-used search mode per search area, keyed by area name
    /// (`"Suttas"`, `"Dictionary"`, `"Library"`). Values are the labels as
    /// they appear in the QML dropdown. Missing entries fall back to the
    /// per-area default at read time.
    #[serde(default)]
    pub search_last_mode: IndexMap<String, String>,
    /// Last-used language filter key per search area, keyed by area name
    /// (`"Suttas"`, `"Dictionary"`, `"Library"`). Values are the language codes
    /// as they appear in the QML dropdown (e.g. `"pli"`, `"en"`). The sentinel
    /// `"Language"` and any missing entry both mean "no language filter".
    #[serde(default)]
    pub search_last_language: IndexMap<String, String>,
    /// Cached set of distinct `dict_words.dict_label` values for non-user-imported
    /// dictionaries. Populated at startup (and refreshed after user-dict
    /// import / delete / rename) so the dictionary search bar doesn't run a
    /// `SELECT DISTINCT` against `dict_words` on every query.
    #[serde(default)]
    pub cached_shipped_source_uids: Vec<String>,
    /// Cached set of distinct DPD `bold_definitions.ref_code` values used as
    /// commentary-definition source uids. Populated at startup (refreshed on
    /// user-dict mutations for symmetry — the underlying DPD table is static
    /// during a session).
    #[serde(default)]
    pub cached_commentary_definitions_source_uids: Vec<String>,
    /// Cached distinct `suttas.language` values. Populated at bootstrap and
    /// refreshed after sutta-language download / removal.
    #[serde(default)]
    pub cached_sutta_languages: Vec<String>,
    /// Cached distinct `dict_words.language` values. Populated at bootstrap and
    /// refreshed after user-dict import / delete / rename.
    #[serde(default)]
    pub cached_dict_languages: Vec<String>,
    /// Cached distinct library languages (merge of `book_spine_items.language`
    /// and `books.language`). Populated at bootstrap; no runtime book-import
    /// path exists today.
    #[serde(default)]
    pub cached_library_languages: Vec<String>,
    /// Whether to restore the last session (open tabs) on startup
    #[serde(default = "default_true")]
    pub restore_last_session: bool,

    // --- Mobile rendering troubleshooting toggles ---
    // These work around GPU framebuffer / scene-graph corruption seen on some
    // flaky Android GPU drivers. All default to off; the user enables them in
    // the Settings → Rendering tab (mobile only). The env-var backed one
    // (`render_loop_basic`) is read in `gui.cpp` before the QApplication is
    // constructed, so it requires an app restart to take effect.
    /// Collapse the per-result-card gradient background to a flat color
    /// (sets `use_flat_bg` on `ListBackground` in `FulltextResults.qml`).
    #[serde(default)]
    pub render_use_flat_results_background: bool,
    /// Disable `clip: true` on the results `ListView` (stencil/scissor clip is
    /// mishandled by some drivers).
    #[serde(default)]
    pub render_disable_results_clip: bool,
    /// Force the single-threaded `basic` Qt Quick render loop
    /// (`QSG_RENDER_LOOP=basic`).
    #[serde(default)]
    pub render_loop_basic: bool,

    // --- Snippet display settings ---

    /// Chars before the matched term in single-snippet mode
    #[serde(default = "default_snippet_chars_before")]
    pub snippet_chars_before: usize,
    /// Chars after the matched term in single-snippet mode
    #[serde(default = "default_snippet_chars_after")]
    pub snippet_chars_after: usize,
    /// Chars before the matched term in all-snippets mode
    #[serde(default = "default_snippet_all_chars_before")]
    pub snippet_all_chars_before: usize,
    /// Chars after the matched term in all-snippets mode
    #[serde(default = "default_snippet_all_chars_after")]
    pub snippet_all_chars_after: usize,
    /// Whether to use default item height (line height x 4) in search results
    #[serde(default = "default_true")]
    pub item_height_use_default: bool,
    /// Fixed item height in pixels (used when item_height_use_default is false)
    #[serde(default)]
    pub item_height_fixed: usize,
    /// Persisted defaults for the sutta display (layout, typography, per-author
    /// colors). Concrete column uid lists are per-view state and are NOT part
    /// of these defaults — the default column set is resolved at render time
    /// (opened sutta + Pāli counterpart). See
    /// tasks/2026-07-01-192905-prd---side-by-side-translation-view.md.
    #[serde(default)]
    pub sutta_display: SuttaDisplayDefaults,

    // --- Gloss tab: AI word selection ---
    /// Whether AI word selection is enabled in the Gloss tab (a model is selected).
    #[serde(default)]
    pub gloss_word_selection_enabled: bool,
    /// Provider name of the selected word-selection model (empty = none).
    #[serde(default)]
    pub gloss_word_selection_provider: String,
    /// Model name used for word selection (empty = none).
    #[serde(default)]
    pub gloss_word_selection_model: String,

    // --- Global model-usage lists (ModelsDialog global options) ---
    /// Models tried one after another by the sequential fallback engine. The
    /// vector order is the fallback order. See
    /// docs/ai-model-management-and-fallback.md.
    #[serde(default)]
    pub ai_fallback_sequence: Vec<ModelUsageEntry>,
    /// Models dispatched simultaneously in parallel-prompt mode. Unordered.
    #[serde(default)]
    pub ai_parallel_prompts: Vec<ModelUsageEntry>,
    /// On a retryable error, fall back to the next enabled model in
    /// `ai_fallback_sequence` (sequential engine). When off, only the first
    /// enabled sequence model is used.
    #[serde(default = "default_true")]
    pub ai_auto_fallback: bool,
    /// Gloss tab: how AI translation requests are dispatched.
    #[serde(default)]
    pub gloss_ai_translate_mode: AiRequestMode,
    /// Prompts tab: how the next assistant response is requested.
    #[serde(default)]
    pub prompts_request_mode: AiRequestMode,
}

/// Sutta view layout mode. UI labels are "Solo" / "Columns" / "Lines"; the
/// enum keeps the descriptive line-by-line / side-by-side naming. Both
/// spellings are accepted when parsing (`linebyline`/`lines`,
/// `sidebyside`/`columns`). Solo renders only the opened translation via the
/// standard whole-document path (no Pāli / other translation columns).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SuttaLayout {
    #[serde(rename = "solo")]
    Solo,
    #[default]
    #[serde(rename = "linebyline", alias = "lines", alias = "line-by-line")]
    LineByLine,
    #[serde(rename = "sidebyside", alias = "columns", alias = "side-by-side")]
    SideBySide,
}

impl SuttaLayout {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "solo" => Some(SuttaLayout::Solo),
            "linebyline" | "lines" | "line-by-line" => Some(SuttaLayout::LineByLine),
            "sidebyside" | "columns" | "side-by-side" => Some(SuttaLayout::SideBySide),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SuttaLayout::Solo => "solo",
            SuttaLayout::LineByLine => "linebyline",
            SuttaLayout::SideBySide => "sidebyside",
        }
    }
}

/// Where the Pāli text appears in the multi-column sutta view (follows the
/// study.jhana.info "Repeat Pāli" control). The Pāli's default position is
/// the first column; this controls whether/where it repeats:
/// Off = first column only; Alternate = before each translation;
/// AtEnd = first column and once more as the last column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RepeatPali {
    #[default]
    #[serde(rename = "off")]
    Off,
    #[serde(rename = "alternate")]
    Alternate,
    #[serde(rename = "atend", alias = "at-end", alias = "at_end")]
    AtEnd,
}

impl RepeatPali {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "off" => Some(RepeatPali::Off),
            "alternate" => Some(RepeatPali::Alternate),
            "atend" | "at-end" | "at_end" => Some(RepeatPali::AtEnd),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            RepeatPali::Off => "off",
            RepeatPali::Alternate => "alternate",
            RepeatPali::AtEnd => "atend",
        }
    }
}

/// Font family kind for a sutta text column group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SuttaFontFamilyKind {
    #[default]
    Serif,
    Sans,
}

/// Typography settings for one group of sutta view cells (the Pāli cells or
/// the translation cells). Sizes are stored as integer percentages so
/// `AppSettings` stays `Eq` (100 = 1em / normal line height baseline 1.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SuttaFontGroup {
    pub family_kind: SuttaFontFamilyKind,
    /// Font size as a percentage of the base sutta font size (100 = 1em).
    pub size_percent: usize,
    /// Line height as a percentage (150 = 1.5).
    pub line_height_percent: usize,
    pub bold: bool,
    pub italic: bool,
}

impl Default for SuttaFontGroup {
    fn default() -> Self {
        SuttaFontGroup {
            family_kind: SuttaFontFamilyKind::Serif,
            size_percent: 100,
            line_height_percent: 150,
            bold: false,
            italic: false,
        }
    }
}

/// Persisted sutta display defaults, nested in `AppSettings` as
/// `sutta_display`. Color maps are keyed by author uid (e.g. "sujato"); "pali"
/// is used for the Pāli column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SuttaDisplayDefaults {
    pub layout: SuttaLayout,
    /// Pāli column placement/repetition in the multi-column layouts.
    pub repeat_pali: RepeatPali,
    /// Reading-measure width as a percentage of the base `sutta_max_width`
    /// (100 = unchanged). Applied as the `--width-scale` CSS var to both the
    /// line-by-line body measure and the side-by-side per-column cap.
    pub width_percent: usize,
    pub pali_font: SuttaFontGroup,
    pub translation_font: SuttaFontGroup,
    /// Per-author text ("ink") colors, e.g. "sujato" -> "#663399".
    pub author_ink_colors: IndexMap<String, String>,
    /// Per-author column background colors.
    pub author_bg_colors: IndexMap<String, String>,
}

impl Default for SuttaDisplayDefaults {
    fn default() -> Self {
        SuttaDisplayDefaults {
            layout: SuttaLayout::default(),
            repeat_pali: RepeatPali::default(),
            width_percent: 100,
            // Matches the stylesheet's un-overridden look: Pāli cells render
            // in "Source Sans 3 SSP" at 0.8em (see _suttacentral.sass),
            // translations in the serif body font at 1em. The CSS custom
            // properties applied from these values must not change the
            // appearance of a fresh install.
            pali_font: SuttaFontGroup {
                family_kind: SuttaFontFamilyKind::Sans,
                size_percent: 80,
                line_height_percent: 150,
                ..SuttaFontGroup::default()
            },
            translation_font: SuttaFontGroup::default(),
            author_ink_colors: IndexMap::new(),
            author_bg_colors: IndexMap::new(),
        }
    }
}

/// The built-in default system prompts, keyed as shown in the System Prompts
/// window. Used for `AppSettings::default()`, for merging newly added default
/// keys into existing users' settings on load
/// (`merge_default_system_prompts()`), and for serving individual defaults to
/// the "Reset to Default" button (`SuttaBridge::get_default_system_prompt`).
pub fn default_system_prompts() -> IndexMap<String, String> {
    let mut prompts = IndexMap::new();
    prompts.insert("Gloss Tab: System Prompt".to_string(),
        r#"
You are a helpful assistant for studying the suttas of the Theravāda Pāli Tipitaka and the Pāli language.
Respond with concise answers and respond only with the information requested in the task.
Respond with GFM-Markdown formatted text.
"#.trim().to_string());

    prompts.insert("Gloss Tab: AI Translation with Vocabulary".to_string(),
        r#"
Translate the following Pāli passage to English, keeping in mind the provided dictionary definitions.

Pāli passage:

<<PALI_PASSAGE>>

Dictionary definitions:

<<DICTIONARY_DEFINITIONS>>

Respond with only the translation of the Pāli passage.
Respond with GFM-Markdown formatted text.
"#.trim().to_string());

    prompts.insert("Gloss Tab: AI Translation without Vocabulary".to_string(),
        r#"
Translate the following Pāli passage to English.

Pāli passage:

<<PALI_PASSAGE>>

Respond with only the translation of the Pāli passage.
Respond with GFM-Markdown formatted text.
"#.trim().to_string());

    prompts.insert("Gloss Tab: Word Selection System Prompt".to_string(),
        r#"
You are an expert in Pāli grammar and vocabulary, assisting with the word-by-word glossing of Theravāda Pāli texts. For each listed word, choose the dictionary entry whose meaning fits the word as used in its context. Some items instead ask which compound break-down (sandhi/compound deconstruction) fits the context, or which sense a component word of a compound has; answer them with the same option format. Respond with JSON only — no explanations, no markdown code fences.
"#.trim().to_string());

    prompts.insert("Gloss Tab: Word Selection Request".to_string(),
        r#"
Each item below is a Pāli word in its context, with candidate options. For each item, select the option that fits the context, and return its "word" value copied verbatim (including sense numbers and diacritics).

There are three kinds of items, distinguished by their "id" suffix:

- Sense items (id "p<n>w<n>"): the options are candidate dictionary entries for the word; select the entry whose meaning fits the context.
- Break-down items (id ending in "d"): the word is a compound or sandhi form, and each option's "word" is a possible break-down into component words (e.g. "sādhu + iti"; the option "uid" is a pseudo-uid like "d:0"). Select the break-down that fits the context.
- Component items (id ending in "c<n>"): the item's "word" is the compound, and "component_word" names one of its component words; the options are candidate dictionary entries for that component. The item's "deconstructions" array lists the compound's possible break-downs as strings; when the item also has a "breakdowns" array, the component occurs only in those break-downs. Select the entry whose meaning fits the component as used within the compound in context, and keep your component selections consistent with the break-down you selected for that compound (when a break-down item for it is present).

<<WORD_SELECTION_JSON>>

Respond with JSON in exactly this format, one selection per item:

{"selections": [{"id": "<item id>", "word": "<chosen option's word>"}]}

A selection entry may also carry two optional fields: "confidence" — either "confident" (the default) or "review" when the context is insufficient to decide; and "note" — a one-line reason for "review" entries.
"#.trim().to_string());

    prompts.insert("Prompts Tab: System Prompt".to_string(),
        r#"
You are a helpful assistant for studying the suttas of the Theravāda Pāli Tipitaka and the Pāli language.
Respond with concise answers and respond only with the information requested in the task.
Respond with GFM-Markdown formatted text.
"#.trim().to_string());

    prompts
}

fn default_true() -> bool {
    true
}

#[allow(dead_code)]
fn default_false() -> bool {
    false
}

fn default_snippet_chars_before() -> usize {
    30
}

fn default_snippet_chars_after() -> usize {
    350
}

fn default_snippet_all_chars_before() -> usize {
    30
}

// Use 200 for shorter results text in all-snippets mode
fn default_snippet_all_chars_after() -> usize {
    200
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            sutta_font_size: 22,
            sutta_max_width: 75,
            show_bookmarks: true,
            show_all_variant_readings: false,
            show_glosses: false,
            theme_name: ThemeName::Light,
            api_keys: IndexMap::new(),
            system_prompts: default_system_prompts(),
            providers: {
                match serde_json::from_str::<Vec<Provider>>(PROVIDERS_JSON) {
                    Ok(providers) => providers,
                    Err(e) => {
                        error(&format!("Failed to parse providers JSON: {}", e));
                        vec![]
                    }
                }
            },
            ai_models_auto_retry: false,

            anki_template_front: r#"<style>
.word \{
color: #CF6303;
font-size: 1.2em;
}
.snippet \{ color: #504949; }
</style>
<div>
<p class="word">{word_stem}</p>
<p class="snippet">{context_snippet}</p>
</div>"#.to_string(),

            anki_template_back: r#"<style>
.gram \{ color: #BA9903; font-weight: bold; }
.constr \{ color: #890339; font-weight: bold; }
.constr-desc \{ color: #6E505E; }
table \{ text-align: left; }
table tr \{ text-align: left; }
table tr td \{ text-align: left; padding: 0.1em 0.5em; }
</style>
<div>
<p>{vocab.summary}</p>
<table style="padding-top: 0.5em;">
<tr>
    <td class="gram">Meaning:</td>
    <td>{dpd.meaning_1}</td>
</tr>
<tr>
    <td class="gram">Grammar:</td>
    <td>{dpd.grammar}</td>
</tr>
<tr>
    <td class="gram">Pos:</td>
    <td>{dpd.pos}</td>
</tr>
<tr>
    <td class="constr">Root:</td>
    <td class="constr-desc">
        {{ if dpd.root_key }}
            {root.root_clean}･{root.root_group} {root.root_sign} ({root.root_meaning})
        {{ endif }}
    </td>
</tr>
<tr>
    <td class="constr">Construction:</td>
    <td class="constr-desc">{dpd.construction}</td>
</tr>
</table>
</div>"#.to_string(),

            anki_template_cloze_front: "<div>{context_snippet}</div>".to_string(),
            anki_template_cloze_back: "<div>{vocab.summary}</div>".to_string(),
            anki_export_format: AnkiExportFormat::Templated,
            anki_include_cloze: true,
            search_as_you_type: true,
            open_find_in_sutta_results: true,
            show_bottom_footnotes: true,
            first_time_start: true,
            mobile_top_bar_margin: MobileTopBarMargin::default(),
            notify_about_simsapa_updates: true,
            release_channel: None,
            app_keybindings: AppKeybindings::default(),
            global_hotkeys: crate::global_hotkeys::GlobalHotkeysConfig::default(),
            include_cst_commentary_in_translations: false,
            include_cst_mula_in_search_results: false,
            include_cst_commentary_in_search_results: true,
            include_cst_mula_in_translations: false,
            include_ms_mula_in_search_results: true,
            include_comm_bold_definitions_in_search_results: true,
            dict_search_dpd_enabled: true,
            dict_search_dict_enabled: IndexMap::new(),
            search_last_mode: IndexMap::new(),
            search_last_language: IndexMap::new(),
            cached_shipped_source_uids: Vec::new(),
            cached_commentary_definitions_source_uids: Vec::new(),
            cached_sutta_languages: Vec::new(),
            cached_dict_languages: Vec::new(),
            cached_library_languages: Vec::new(),
            restore_last_session: true,
            render_use_flat_results_background: false,
            render_disable_results_clip: false,
            render_loop_basic: false,
            snippet_chars_before: default_snippet_chars_before(),
            snippet_chars_after: default_snippet_chars_after(),
            snippet_all_chars_before: default_snippet_all_chars_before(),
            snippet_all_chars_after: default_snippet_all_chars_after(),
            item_height_use_default: true,
            item_height_fixed: 100,
            sutta_display: SuttaDisplayDefaults::default(),
            gloss_word_selection_enabled: false,
            gloss_word_selection_provider: String::new(),
            gloss_word_selection_model: String::new(),
            ai_fallback_sequence: Vec::new(),
            ai_parallel_prompts: Vec::new(),
            ai_auto_fallback: true,
            gloss_ai_translate_mode: AiRequestMode::default(),
            prompts_request_mode: AiRequestMode::default(),
        }
    }
}

impl AppSettings {
    /// Insert any missing built-in default `system_prompts` keys, so existing
    /// users' settings gain newly added prompts without overwriting their
    /// edits to existing keys. Returns true if any key was added.
    pub fn merge_default_system_prompts(&mut self) -> bool {
        let mut changed = false;
        for (prompt_key, prompt_text) in default_system_prompts() {
            if !self.system_prompts.contains_key(&prompt_key) {
                self.system_prompts.insert(prompt_key, prompt_text);
                changed = true;
            }
        }
        changed
    }

    pub fn theme_name_as_string(&self) -> String {
        match self.theme_name {
            ThemeName::Light => "light".to_string(),
            ThemeName::Dark => "dark".to_string(),
        }
    }

    pub fn set_theme_name_from_str(&mut self, theme_name: &str) {
        let theme_name = match theme_name.to_lowercase().as_str() {
            "light" => ThemeName::Light,
            "dark" => ThemeName::Dark,
            _ => {
                error(&format!("Can't recognize theme name: {}, using the default ThemeName::Light", theme_name));
                ThemeName::Light
            }
        };
        self.theme_name = theme_name;
    }

    pub fn is_mobile_top_bar_margin_system(&self) -> bool {
        matches!(self.mobile_top_bar_margin, MobileTopBarMargin::SystemValue)
    }

    pub fn get_mobile_top_bar_margin_custom_value(&self) -> u32 {
        match self.mobile_top_bar_margin {
            MobileTopBarMargin::CustomValue(value) => value,
            MobileTopBarMargin::SystemValue => 0,
        }
    }

    pub fn set_mobile_top_bar_margin_system(&mut self) {
        self.mobile_top_bar_margin = MobileTopBarMargin::SystemValue;
    }

    pub fn set_mobile_top_bar_margin_custom(&mut self, value: u32) {
        self.mobile_top_bar_margin = MobileTopBarMargin::CustomValue(value);
    }
}

/// Anki CSV export format type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnkiExportFormat {
    Simple,
    Templated,
    DataCsv,
}

/// Mobile top bar margin setting
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub enum MobileTopBarMargin {
    #[default]
    SystemValue,
    CustomValue(u32),
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeName {
    #[serde(rename = "light")]
    Light,
    #[serde(rename = "dark")]
    Dark,
}

/// Definition of a keybinding action loaded from JSON (used for defaults and UI display)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeybindingDefinition {
    /// Unique identifier for the action (e.g., "focus_search")
    pub id: String,
    /// Human-readable name for display (e.g., "Focus Search Input")
    pub name: String,
    /// Description of what the action does (for UI help text)
    pub description: String,
    /// Default keyboard shortcuts for this action
    pub shortcuts: Vec<String>,
}

/// Returns the keybinding definitions loaded from the embedded JSON
fn get_keybinding_definitions() -> Vec<KeybindingDefinition> {
    match serde_json::from_str::<Vec<KeybindingDefinition>>(KEYBINDINGS_JSON) {
        Ok(definitions) => definitions,
        Err(e) => {
            error(&format!("Failed to parse keybindings JSON: {}", e));
            vec![]
        }
    }
}

/// Stores custom keyboard shortcuts for application actions.
/// Maps action IDs to lists of key sequences (e.g., "settings" -> ["Ctrl+,"])
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppKeybindings {
    /// Mapping of action ID to list of keyboard shortcuts
    pub bindings: IndexMap<String, Vec<String>>,
}

impl Default for AppKeybindings {
    fn default() -> Self {
        let definitions = get_keybinding_definitions();
        let mut bindings = IndexMap::new();

        for def in definitions {
            bindings.insert(def.id, def.shortcuts);
        }

        AppKeybindings { bindings }
    }
}

impl AppKeybindings {
    /// Returns a mapping of action IDs to human-readable names for UI display.
    pub fn get_action_names() -> IndexMap<String, String> {
        let definitions = get_keybinding_definitions();
        let mut names = IndexMap::new();

        for def in definitions {
            names.insert(def.id, def.name);
        }

        names
    }

    /// Returns a mapping of action IDs to descriptions for UI display.
    pub fn get_action_descriptions() -> IndexMap<String, String> {
        let definitions = get_keybinding_definitions();
        let mut descriptions = IndexMap::new();

        for def in definitions {
            descriptions.insert(def.id, def.description);
        }

        descriptions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical string must be the serde spelling for every variant, so that
    /// a provider name persisted in the model-usage lists (or passed from QML)
    /// parses back to the same variant. `xAI` is the one where the `Debug` form
    /// differs, and that mismatch used to silently break provider lookups.
    #[test]
    fn provider_name_canonical_string_round_trip() {
        for provider in ProviderName::ALL {
            let canonical = provider.as_str();
            let serialized = serde_json::to_string(&provider).unwrap();
            assert_eq!(serialized, format!("\"{}\"", canonical));

            let parsed: ProviderName = serde_json::from_str(&serialized).unwrap();
            assert_eq!(parsed, provider);

            assert_eq!(ProviderName::from_canonical_or_legacy(canonical), Some(provider));
        }

        assert_eq!(ProviderName::XAI.as_str(), "xAI");
        // Legacy Debug-form values persisted by the old `format!("{:?}")` call sites.
        assert_eq!(ProviderName::from_canonical_or_legacy("XAI"), Some(ProviderName::XAI));
        assert_eq!(ProviderName::from_canonical_or_legacy("Nonsense"), None);
    }

    /// FR-A7 migration: stored settings and the old bundled JSON carry
    /// `removable`, which serde ignores; origin/stale take their defaults.
    #[test]
    fn model_entry_deserializes_legacy_removable_schema() {
        let legacy = r#"{"model_name": "gemini-flash-latest", "enabled": true, "removable": false}"#;
        let entry: ModelEntry = serde_json::from_str(legacy).unwrap();
        assert_eq!(entry.model_name, "gemini-flash-latest");
        assert!(entry.enabled);
        assert_eq!(entry.origin, ModelOrigin::Fetched);
        assert!(!entry.stale);

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"origin\":\"fetched\""), "{}", json);
        assert!(!json.contains("removable"), "{}", json);
    }
}

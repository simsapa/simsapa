use serde::Deserialize;

use crate::app_settings::PaliEditionPreference;
use crate::types::{SearchArea, SearchMode};

/// Filters that can be applied to fulltext search.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchFilters {
    pub lang: Option<String>,
    pub lang_include: bool,
    pub source_uid: Option<String>,
    pub source_include: bool,
    pub nikaya_prefix: Option<String>,
    pub uid_prefix: Option<String>,
    pub uid_suffix: Option<String>,
    pub sutta_ref: Option<String>,
    pub include_cst_mula: bool,
    pub include_cst_commentary: bool,
    pub include_ms_mula: bool,
    /// Which edition to show of a text that is in both Pāli editions. Only
    /// consulted by the sutta path, and only when both mūla flags are true.
    #[serde(default)]
    pub pali_edition: PaliEditionPreference,
    /// When `false`, the dict-index search excludes documents with
    /// `is_bold_definition = true` via `Occur::MustNot`. Default `true`
    /// means bold-definition rows participate alongside dict_words in the
    /// unified dict index. Only consulted by the dict path.
    #[serde(default = "default_true")]
    pub include_bold_definitions: bool,
    /// Inclusion set for dict_words `source_uid` (a.k.a. `dict_label`).
    /// `None` means no constraint. `Some([])` means no dict_words rows
    /// match. Bold-definition rows are gated independently via
    /// `include_bold_definitions`. Only consulted by the dict path.
    #[serde(default)]
    pub dict_source_uids: Option<Vec<String>>,
    /// When `true`, each matched page record is expanded into one
    /// `SearchResult` per matched occurrence (focal-highlighted). Only the
    /// Suttas and Library indexes honour this; the Dict path stays
    /// single-snippet. See docs/search-snippet-highlight-pipeline.md.
    #[serde(default)]
    pub show_all_snippets: bool,
}

fn default_true() -> bool { true }

impl Default for SearchFilters {
    fn default() -> Self {
        Self {
            lang: None,
            lang_include: false,
            source_uid: None,
            source_include: false,
            nikaya_prefix: None,
            uid_prefix: None,
            uid_suffix: None,
            sutta_ref: None,
            include_cst_mula: true,
            include_cst_commentary: true,
            include_ms_mula: true,
            pali_edition: Default::default(),
            include_bold_definitions: true,
            dict_source_uids: None,
            show_all_snippets: false,
        }
    }
}

/// A single step in a composable search pipeline.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchStep {
    pub mode: SearchMode,
    pub query_text: String,
    /// Filters applied at this step
    pub filters: SearchFilters,
}

/// A search pipeline is a list of steps, each narrowing the previous results.
/// For now, only single-step pipelines are supported.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchPipeline {
    pub steps: Vec<SearchStep>,
    pub area: SearchArea,
    pub page_len: Option<usize>,
}

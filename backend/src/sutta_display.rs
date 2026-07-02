/// Runtime sutta display options, resolved once at the call boundary
/// (bridge fn or API route) and passed into the renderer as an argument —
/// `render_sutta_content()` must not read layout from `app_settings_cache`
/// internally, so tests and API requests are deterministic.
///
/// Persisted defaults live in `AppSettings::sutta_display`
/// (`crate::app_settings::SuttaDisplayDefaults`); explicit request parameters
/// override them via `SuttaDisplayOverrides`. See
/// tasks/2026-07-01-192905-prd---side-by-side-translation-view.md.

use crate::app_settings::{AppSettings, SuttaLayout};

/// Explicit per-request overrides of the persisted display defaults
/// (e.g. from `?layout=` / `?columns=` GET parameters).
#[derive(Debug, Clone, Default)]
pub struct SuttaDisplayOverrides {
    pub layout: Option<SuttaLayout>,
    /// Ordered column source sutta uids.
    pub columns: Option<Vec<String>>,
}

/// The resolved options the sutta content renderer receives.
#[derive(Debug, Clone)]
pub struct SuttaDisplayOptions {
    pub layout: SuttaLayout,
    /// Ordered column source sutta uids. The default set is the opened sutta
    /// plus its Pāli counterpart (when one exists); the opened sutta comes
    /// first unless explicit columns say otherwise.
    pub columns: Vec<String>,
    /// Whether per-segment reference anchors are rendered.
    pub show_references: bool,
}

impl SuttaDisplayOptions {
    /// Resolve the effective options: persisted defaults from `app_settings`,
    /// overridden by any explicit request parameters. `pali_uid` is the
    /// opened sutta's Pāli counterpart uid (from
    /// `AppData::get_pali_for_translated`), used for the default column set.
    pub fn resolve(
        app_settings: &AppSettings,
        sutta_uid: &str,
        pali_uid: Option<&str>,
        show_references: bool,
        overrides: &SuttaDisplayOverrides,
    ) -> Self {
        let layout = overrides.layout.unwrap_or(app_settings.sutta_display.layout);

        let columns = match &overrides.columns {
            Some(cols) if !cols.is_empty() => cols.clone(),
            _ => {
                let mut cols = vec![sutta_uid.to_string()];
                if let Some(pali) = pali_uid {
                    if pali != sutta_uid {
                        cols.push(pali.to_string());
                    }
                }
                cols
            }
        };

        SuttaDisplayOptions {
            layout,
            columns,
            show_references,
        }
    }
}

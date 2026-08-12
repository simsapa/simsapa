/// Runtime sutta display options, resolved once at the call boundary
/// (bridge fn or API route) and passed into the renderer as an argument —
/// `render_sutta_content()` must not read layout from `app_settings_cache`
/// internally, so tests and API requests are deterministic.
///
/// Persisted defaults live in `AppSettings::sutta_display`
/// (`crate::app_settings::SuttaDisplayDefaults`); explicit request parameters
/// override them via `SuttaDisplayOverrides`. See
/// tasks/2026-07-01-192905-prd---side-by-side-translation-view.md.

use crate::app_settings::{AppSettings, RepeatPali, SuttaLayout};

/// Explicit per-request overrides of the persisted display defaults
/// (e.g. from `?layout=` / `?columns=` / `?repeat_pali=` GET parameters).
#[derive(Debug, Clone, Default)]
pub struct SuttaDisplayOverrides {
    pub layout: Option<SuttaLayout>,
    /// Ordered column source sutta uids.
    pub columns: Option<Vec<String>>,
    pub repeat_pali: Option<RepeatPali>,
    /// Whether the per-segment reference numbers are rendered. `None` = use
    /// the persisted default. Set by an explicit `show_references` request
    /// parameter (highest precedence) or, on the full-page sutta routes, by
    /// the presence of an `anchor` parameter — see
    /// `docs/sutta-display-settings-and-multi-column-view.md`.
    pub show_references: Option<bool>,
}

/// The resolved options the sutta content renderer receives.
#[derive(Debug, Clone)]
pub struct SuttaDisplayOptions {
    pub layout: SuttaLayout,
    /// Ordered column source sutta uids. The default set is the opened sutta
    /// plus its Pāli counterpart (when one exists); the final Pāli placement
    /// (first column, alternating, repeated at the end) is applied by
    /// `AppData::resolve_sutta_display_options` per `repeat_pali`.
    pub columns: Vec<String>,
    pub repeat_pali: RepeatPali,
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
        overrides: &SuttaDisplayOverrides,
    ) -> Self {
        let layout = overrides.layout.unwrap_or(app_settings.sutta_display.layout);
        let repeat_pali = overrides.repeat_pali.unwrap_or(app_settings.sutta_display.repeat_pali);
        let show_references = overrides.show_references
            .unwrap_or(app_settings.sutta_display.show_references);

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
            repeat_pali,
            show_references,
        }
    }
}

/// Parses the optional `layout` / `columns` / `repeat_pali` GET parameters
/// shared by the sutta-HTML routes and `GET /sutta_content_block` into
/// overrides.
///
/// `layout` accepts both spellings per mode (`lines`/`linebyline`,
/// `columns`/`sidebyside`, plus `solo`); an unknown value is an `Err` whose
/// message the route maps to HTTP 400, and the same applies to `repeat_pali`
/// (`off`/`alternate`/`atend`). `columns` is a `|`-separated list of column
/// sutta uids — Rocket hands query values fully percent-decoded (uids contain
/// `/`), so no decoding happens here; empty items are skipped, and an
/// effectively-empty list is treated as absent (falls back to defaults).
pub fn parse_display_overrides(
    layout: Option<&str>,
    columns: Option<&str>,
    repeat_pali: Option<&str>,
) -> Result<SuttaDisplayOverrides, String> {
    let layout = match layout {
        Some(s) => match SuttaLayout::from_str(s) {
            Some(l) => Some(l),
            None => {
                return Err(format!(
                    "Unknown layout value: '{}' (expected one of: solo, lines, linebyline, columns, sidebyside)",
                    s
                ));
            }
        },
        None => None,
    };

    let repeat_pali = match repeat_pali {
        Some(s) => match RepeatPali::from_str(s) {
            Some(r) => Some(r),
            None => {
                return Err(format!(
                    "Unknown repeat_pali value: '{}' (expected one of: off, alternate, atend)",
                    s
                ));
            }
        },
        None => None,
    };

    let columns = columns.and_then(|s| {
        let cols: Vec<String> = s.split('|')
            .map(|c| c.trim())
            .filter(|c| !c.is_empty())
            .map(|c| c.to_string())
            .collect();
        if cols.is_empty() { None } else { Some(cols) }
    });

    // `show_references` is deliberately not parsed here: Rocket hands it over
    // already typed (`Option<bool>`) on the one route that accepts it
    // (`/sutta_content_block`), so there is no string spelling to validate.
    // The routes set the field on the returned overrides themselves — the
    // full-page sutta routes from the presence of an `anchor` parameter.
    Ok(SuttaDisplayOverrides { layout, columns, repeat_pali, show_references: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_layout_spellings() {
        for s in ["lines", "linebyline", "line-by-line", "LineByLine", "LINES"] {
            let o = parse_display_overrides(Some(s), None, None).unwrap();
            assert_eq!(o.layout, Some(SuttaLayout::LineByLine), "spelling: {}", s);
        }
        for s in ["columns", "sidebyside", "side-by-side", "SideBySide", "COLUMNS"] {
            let o = parse_display_overrides(Some(s), None, None).unwrap();
            assert_eq!(o.layout, Some(SuttaLayout::SideBySide), "spelling: {}", s);
        }
        for s in ["solo", "Solo", "SOLO"] {
            let o = parse_display_overrides(Some(s), None, None).unwrap();
            assert_eq!(o.layout, Some(SuttaLayout::Solo), "spelling: {}", s);
        }
    }

    #[test]
    fn test_parse_layout_unknown_is_err() {
        let e = parse_display_overrides(Some("stacked"), None, None).unwrap_err();
        assert!(e.contains("stacked"), "error should name the bad value: {}", e);
        assert!(e.contains("sidebyside"), "error should list accepted values: {}", e);
        assert!(e.contains("solo"), "error should list accepted values: {}", e);
    }

    #[test]
    fn test_parse_repeat_pali_spellings() {
        for (s, expected) in [
            ("off", RepeatPali::Off),
            ("alternate", RepeatPali::Alternate),
            ("atend", RepeatPali::AtEnd),
            ("at-end", RepeatPali::AtEnd),
            ("at_end", RepeatPali::AtEnd),
            ("AtEnd", RepeatPali::AtEnd),
        ] {
            let o = parse_display_overrides(None, None, Some(s)).unwrap();
            assert_eq!(o.repeat_pali, Some(expected), "spelling: {}", s);
        }
    }

    #[test]
    fn test_parse_repeat_pali_unknown_is_err() {
        let e = parse_display_overrides(None, None, Some("always")).unwrap_err();
        assert!(e.contains("always"), "error should name the bad value: {}", e);
        assert!(e.contains("alternate"), "error should list accepted values: {}", e);
    }

    #[test]
    fn test_parse_absent_params() {
        let o = parse_display_overrides(None, None, None).unwrap();
        assert_eq!(o.layout, None);
        assert_eq!(o.columns, None);
        assert_eq!(o.repeat_pali, None);
    }

    #[test]
    fn test_parse_columns_pipe_separated() {
        let o = parse_display_overrides(None, Some("mn1/en/sujato|mn1/pli/ms|mn1/en/bodhi"), None).unwrap();
        assert_eq!(o.columns, Some(vec![
            "mn1/en/sujato".to_string(),
            "mn1/pli/ms".to_string(),
            "mn1/en/bodhi".to_string(),
        ]));
    }

    #[test]
    fn test_parse_columns_skips_empty_items() {
        let o = parse_display_overrides(None, Some("|mn1/en/sujato|| mn1/pli/ms |"), None).unwrap();
        assert_eq!(o.columns, Some(vec![
            "mn1/en/sujato".to_string(),
            "mn1/pli/ms".to_string(),
        ]));
    }

    #[test]
    fn test_parse_columns_empty_is_absent() {
        for s in ["", "|", " | "] {
            let o = parse_display_overrides(None, Some(s), None).unwrap();
            assert_eq!(o.columns, None, "columns value: '{}'", s);
        }
    }

    #[test]
    fn test_resolve_default_columns() {
        let settings = AppSettings::default();
        let o = SuttaDisplayOptions::resolve(&settings, "mn1/en/sujato", Some("mn1/pli/ms"), &SuttaDisplayOverrides::default());
        assert_eq!(o.layout, SuttaLayout::LineByLine);
        assert_eq!(o.repeat_pali, RepeatPali::Off);
        assert_eq!(o.columns, vec!["mn1/en/sujato".to_string(), "mn1/pli/ms".to_string()]);
        // Precedence rule 3: no override, so the persisted default (off) wins.
        assert!(!o.show_references);
    }

    #[test]
    fn test_resolve_overrides_win() {
        let settings = AppSettings::default();
        let overrides = SuttaDisplayOverrides {
            layout: Some(SuttaLayout::SideBySide),
            columns: Some(vec!["an4.1/pli/ms".to_string()]),
            repeat_pali: Some(RepeatPali::AtEnd),
            show_references: Some(true),
        };
        let o = SuttaDisplayOptions::resolve(&settings, "an4.1/en/sujato", Some("an4.1/pli/ms"), &overrides);
        assert_eq!(o.layout, SuttaLayout::SideBySide);
        assert_eq!(o.columns, vec!["an4.1/pli/ms".to_string()]);
        assert_eq!(o.repeat_pali, RepeatPali::AtEnd);
        assert!(o.show_references);
    }

    #[test]
    fn test_resolve_show_references_precedence() {
        let mut settings = AppSettings::default();
        settings.sutta_display.show_references = true;

        // No override: the persisted default is used (rule 3).
        let o = SuttaDisplayOptions::resolve(&settings, "mn1/pli/ms", None, &SuttaDisplayOverrides::default());
        assert!(o.show_references);

        // An explicit `false` override wins over a `true` default (rule 1) —
        // this is the user turning references off on an anchor-opened page.
        let overrides = SuttaDisplayOverrides { show_references: Some(false), ..Default::default() };
        let o = SuttaDisplayOptions::resolve(&settings, "mn1/pli/ms", None, &overrides);
        assert!(!o.show_references);
    }
}

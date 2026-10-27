//! Request validation for the exact DPD form lookup (`POST /lookup_forms`) and
//! for `/gloss_text`'s exact mode. Kept Qt-free and outside the route so it can
//! be unit-tested: `cargo test` does not compile in `bridges/`.
//!
//! See docs/simsapa-cli-and-localhost-api.md (`POST /lookup_forms`,
//! §16.1).

/// The most forms one `/lookup_forms` request may carry.
pub const MAX_LOOKUP_FORMS: usize = 5000;

/// Check a `/lookup_forms` request. `Err` carries the user-facing message for
/// a `400`. An empty `forms` list is valid and gives no results.
pub fn validate_lookup_forms_request(forms: Option<&Vec<String>>, exact: Option<bool>) -> Result<(), String> {
    let forms = forms.ok_or_else(|| "missing field: forms".to_string())?;
    if exact == Some(false) {
        return Err("only exact lookups are supported; use /dict_combined_search for fuzzy matching".to_string());
    }
    if forms.len() > MAX_LOOKUP_FORMS {
        return Err(format!("too many forms: {} (max {})", forms.len(), MAX_LOOKUP_FORMS));
    }
    Ok(())
}

/// Check the `/gloss_text` options that exact mode ignores. Any of them set to
/// a non-default value (`true`, or a non-empty list) is an `Err`, so a caller
/// is not misled into thinking it took effect. Call it only for `exact: true`.
pub fn validate_gloss_exact_options(
    no_duplicates_globally: Option<bool>,
    skip_common: Option<bool>,
    common_words: Option<&Vec<String>>,
) -> Result<(), String> {
    let mut set: Vec<&str> = Vec::new();
    if no_duplicates_globally == Some(true) {
        set.push("no_duplicates_globally");
    }
    if skip_common == Some(true) {
        set.push("skip_common");
    }
    if common_words.is_some_and(|w| !w.is_empty()) {
        set.push("common_words");
    }
    if set.is_empty() {
        Ok(())
    } else {
        Err(format!("exact mode does not support these options: {}", set.join(", ")))
    }
}

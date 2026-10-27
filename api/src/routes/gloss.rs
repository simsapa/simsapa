//! Gloss pipeline and exact DPD form lookup routes: `POST /gloss_text` and
//! `POST /lookup_forms`. Neither uses an AI provider; the AI word-selection
//! WebSocket stays in `bridges`.
//! See docs/simsapa-cli-and-localhost-api.md §16 and §19.

use std::sync::Arc;

use rocket::serde::{Deserialize, Serialize};
use rocket::serde::json::Json;
use rocket::http::Status;
use rocket::{post, State};

use simsapa_backend::db::DbManager;
use simsapa_backend::helpers::{process_all_paragraphs, process_all_paragraphs_exact};
use simsapa_backend::logger::{info, error};
use simsapa_backend::lookup_forms::{validate_lookup_forms_request, validate_gloss_exact_options};
use simsapa_backend::types::{AllParagraphsProcessingInput, WordProcessingOptions, AllParagraphsProcessingResult};
use simsapa_backend::types::{ExactGlossResult, FormLookupResult, FormLookupStatus};

use crate::helpers::json_error;

/// Request body for `POST /gloss_text`. `options` is optional; each option
/// field defaults to a stateless route-level default (see the docs).
#[derive(Debug, Clone, Deserialize)]
pub struct GlossTextRequest {
    pub text: String,
    #[serde(default)]
    pub options: Option<GlossTextOptions>,
}

/// Per-call gloss options. All fields optional; defaults: `no_duplicates_globally`
/// = false, `skip_common` = false, `common_words` = [], `exact` = false. The
/// route is **stateless** — there is no cross-call dedup state (unlike in-app
/// incremental glossing), so `no_duplicates_globally` applies within the single
/// submitted text only.
///
/// `exact: true` returns one exact DPD form lookup per token instead
/// (`ExactGlossResult`); the other three options do not apply to it, and
/// setting any of them to a non-default value with it is a `400`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GlossTextOptions {
    pub no_duplicates_globally: Option<bool>,
    pub skip_common: Option<bool>,
    pub common_words: Option<Vec<String>>,
    pub exact: Option<bool>,
}

/// The two `/gloss_text` response shapes. Untagged, so each serialises exactly
/// as its struct; never a `serde_json::Value`, which (built without
/// `preserve_order`) would sort the keys and change the default output.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum GlossTextResponse {
    Default(AllParagraphsProcessingResult),
    Exact(ExactGlossResult),
}

/// `/gloss_text` errors: an unparseable body keeps Rocket's bare `400` (the
/// mode is unknown until the body parses); errors found after parsing in exact
/// mode carry a JSON `{ "error" }` body.
#[derive(rocket::Responder)]
pub enum GlossTextError {
    Bare(Status),
    Json((Status, Json<serde_json::Value>)),
}

/// Split a gloss text into paragraphs the way `/gloss_text` does: on blank
/// lines, trimmed, empties dropped. Empty text → no paragraphs.
pub fn split_gloss_paragraphs(text: &str) -> Vec<String> {
    text.split("\n\n")
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// The default-mode processing options of `/gloss_text`: stateless per call,
/// with no cross-call carry-over.
pub fn gloss_word_processing_options(opts: &GlossTextOptions) -> WordProcessingOptions {
    WordProcessingOptions {
        no_duplicates_globally: opts.no_duplicates_globally.unwrap_or(false),
        skip_common: opts.skip_common.unwrap_or(false),
        common_words: opts.common_words.clone().unwrap_or_default(),
        existing_global_stems: std::collections::HashMap::new(),
        existing_paragraph_unrecognized: std::collections::HashMap::new(),
        existing_global_unrecognized: Vec::new(),
    }
}

/// POST /gloss_text
/// Synchronously gloss one or more `\n\n`-separated paragraphs, returning the
/// same `AllParagraphsProcessingResult` shape the GlossTab consumes (full
/// `ProcessedWord` per word, incl. the grouped deconstruction fields). 400 on
/// an unparseable body; empty text → 200 with empty paragraphs.
///
/// With `options.exact: true`, returns `ExactGlossResult` instead: every token
/// in order with its exact DPD form lookup. 400 with `{ "error" }` when an
/// option exact mode ignores is set, 500 with `{ "error" }` on a DB error.
#[post("/gloss_text", data = "<body>")]
pub fn gloss_text(body: String, dbm: &State<Arc<DbManager>>) -> Result<Json<GlossTextResponse>, GlossTextError> {
    let req: GlossTextRequest = serde_json::from_str(&body).map_err(|e| {
        error(&format!("gloss_text(): bad request: {}", e));
        GlossTextError::Bare(Status::BadRequest)
    })?;

    let paragraphs = split_gloss_paragraphs(&req.text);

    let opts = req.options.unwrap_or_default();

    if opts.exact == Some(true) {
        validate_gloss_exact_options(opts.no_duplicates_globally, opts.skip_common, opts.common_words.as_ref())
            .map_err(|msg| {
                error(&format!("gloss_text(): bad request: {}", msg));
                GlossTextError::Json(json_error(Status::BadRequest, &msg))
            })?;

        info(&format!("gloss_text(): exact, {} paragraph(s)", paragraphs.len()));

        return match process_all_paragraphs_exact(&paragraphs, &dbm.dpd) {
            Ok(result) => Ok(Json(GlossTextResponse::Exact(result))),
            Err(e) => {
                error(&format!("gloss_text(): exact: {}", e));
                Err(GlossTextError::Json(json_error(Status::InternalServerError, &e)))
            }
        };
    }

    let options = gloss_word_processing_options(&opts);

    let input = AllParagraphsProcessingInput { paragraphs, options };

    info(&format!("gloss_text(): {} paragraph(s)", input.paragraphs.len()));

    match process_all_paragraphs(&input, &dbm.appdata, &dbm.dpd) {
        Ok(result) => Ok(Json(GlossTextResponse::Default(result))),
        Err(e) => {
            error(&format!("gloss_text(): {}", e));
            Ok(Json(GlossTextResponse::Default(AllParagraphsProcessingResult {
                success: false,
                paragraphs: Vec::new(),
                global_unrecognized_words: Vec::new(),
                updated_global_stems: std::collections::HashMap::new(),
            })))
        }
    }
}

/// Request body for `POST /lookup_forms`. `forms` is an `Option` so a missing
/// field reaches the validator and gets its message, not serde's.
#[derive(Debug, Clone, Deserialize)]
pub struct LookupFormsRequest {
    pub forms: Option<Vec<String>>,
    pub exact: Option<bool>,
}

/// The `/lookup_forms` response: `results[i]` answers `forms[i]`.
#[derive(Debug, Serialize)]
pub struct LookupFormsResponse {
    pub dpd_version: Option<String>,
    pub results: Vec<FormLookupResult>,
}

/// POST /lookup_forms
/// Exact DPD form lookup: one result per input form, in input order, with
/// every headword and split of the form's `lookup` row and nothing chosen.
/// 400 with `{ "error" }` for an unparseable body or a validation failure,
/// 500 with `{ "error" }` on a DB error. See
/// docs/simsapa-cli-and-localhost-api.md.
#[post("/lookup_forms", data = "<body>")]
pub fn lookup_forms(body: String, dbm: &State<Arc<DbManager>>) -> Result<Json<LookupFormsResponse>, (Status, Json<serde_json::Value>)> {
    let req: LookupFormsRequest = serde_json::from_str(&body).map_err(|e| {
        error(&format!("lookup_forms(): bad request: {}", e));
        json_error(Status::BadRequest, &format!("invalid request body: {}", e))
    })?;

    validate_lookup_forms_request(req.forms.as_ref(), req.exact).map_err(|msg| {
        error(&format!("lookup_forms(): bad request: {}", msg));
        json_error(Status::BadRequest, &msg)
    })?;
    let forms = req.forms.unwrap_or_default();

    let results = dbm.dpd.lookup_forms_exact(&forms).map_err(|e| {
        error(&format!("lookup_forms(): {}", e));
        json_error(Status::InternalServerError, &e.to_string())
    })?;

    let count = |status: FormLookupStatus| results.iter().filter(|r| r.status == status).count();
    info(&format!(
        "lookup_forms(): {} form(s), {}/{}/{}/{}",
        forms.len(),
        count(FormLookupStatus::Found),
        count(FormLookupStatus::Deconstructed),
        count(FormLookupStatus::NotFound),
        count(FormLookupStatus::Invalid),
    ));

    Ok(Json(LookupFormsResponse {
        dpd_version: dbm.dpd.dpd_release_version(),
        results,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_gloss_paragraphs() {
        assert!(split_gloss_paragraphs("").is_empty());
        assert!(split_gloss_paragraphs("  \n\n  ").is_empty());
        assert_eq!(split_gloss_paragraphs(" a b \n\n\n\n c "), vec!["a b", "c"]);
        // A single newline stays inside the paragraph.
        assert_eq!(split_gloss_paragraphs("a\nb"), vec!["a\nb"]);
    }
}

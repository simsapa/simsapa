//! Gloss pipeline and exact DPD form lookup routes: `POST /gloss_text` and
//! `POST /lookup_forms`.
//!
//! `/gloss_text` can ask an AI provider (`"ai": true`) only through a
//! [`GlossAiRunner`] the server manages: this crate carries no AI provider,
//! and Rocket refuses a second handler for the same path and method, so the
//! GUI's server and `simsapa serve` hand it in instead of mounting their own
//! route. See docs/simsapa-cli-and-localhost-api.md §16 and §19.

use std::sync::Arc;

use rocket::serde::{Deserialize, Serialize};
use rocket::serde::json::Json;
use rocket::http::{ContentType, Status};
use rocket::{post, State};

use simsapa_backend::db::DbManager;
use simsapa_backend::get_app_data;
use simsapa_backend::gloss_export_builder::{build_gloss_export_data, GlossExportOptions};
use simsapa_backend::gloss_source::{resolve_gloss_source, GlossSourceError, PaliEdition};
use simsapa_backend::helpers::{process_all_paragraphs, process_all_paragraphs_exact};
use simsapa_backend::logger::{info, error};
use simsapa_backend::lookup_forms::{validate_lookup_forms_request, validate_gloss_exact_options};
use simsapa_backend::text_export::{gloss_export_document, TextFormat};
use simsapa_backend::types::{AllParagraphsProcessingInput, GlossFormat, WordProcessingOptions, AllParagraphsProcessingResult};
use simsapa_backend::types::{ExactGlossResult, FormLookupResult, FormLookupStatus};

use crate::helpers::json_error;

/// Request body for `POST /gloss_text`: Pāli `text`, or a sutta `reference`
/// (with an optional `edition` and `paragraphs` spec), never both.
/// `options` is optional; each option field defaults to a stateless
/// route-level default (see the docs).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GlossTextRequest {
    #[serde(default)]
    pub text: Option<String>,
    /// A sutta reference: `dhp23`, `sn1.71`, `mn4/pli/ms`.
    #[serde(default)]
    pub reference: Option<String>,
    /// `"cst"` or `"ms"`; default: `/pli/cst` when it exists, else `/pli/ms`.
    #[serde(default)]
    pub edition: Option<String>,
    /// Which paragraphs of the referenced text: `"1"`, `"1-3"`, `"1-2 4"`.
    #[serde(default)]
    pub paragraphs: Option<String>,
    #[serde(default)]
    pub options: Option<GlossTextOptions>,
}

/// Per-call gloss options. All fields optional; defaults: `no_duplicates_globally`
/// = false, `skip_common` = false, `common_words` = [], `exact` = false,
/// `gloss_format` = `"word_in_sentence"`, `ai` = false, `translate` = false,
/// `output_format` = `"json"`. The route is **stateless** — there is no
/// cross-call dedup state (unlike in-app incremental glossing), so
/// `no_duplicates_globally` applies within the single submitted text only.
///
/// `exact: true` returns one exact DPD form lookup per token instead
/// (`ExactGlossResult`); the other options do not apply to it, and setting
/// any of them to a non-default value with it is a `400`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GlossTextOptions {
    pub no_duplicates_globally: Option<bool>,
    pub skip_common: Option<bool>,
    pub common_words: Option<Vec<String>>,
    pub exact: Option<bool>,
    /// `"word_in_sentence"` (default) or `"headwords"`.
    pub gloss_format: Option<String>,
    /// Ask the server's AI provider to choose senses (default false).
    pub ai: Option<bool>,
    /// Also translate each paragraph; only with `ai`.
    pub translate: Option<bool>,
    /// `"json"` (default), `"markdown"`, `"html"` or `"org"`.
    pub output_format: Option<String>,
}

/// The `/gloss_text` output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlossOutputFormat {
    Json,
    Document(TextFormat),
}

impl GlossOutputFormat {
    pub fn parse(name: &str) -> Result<GlossOutputFormat, String> {
        match name {
            "json" => Ok(GlossOutputFormat::Json),
            "markdown" => Ok(GlossOutputFormat::Document(TextFormat::Markdown)),
            "html" => Ok(GlossOutputFormat::Document(TextFormat::Html)),
            "org" => Ok(GlossOutputFormat::Document(TextFormat::OrgMode)),
            other => Err(format!("unknown output_format '{}' (expected json, markdown, html or org)", other)),
        }
    }

    fn content_type(self) -> ContentType {
        match self {
            GlossOutputFormat::Document(TextFormat::Markdown) => {
                ContentType::new("text", "markdown").with_params(("charset", "utf-8"))
            }
            GlossOutputFormat::Document(TextFormat::Html) => ContentType::HTML,
            GlossOutputFormat::Document(TextFormat::OrgMode) => ContentType::Plain,
            GlossOutputFormat::Json => ContentType::JSON,
        }
    }
}

/// Parse a `gloss_format` option value.
pub fn parse_gloss_format(name: &str) -> Result<GlossFormat, String> {
    match name {
        "word_in_sentence" => Ok(GlossFormat::WordInSentence),
        "headwords" => Ok(GlossFormat::Headwords),
        other => Err(format!("unknown gloss_format '{}' (expected word_in_sentence or headwords)", other)),
    }
}

/// Runs the AI steps of a `/gloss_text` request: word selection over the
/// glossed `result`, then, when `translate`, the two-part translation of
/// each paragraph (`paragraph_texts` indexed by `paragraph_index`). It writes
/// no word-selection cache rows (the route is stateless), and reports AI
/// failures in the result's `ai_errors`, never as an error. It blocks and may
/// start its own async runtime, so the route calls it on a thread of its own.
///
/// Managed by a server that has an AI provider (`.manage(GlossAiRunner::new(…))`);
/// without one, `"ai": true` is a `400`.
pub type GlossAiRunFn = dyn Fn(AllParagraphsProcessingResult, Vec<String>, bool) -> AllParagraphsProcessingResult + Send + Sync;

#[derive(Clone)]
pub struct GlossAiRunner(pub Arc<GlossAiRunFn>);

impl GlossAiRunner {
    pub fn new(f: impl Fn(AllParagraphsProcessingResult, Vec<String>, bool) -> AllParagraphsProcessingResult + Send + Sync + 'static) -> Self {
        GlossAiRunner(Arc::new(f))
    }
}

/// The server's `GlossAiRunner`, if it manages one. A request guard rather
/// than `Option<&State<GlossAiRunner>>`: Rocket's sentinel refuses to launch
/// a server whose routes name an unmanaged `State`, even inside an `Option`.
pub struct MaybeGlossAiRunner(pub Option<GlossAiRunner>);

#[rocket::async_trait]
impl<'r> rocket::request::FromRequest<'r> for MaybeGlossAiRunner {
    type Error = std::convert::Infallible;

    async fn from_request(req: &'r rocket::Request<'_>) -> rocket::request::Outcome<Self, Self::Error> {
        rocket::request::Outcome::Success(MaybeGlossAiRunner(req.rocket().state::<GlossAiRunner>().cloned()))
    }
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

/// A `/gloss_text` success: the JSON response, or a document in the
/// requested `output_format`.
#[derive(rocket::Responder)]
pub enum GlossTextOk {
    Json(Json<GlossTextResponse>),
    Document((ContentType, String)),
}

/// `/gloss_text` errors: an unparseable body keeps Rocket's bare `400`;
/// errors found after parsing carry a JSON `{ "error" }` body.
#[derive(rocket::Responder)]
pub enum GlossTextError {
    Bare(Status),
    Json((Status, Json<serde_json::Value>)),
}

fn bad_request(msg: &str) -> GlossTextError {
    error(&format!("gloss_text(): bad request: {}", msg));
    GlossTextError::Json(json_error(Status::BadRequest, msg))
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
pub fn gloss_word_processing_options(opts: &GlossTextOptions, gloss_format: GlossFormat) -> WordProcessingOptions {
    WordProcessingOptions {
        no_duplicates_globally: opts.no_duplicates_globally.unwrap_or(false),
        skip_common: opts.skip_common.unwrap_or(false),
        common_words: opts.common_words.clone().unwrap_or_default(),
        existing_global_stems: std::collections::HashMap::new(),
        existing_paragraph_unrecognized: std::collections::HashMap::new(),
        existing_global_unrecognized: Vec::new(),
        gloss_format,
    }
}

/// A validated `/gloss_text` request.
#[derive(Debug, Clone)]
pub struct GlossTextPlan {
    pub gloss_format: GlossFormat,
    pub exact: bool,
    pub ai: bool,
    pub translate: bool,
    pub output_format: GlossOutputFormat,
    pub edition: PaliEdition,
}

/// Check the request's combinations: `text` xor `reference`; `edition` and
/// `paragraphs` only with `reference`; `translate` only with `ai`; exact mode
/// with none of the Word in Sentence, AI or document options.
pub fn validate_gloss_text_request(req: &GlossTextRequest, opts: &GlossTextOptions) -> Result<GlossTextPlan, String> {
    match (&req.text, &req.reference) {
        (Some(_), Some(_)) => return Err("give either text or reference, not both".to_string()),
        (None, None) => return Err("missing field: text or reference".to_string()),
        (Some(_), None) if req.edition.is_some() || req.paragraphs.is_some() => {
            return Err("edition and paragraphs need a reference".to_string());
        }
        _ => {}
    }
    let edition = match &req.edition {
        Some(e) => PaliEdition::parse(e).map_err(|e| e.to_string())?,
        None => PaliEdition::Default,
    };
    let exact = opts.exact == Some(true);
    let ai = opts.ai == Some(true);
    let translate = opts.translate == Some(true);
    let output_format = match &opts.output_format {
        Some(f) => GlossOutputFormat::parse(f)?,
        None => GlossOutputFormat::Json,
    };
    if translate && !ai {
        return Err("translate needs ai".to_string());
    }
    let gloss_format = match &opts.gloss_format {
        Some(f) => parse_gloss_format(f)?,
        None => GlossFormat::WordInSentence,
    };
    if exact {
        if opts.gloss_format.as_deref() == Some("word_in_sentence") {
            return Err("exact cannot be combined with gloss_format word_in_sentence".to_string());
        }
        if ai {
            return Err("exact cannot be combined with ai".to_string());
        }
        if output_format != GlossOutputFormat::Json {
            return Err("exact returns JSON only".to_string());
        }
        validate_gloss_exact_options(opts.no_duplicates_globally, opts.skip_common, opts.common_words.as_ref())?;
    }
    Ok(GlossTextPlan { gloss_format, exact, ai, translate, output_format, edition })
}

/// POST /gloss_text
/// Gloss one or more `\n\n`-separated paragraphs of `text`, or the Pāli text
/// of a sutta `reference`. The default JSON response is the
/// `AllParagraphsProcessingResult` the Gloss tab consumes (full
/// `ProcessedWord` per word); in the Word in Sentence format (the default)
/// each paragraph also carries its `rows`. `output_format` markdown, html or
/// org returns the analysis document instead.
///
/// With `options.exact: true`, returns `ExactGlossResult`: every token in
/// order with its exact DPD form lookup.
///
/// 400 on an unparseable body (bare) or a bad combination (`{ "error" }`),
/// 404 with `{ "error" }` for a reference with no Pāli text, 500 with
/// `{ "error" }` on a DB error in exact mode.
#[post("/gloss_text", data = "<body>")]
pub async fn gloss_text(
    body: String,
    dbm: &State<Arc<DbManager>>,
    ai_runner: MaybeGlossAiRunner,
) -> Result<GlossTextOk, GlossTextError> {
    let req: GlossTextRequest = serde_json::from_str(&body).map_err(|e| {
        error(&format!("gloss_text(): bad request: {}", e));
        GlossTextError::Bare(Status::BadRequest)
    })?;
    let opts = req.options.clone().unwrap_or_default();
    let plan = validate_gloss_text_request(&req, &opts).map_err(|m| bad_request(&m))?;

    let (paragraphs, title) = match &req.reference {
        Some(reference) => {
            let source = resolve_gloss_source(&dbm.appdata, reference, plan.edition, req.paragraphs.as_deref())
                .map_err(|e| {
                    let status = match e {
                        GlossSourceError::BadRequest(_) => Status::BadRequest,
                        GlossSourceError::NotFound(_) => Status::NotFound,
                    };
                    error(&format!("gloss_text(): {}", e));
                    GlossTextError::Json(json_error(status, &e.to_string()))
                })?;
            let title = source.title();
            (source.paragraphs, title)
        }
        None => (split_gloss_paragraphs(req.text.as_deref().unwrap_or("")), String::new()),
    };

    if plan.exact {
        info(&format!("gloss_text(): exact, {} paragraph(s)", paragraphs.len()));
        return match process_all_paragraphs_exact(&paragraphs, &dbm.dpd) {
            Ok(result) => Ok(GlossTextOk::Json(Json(GlossTextResponse::Exact(result)))),
            Err(e) => {
                error(&format!("gloss_text(): exact: {}", e));
                Err(GlossTextError::Json(json_error(Status::InternalServerError, &e)))
            }
        };
    }

    let runner = match (plan.ai, ai_runner.0) {
        (true, None) => return Err(bad_request("ai not available in this server")),
        (true, Some(r)) => Some(r),
        (false, _) => None,
    };

    let input = AllParagraphsProcessingInput {
        paragraphs: paragraphs.clone(),
        options: gloss_word_processing_options(&opts, plan.gloss_format),
    };
    info(&format!("gloss_text(): {} paragraph(s), {:?}, ai: {}", input.paragraphs.len(), plan.gloss_format, plan.ai));

    let mut result = match process_all_paragraphs(&input, &dbm.appdata, &dbm.dpd) {
        Ok(result) => result,
        Err(e) => {
            error(&format!("gloss_text(): {}", e));
            AllParagraphsProcessingResult {
                success: false,
                paragraphs: Vec::new(),
                global_unrecognized_words: Vec::new(),
                updated_global_stems: std::collections::HashMap::new(),
                ai_errors: Vec::new(),
            }
        }
    };

    if let Some(runner) = runner
        && result.success
    {
        result = run_ai_off_runtime(runner, result, paragraphs.clone(), plan.translate).await?;
    }

    match plan.output_format {
        GlossOutputFormat::Json => Ok(GlossTextOk::Json(Json(GlossTextResponse::Default(result)))),
        GlossOutputFormat::Document(format) => {
            let doc = gloss_document(&result, &paragraphs, &title, plan.gloss_format, format);
            Ok(GlossTextOk::Document((plan.output_format.content_type(), doc)))
        }
    }
}

/// Run the AI runner on a thread of its own and await it: the runner blocks
/// and starts its own tokio runtime, which must not happen on a thread of
/// Rocket's runtime.
async fn run_ai_off_runtime(
    runner: GlossAiRunner,
    result: AllParagraphsProcessingResult,
    paragraphs: Vec<String>,
    translate: bool,
) -> Result<AllParagraphsProcessingResult, GlossTextError> {
    let (tx, rx) = rocket::tokio::sync::oneshot::channel();
    let spawned = std::thread::Builder::new()
        .name("gloss_text-ai".to_string())
        .spawn(move || {
            let _ = tx.send((runner.0)(result, paragraphs, translate));
        });
    if let Err(e) = spawned {
        error(&format!("gloss_text(): cannot start the AI thread: {}", e));
        return Err(GlossTextError::Json(json_error(Status::InternalServerError, "cannot start the AI thread")));
    }
    rx.await.map_err(|_| {
        error("gloss_text(): the AI thread ended without a result");
        GlossTextError::Json(json_error(Status::InternalServerError, "the AI run failed"))
    })
}

/// The analysis document of a gloss result, as the GUI's export writes it.
pub fn gloss_document(
    result: &AllParagraphsProcessingResult,
    paragraphs: &[String],
    title: &str,
    gloss_format: GlossFormat,
    format: TextFormat,
) -> String {
    let translations: Vec<_> = {
        let mut t = vec![Vec::new(); paragraphs.len()];
        for p in &result.paragraphs {
            if let Some(slot) = t.get_mut(p.paragraph_index) {
                *slot = p.ai_translations.clone();
            }
        }
        t
    };
    let options = GlossExportOptions {
        text: paragraphs.join("\n\n"),
        title: title.to_string(),
        gloss_format,
        no_ai_provider: get_app_data().get_ai_fallback_sequence().is_empty(),
    };
    let data = build_gloss_export_data(result, paragraphs, &translations, &options);
    gloss_export_document(&data, format)
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

//! `simsapa gloss` and `simsapa lookup`: the glossing pipeline of
//! `POST /gloss_text` and the exact DPD form lookup of `POST /lookup_forms`,
//! in-process, with the same request handling and the same `--json` output.
//! See docs/simsapa-cli-and-localhost-api.md §16 and §19.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::{Args, ValueEnum};

use simsapa_api::routes::gloss::{
    GlossTextOptions, GlossTextResponse, LookupFormsResponse, gloss_result_export_data, gloss_word_processing_options,
    result_translations, split_gloss_paragraphs,
};
use simsapa_backend::ai_engine::walk_settings_for;
use simsapa_backend::anki_export::export_anki_csv;
use simsapa_backend::app_settings::{ModelUsageEntry, ProviderName};
use simsapa_backend::docx_export::generate_gloss_docx;
use simsapa_backend::errln;
use simsapa_backend::export_types::GlossExportData;
use simsapa_backend::get_app_data;
use simsapa_backend::gloss_ai_run::{GlossAiOptions, GlossAiProgress, GlossAiStage, ai_available, run_gloss_ai};
use simsapa_backend::gloss_export_builder::{GlossExportOptions, build_gloss_session_json};
use simsapa_backend::gloss_rows::{CONFIDENCE_LEGEND, GlossRow, NO_AI_PROVIDER_NOTE};
use simsapa_backend::gloss_source::{GlossSourceError, PaliEdition, is_sutta_reference, resolve_gloss_source};
use simsapa_backend::helpers::{build_gloss_session_export_json, process_all_paragraphs, process_all_paragraphs_exact};
use simsapa_backend::lookup_forms::{validate_gloss_exact_options, validate_lookup_forms_request};
use simsapa_backend::text_export::{TextFormat, gloss_export_document};
use simsapa_backend::types::{
    AllParagraphsProcessingInput, AllParagraphsProcessingResult, AnkiCsvExportInput, AnkiCsvTemplates, ExactGlossResult,
    FormLookupResult, FormLookupStatus, FormResultSource, GlossFormat, ProcessedWord,
};

use crate::exit::{AI_FAILED, CliError, CliResult};
use crate::output::{out, outln, print_json, snippet_to_terminal, stdout_is_tty};

/// The words given as arguments joined with spaces, or stdin for a single `-`.
fn text_or_stdin(words: &[String]) -> Result<String, CliError> {
    if words.len() == 1 && words[0] == "-" {
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| CliError::error(format!("Failed to read stdin: {}", e)))?;
        Ok(s)
    } else {
        Ok(words.join(" "))
    }
}

/// The `--gloss-format` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum GlossFormatArg {
    /// Every word of the passage as written, with its grammar, meaning,
    /// construction and root
    WordInSentence,
    /// The dictionary entries of the passage's words
    Headwords,
}

impl GlossFormatArg {
    fn gloss_format(self) -> GlossFormat {
        match self {
            GlossFormatArg::WordInSentence => GlossFormat::WordInSentence,
            GlossFormatArg::Headwords => GlossFormat::Headwords,
        }
    }
}

/// The `--edition` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EditionArg {
    Cst,
    Ms,
}

/// The `--format` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormatArg {
    Markdown,
    Html,
    Org,
    Docx,
    AnkiCsv,
    /// The Gloss tab's session export, which the app opens with "Open JSON"
    Json,
    /// Aligned columns for a terminal
    Text,
}

impl OutputFormatArg {
    /// The format an `--output` file name implies, when `--format` is left off.
    fn from_extension(path: &Path) -> Option<OutputFormatArg> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        Some(match ext.as_str() {
            "md" | "markdown" => OutputFormatArg::Markdown,
            "html" | "htm" => OutputFormatArg::Html,
            "org" => OutputFormatArg::Org,
            "docx" => OutputFormatArg::Docx,
            "csv" => OutputFormatArg::AnkiCsv,
            "json" => OutputFormatArg::Json,
            _ => return None,
        })
    }
}

#[derive(Args, Debug, Default)]
pub struct GlossArgs {
    /// The Pāli text, `-` to read it from stdin, or a sutta reference
    /// (`dhp23`, `sn1.71`, `mn4`). Paragraphs are separated by a blank line.
    #[arg(required = true, num_args = 1..)]
    pub text: Vec<String>,

    /// word-in-sentence (default): every word as written, with its grammar;
    /// headwords: the dictionary entries
    #[arg(long, value_enum, value_name = "FORMAT")]
    pub gloss_format: Option<GlossFormatArg>,

    /// The Pāli edition of a reference; default: CST when the text has one,
    /// else Mahāsaṅgīti (ms)
    #[arg(long, value_enum)]
    pub edition: Option<EditionArg>,

    /// Which paragraphs of a reference: `1`, `1-3`, `1-2 4`
    #[arg(long, value_name = "SPEC")]
    pub paragraphs: Option<String>,

    /// The output format; default: text on a terminal, markdown otherwise, or
    /// what the --output file name's extension says
    #[arg(long, value_enum)]
    pub format: Option<OutputFormatArg>,

    /// Write the document to this file instead of stdout (needed for docx)
    #[arg(long, short = 'o', value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Word in Sentence tables: 1 puts each word on one line; 2 (default)
    /// puts the meanings on the first line and the grammar, construction and
    /// root on the second. For --format text, markdown, html, org and docx
    #[arg(long, value_name = "1|2", value_parser = clap::value_parser!(u8).range(1..=2))]
    pub lines_per_word: Option<u8>,

    /// Make no AI request, even when an AI provider is enabled in the app
    #[arg(long)]
    pub no_ai: bool,

    /// The AI model to ask (`gemini-3-flash-preview`) instead of the app's Fallback sequence
    ///
    /// The model is asked whether or not it is enabled in the app. Its API key
    /// is read from the provider's environment variable (exported in the shell,
    /// or in a .env file in the current folder), else from the key added in the
    /// app under Prompts > AI Models. The variable of each provider:
    ///
    /// Gemini: GEMINI_API_KEY
    /// OpenRouter: OPENROUTER_API_KEY
    /// Mistral: MISTRAL_API_KEY
    /// SambaNova: SAMBANOVA_API_KEY
    /// NvidiaNim: NVIDIA_API_KEY
    /// Anthropic: ANTHROPIC_API_KEY
    /// OpenAI: OPENAI_API_KEY
    /// DeepSeek: DEEPSEEK_API_KEY
    /// xAI: XAI_API_KEY
    /// HuggingFace: HUGGINGFACE_API_KEY
    /// Perplexity: PERPLEXITY_API_KEY
    #[arg(long, value_name = "MODEL", conflicts_with = "no_ai", verbatim_doc_comment)]
    pub model_name: Option<String>,

    /// The provider of --model-name (`Gemini`, `OpenRouter`, …); needed only
    /// for a model not in the app's model lists, or listed by several
    #[arg(long, value_name = "PROVIDER", requires = "model_name")]
    pub provider: Option<String>,

    /// Let the AI choose word senses, but make no translation
    #[arg(long)]
    pub no_translate: bool,

    /// Ask the AI for a contextual meaning for every word, not only for the
    /// words with several senses or readings (more tokens per request);
    /// default: the app's "Contextual meaning for every word" setting.
    /// Word in Sentence only
    #[arg(long, overrides_with = "no_contextual_all")]
    pub contextual_all: bool,

    /// Ask for contextual meanings only for the words with a choice to make
    #[arg(long, overrides_with = "contextual_all")]
    pub no_contextual_all: bool,

    /// One exact DPD form lookup per word, in order, instead of the
    /// dictionary options (`/gloss_text` with `"exact": true`); Headwords only
    #[arg(long)]
    pub exact: bool,

    /// List a word only the first time it occurs in the text
    #[arg(long)]
    pub no_duplicates_globally: bool,

    /// Leave out the words in --common-words
    #[arg(long)]
    pub skip_common: bool,

    /// Comma-separated common words for --skip-common
    #[arg(long, value_delimiter = ',', value_name = "WORDS")]
    pub common_words: Option<Vec<String>>,
}

impl GlossArgs {
    /// The `/gloss_text` options these flags stand for. A flag left off is
    /// `None`, as an option left out of the request body is.
    fn options(&self) -> GlossTextOptions {
        GlossTextOptions {
            no_duplicates_globally: self.no_duplicates_globally.then_some(true),
            skip_common: self.skip_common.then_some(true),
            common_words: self.common_words.clone(),
            exact: self.exact.then_some(true),
            ..Default::default()
        }
    }

    /// The lines per word of the Word in Sentence tables: 1 or 2 (default).
    fn lines_per_word(&self) -> u8 {
        self.lines_per_word.unwrap_or(2)
    }

    /// `--contextual-all` / `--no-contextual-all` when given, the last one
    /// winning; `None` means the app setting.
    fn contextual_all_flag(&self) -> Option<bool> {
        if self.contextual_all {
            Some(true)
        } else if self.no_contextual_all {
            Some(false)
        } else {
            None
        }
    }

    fn gloss_format(&self) -> GlossFormat {
        self.gloss_format.map(GlossFormatArg::gloss_format).unwrap_or(GlossFormat::WordInSentence)
    }

    /// The output format, from `--format`, the `--output` extension, or
    /// whether stdout is a terminal.
    fn output_format(&self, stdout_is_terminal: bool) -> OutputFormatArg {
        if let Some(f) = self.format {
            return f;
        }
        if let Some(f) = self.output.as_deref().and_then(OutputFormatArg::from_extension) {
            return f;
        }
        if self.output.is_none() && stdout_is_terminal {
            OutputFormatArg::Text
        } else {
            OutputFormatArg::Markdown
        }
    }

    /// The combinations the command refuses.
    fn validate(&self, json: bool) -> Result<(), CliError> {
        if json && self.format.is_some() {
            return Err(CliError::error(
                "--json prints the gloss result; --format json is the session export. Give one of them.",
            ));
        }
        if json && self.output.is_some() {
            return Err(CliError::error("--json prints to stdout; use --format with --output"));
        }
        if self.exact && self.model_name.is_some() {
            return Err(CliError::error("--exact makes no AI request; leave out --model-name"));
        }
        if self.exact {
            if self.gloss_format == Some(GlossFormatArg::WordInSentence) {
                return Err(CliError::error("--exact needs --gloss-format headwords"));
            }
            if self.format.is_some_and(|f| f != OutputFormatArg::Text) || self.output.is_some() {
                return Err(CliError::error("--exact prints text or --json only"));
            }
        }
        if self.lines_per_word.is_some() {
            if json || self.exact {
                return Err(CliError::error("--lines-per-word is for the Word in Sentence tables; leave it out with --json and --exact"));
            }
            if self.gloss_format == Some(GlossFormatArg::Headwords) {
                return Err(CliError::error("--lines-per-word needs --gloss-format word-in-sentence"));
            }
            let format = self.format.or_else(|| self.output.as_deref().and_then(OutputFormatArg::from_extension));
            if matches!(format, Some(OutputFormatArg::AnkiCsv | OutputFormatArg::Json)) {
                return Err(CliError::error(
                    "--lines-per-word applies to --format text, markdown, html, org and docx; anki-csv and json have one record per word",
                ));
            }
        }
        if self.contextual_all_flag().is_some() {
            if self.no_ai || self.exact {
                return Err(CliError::error(
                    "--contextual-all and --no-contextual-all choose what the AI is asked; leave them out with --no-ai and --exact",
                ));
            }
            if self.gloss_format == Some(GlossFormatArg::Headwords) {
                return Err(CliError::error("--contextual-all and --no-contextual-all need --gloss-format word-in-sentence"));
            }
        }
        if self.format == Some(OutputFormatArg::Text) && self.output.is_some() {
            return Err(CliError::error("--format text is for the terminal; write a file with --format markdown"));
        }
        if self.format == Some(OutputFormatArg::Docx) && self.output.is_none() {
            return Err(CliError::error("--format docx needs --output <path>"));
        }
        Ok(())
    }
}

/// The paragraphs to gloss, the document title, and the record a reference
/// resolved to (`None` for text given directly): a reference's passage, or the
/// text given.
fn gloss_input(args: &GlossArgs) -> Result<(Vec<String>, String, Option<String>), CliError> {
    let joined = args.text.join(" ");
    let is_reference = !(args.text.len() == 1 && args.text[0] == "-") && is_sutta_reference(&joined);
    if !is_reference {
        if args.edition.is_some() || args.paragraphs.is_some() {
            return Err(CliError::error("--edition and --paragraphs need a sutta reference"));
        }
        let text = text_or_stdin(&args.text)?;
        return Ok((split_gloss_paragraphs(&text), String::new(), None));
    }
    let edition = match args.edition {
        Some(EditionArg::Cst) => PaliEdition::Cst,
        Some(EditionArg::Ms) => PaliEdition::Ms,
        None => PaliEdition::Default,
    };
    let source = resolve_gloss_source(&get_app_data().dbm.appdata, &joined, edition, args.paragraphs.as_deref())
        .map_err(|e| match e {
            GlossSourceError::BadRequest(m) => CliError::error(m),
            GlossSourceError::NotFound(m) => CliError::not_found(m),
        })?;
    let title = source.title();
    Ok((source.paragraphs, title, Some(source.uid)))
}

/// Requires the preamble to have run. `can_write` is false while the GUI or
/// another `simsapa` command is using the data folder: the AI's sense
/// choices are then used but not saved.
pub fn run_gloss(args: &GlossArgs, json: bool, can_write: bool) -> CliResult {
    args.validate(json)?;
    let (paragraphs, title, source_uid) = gloss_input(args)?;
    let opts = args.options();
    let dbm = &get_app_data().dbm;

    if args.exact {
        validate_gloss_exact_options(opts.no_duplicates_globally, opts.skip_common, opts.common_words.as_ref())
            .map_err(CliError::error)?;
        let result = process_all_paragraphs_exact(&paragraphs, &dbm.dpd).map_err(CliError::error)?;
        if json {
            return print_json(&GlossTextResponse::Exact(result));
        }
        print_exact_text(&result);
        return Ok(());
    }

    // Before the glossing: a misspelt model name fails at once.
    let models = if args.no_ai { None } else { resolve_model(args)? };
    let no_ai_provider = models.is_none() && !get_app_data().has_enabled_ai_model();

    let gloss_format = args.gloss_format();
    let input = AllParagraphsProcessingInput {
        paragraphs: paragraphs.clone(),
        options: gloss_word_processing_options(&opts, gloss_format),
    };
    let mut result = process_all_paragraphs(&input, &dbm.appdata, &dbm.dpd)
        .map_err(|e| CliError::error(format!("Glossing failed: {}", e)))?;

    let ai_run = run_ai(args, models, &mut result, &paragraphs, source_uid, can_write)?;
    let ai_ran = ai_run == AiRun::Asked;
    // A run with no API key made no AI choice either: the document says so.
    let no_ai_provider = no_ai_provider || ai_run == AiRun::NoApiKey;

    if json {
        print_json(&GlossTextResponse::Default(result.clone()))?;
    } else {
        write_output(args, &result, &paragraphs, &title, gloss_format, no_ai_provider)?;
    }

    if ai_ran && !result.ai_errors.is_empty() {
        for e in &result.ai_errors {
            errln!("AI request failed: {}", e);
        }
        errln!("The document was made without the failed AI steps: senses marked N were not checked.");
        return Err(CliError::reported(AI_FAILED));
    }
    Ok(())
}

/// The `--model-name` model as the one entry to ask, `None` without it. Its
/// provider is `--provider`, or the provider whose model list has it.
fn resolve_model(args: &GlossArgs) -> Result<Option<Vec<ModelUsageEntry>>, CliError> {
    let Some(model_name) = args.model_name.as_deref().map(str::trim) else {
        return Ok(None);
    };
    if model_name.is_empty() {
        return Err(CliError::error("--model-name is empty"));
    }
    let app_data = get_app_data();
    let provider = match args.provider.as_deref() {
        Some(p) => provider_from_arg(p)?,
        None => {
            let listing = app_data.providers_listing_model(model_name);
            let with_key: Vec<&String> =
                listing.iter().filter(|p| !app_data.get_provider_api_key(p).is_empty()).collect();
            match (listing.as_slice(), with_key.as_slice()) {
                ([], _) => {
                    return Err(CliError::error(format!(
                        "No provider in the Simsapa app lists the model '{}'. Name its provider with --provider ({}).",
                        model_name,
                        provider_names()
                    )));
                }
                ([one], _) => one.clone(),
                (_, [one]) => (*one).clone(),
                (many, _) => {
                    return Err(CliError::error(format!(
                        "The model '{}' is listed by several providers ({}). Choose one with --provider.",
                        model_name,
                        many.join(", ")
                    )));
                }
            }
        }
    };
    Ok(Some(vec![ModelUsageEntry { provider, model_name: model_name.to_string(), enabled: true }]))
}

/// The canonical provider name of a `--provider` value, in any letter case.
fn provider_from_arg(name: &str) -> Result<String, CliError> {
    let name = name.trim();
    ProviderName::ALL
        .iter()
        .find(|p| p.as_str().eq_ignore_ascii_case(name) || format!("{:?}", p).eq_ignore_ascii_case(name))
        .map(|p| p.as_str().to_string())
        .ok_or_else(|| CliError::error(format!("Unknown provider '{}'. One of: {}", name, provider_names())))
}

fn provider_names() -> String {
    ProviderName::ALL.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(", ")
}

/// The canonical provider names of `entries`, in order, each once. A saved
/// entry may hold the legacy (Debug) spelling.
fn providers_of(entries: &[ModelUsageEntry]) -> Vec<String> {
    let mut providers: Vec<String> = Vec::new();
    for e in entries {
        let provider = ProviderName::from_canonical_or_legacy(&e.provider)
            .map(|p| p.as_str().to_string())
            .unwrap_or_else(|| e.provider.clone());
        if !providers.contains(&provider) {
            providers.push(provider);
        }
    }
    providers
}

/// The providers of `entries` that have no API key, in order, each once.
fn providers_without_key(entries: &[ModelUsageEntry]) -> Vec<String> {
    let app_data = get_app_data();
    providers_of(entries).into_iter().filter(|p| app_data.get_provider_api_key(p).is_empty()).collect()
}

/// Where an API key for `providers` can come from.
fn api_key_help(providers: &[String]) -> String {
    let app_data = get_app_data();
    let vars: Vec<String> = providers
        .iter()
        .map(|p| app_data.get_provider_api_key_env_var_name(p))
        .filter(|v| !v.is_empty())
        .collect();
    let env = if vars.is_empty() {
        "in the provider's API key environment variable".to_string()
    } else {
        format!("in the environment variable {}", vars.join(" or "))
    };
    format!(
        "Add the key in the Simsapa app under Prompts > AI Models, or give it {} \
         (exported in the shell, or in a .env file in the current folder).",
        env
    )
}

/// What `run_ai()` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiRun {
    /// The AI was asked.
    Asked,
    /// Nothing to ask, or no model to ask.
    NotAsked,
    /// No model to try has an API key.
    NoApiKey,
}

/// Run the AI steps the flags and the app settings ask for: word selection
/// when it is turned on in the Gloss tab's Word Selection dialog, and the
/// translation unless `--no-translate`. `models` (from `--model-name`) are
/// asked in place of the app's Fallback sequence.
fn run_ai(
    args: &GlossArgs,
    models: Option<Vec<ModelUsageEntry>>,
    result: &mut AllParagraphsProcessingResult,
    paragraphs: &[String],
    source_uid: Option<String>,
    can_write: bool,
) -> Result<AiRun, CliError> {
    if args.no_ai || !result.success || result.paragraphs.is_empty() {
        return Ok(AiRun::NotAsked);
    }
    let (word_selection, contextual_all_setting) = {
        let settings = get_app_data().app_settings_cache.read().expect("Failed to read app settings");
        (settings.gloss_word_selection_enabled, settings.gloss_contextual_all)
    };
    let contextual_all = args.contextual_all_flag().unwrap_or(contextual_all_setting);
    if !word_selection && args.no_translate {
        return Ok(AiRun::NotAsked);
    }
    if models.is_none() && !ai_available() {
        errln!(
            "Note: no AI model is enabled in the Simsapa app, so no AI request is made. \
             Enable one in the app under Prompts > AI Models, or name one with --model-name \
             (e.g. --model-name gemini-3-flash-preview) and give its API key in an environment variable \
             such as GEMINI_API_KEY (exported in the shell, or in a .env file in the current folder). \
             --no-ai skips this check."
        );
        return Ok(AiRun::NotAsked);
    }
    // The models the engine tries: with auto-fallback off, only the first.
    let (entries, auto_fallback, _) = walk_settings_for(models.as_deref());
    let mut tried: Vec<ModelUsageEntry> = entries.into_iter().filter(|e| e.enabled).collect();
    if !auto_fallback {
        tried.truncate(1);
    }
    let missing = providers_without_key(&tried);
    if !tried.is_empty() && missing.len() == providers_of(&tried).len() {
        errln!("Note: no API key for {}, so no AI request is made.", missing.join(", "));
        errln!("{}", api_key_help(&missing));
        return Ok(AiRun::NoApiKey);
    }
    if !missing.is_empty() {
        errln!("Note: no API key for {}, so the requests to its models fail.", missing.join(", "));
        errln!("{}", api_key_help(&missing));
    }
    let options = GlossAiOptions {
        word_selection,
        translate: !args.no_translate,
        write_cache: can_write,
        models,
        contextual_all,
        source_uid,
    };
    if options.word_selection && !can_write {
        errln!(
            "Note: the data folder is in use by the Simsapa app or another simsapa command, \
             so the AI's word choices are used but not saved."
        );
    }

    let cancel = install_cancel_flag();
    let dbm = &get_app_data().dbm;
    let mut on_progress = |p: GlossAiProgress| errln!("{}", progress_line(&p));
    let summary = run_gloss_ai(
        result,
        paragraphs,
        options,
        &dbm.appdata,
        &dbm.dpd,
        &|| cancel.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)),
        &mut on_progress,
    );
    if cancel.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
        return Err(CliError::interrupted_with("Interrupted."));
    }
    errln!(
        "AI: {} word sense(s) chosen, {} paragraph(s) translated.",
        summary.senses_applied, summary.paragraphs_translated
    );
    Ok(AiRun::Asked)
}

/// A Ctrl-C flag for the AI requests, which can wait for minutes. `None`
/// when the handler cannot be installed: Ctrl-C then quits at once.
fn install_cancel_flag() -> Option<Arc<AtomicBool>> {
    crate::cancel::install_ctrlc_handler().ok()
}

/// One stderr line of AI progress.
fn progress_line(p: &GlossAiProgress) -> String {
    let stage = match p.stage {
        GlossAiStage::WordSelection { batch, batch_count } if batch_count > 1 => {
            format!("word selection, request {} of {}", batch + 1, batch_count)
        }
        GlossAiStage::WordSelection { .. } => "word selection".to_string(),
        GlossAiStage::Translation { paragraph, paragraph_count } if paragraph_count > 1 => {
            format!("translation, paragraph {} of {}", paragraph + 1, paragraph_count)
        }
        GlossAiStage::Translation { .. } => "translation".to_string(),
    };
    if p.kind == "trying" {
        format!("Waiting for AI response ({}): {}…", p.model, stage)
    } else {
        format!("{}: {}", stage, p.message)
    }
}

// --- Output -----------------------------------------------------------------

/// Write the result in the requested format, to `--output` or stdout.
fn write_output(
    args: &GlossArgs,
    result: &AllParagraphsProcessingResult,
    paragraphs: &[String],
    title: &str,
    gloss_format: GlossFormat,
    no_ai_provider: bool,
) -> CliResult {
    let format = args.output_format(stdout_is_tty());
    let output = args.output.as_deref();
    match format {
        OutputFormatArg::Text => {
            match gloss_format {
                GlossFormat::Headwords => print_gloss_text(result),
                GlossFormat::WordInSentence => print_rows_text(result, title, no_ai_provider, args.lines_per_word() == 2),
            }
            Ok(())
        }
        OutputFormatArg::Markdown | OutputFormatArg::Html | OutputFormatArg::Org => {
            let text_format = match format {
                OutputFormatArg::Html => TextFormat::Html,
                OutputFormatArg::Org => TextFormat::OrgMode,
                _ => TextFormat::Markdown,
            };
            let data = export_data(args, result, paragraphs, title, gloss_format, no_ai_provider);
            emit_text(output, &gloss_export_document(&data, text_format))
        }
        OutputFormatArg::Docx => {
            let path = output.ok_or_else(|| CliError::error("--format docx needs --output <path>"))?;
            let data = export_data(args, result, paragraphs, title, gloss_format, no_ai_provider);
            let bytes = generate_gloss_docx(&serde_json::to_string(&data)?)?;
            write_file(path, &bytes)
        }
        OutputFormatArg::AnkiCsv => {
            let data = export_data(args, result, paragraphs, title, gloss_format, no_ai_provider);
            write_anki_csv(output, &data)
        }
        OutputFormatArg::Json => {
            let session = session_export_json(args, result, paragraphs, title, gloss_format, no_ai_provider)?;
            emit_text(output, &session)
        }
    }
}

/// The export data of the result. `no_ai_provider` replaces the app
/// settings' answer: a `--model-name` run has a model the settings may not.
fn export_data(
    args: &GlossArgs,
    result: &AllParagraphsProcessingResult,
    paragraphs: &[String],
    title: &str,
    gloss_format: GlossFormat,
    no_ai_provider: bool,
) -> GlossExportData {
    let mut data = gloss_result_export_data(result, paragraphs, title, gloss_format, args.lines_per_word());
    data.no_ai_provider = no_ai_provider;
    data
}

/// The `simsapa-gloss-session` export of the result, which the app's Gloss
/// tab opens with "Open JSON".
fn session_export_json(
    args: &GlossArgs,
    result: &AllParagraphsProcessingResult,
    paragraphs: &[String],
    title: &str,
    gloss_format: GlossFormat,
    no_ai_provider: bool,
) -> Result<String, CliError> {
    let options = GlossExportOptions {
        text: paragraphs.join("\n\n"),
        title: title.to_string(),
        gloss_format,
        no_ai_provider,
        lines_per_word: args.lines_per_word(),
    };
    let translations = result_translations(result, paragraphs.len());
    let session = build_gloss_session_json(
        result,
        paragraphs,
        &translations,
        &options,
        args.no_duplicates_globally,
        args.skip_common,
    );
    let dbm = &get_app_data().dbm;
    build_gloss_session_export_json(&dbm.appdata, Some(&dbm.dpd), &serde_json::to_string(&session)?)
        .map_err(CliError::error)
}

/// The Anki CSV with the app's Anki export settings. The cloze file, when
/// the settings ask for one, goes next to `--output` (`<name>_cloze.csv`);
/// on stdout only the main file is printed.
fn write_anki_csv(output: Option<&Path>, data: &GlossExportData) -> CliResult {
    let app_data = get_app_data();
    let input = AnkiCsvExportInput {
        gloss_data_json: serde_json::to_string(data)?,
        export_format: app_data.get_anki_export_format(),
        include_cloze: app_data.get_anki_include_cloze(),
        templates: AnkiCsvTemplates {
            front: app_data.get_anki_template_front(),
            back: app_data.get_anki_template_back(),
            cloze_front: app_data.get_anki_template_cloze_front(),
            cloze_back: app_data.get_anki_template_cloze_back(),
        },
    };
    let result = export_anki_csv(input, app_data)?;
    let mut files = result.files.into_iter();
    let Some(main) = files.next() else {
        return Err(CliError::error(result.error.unwrap_or_else(|| "The Anki export made no file".to_string())));
    };
    emit_text(output, &main.content)?;
    for extra in files {
        match output {
            Some(path) => {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("gloss");
                let suffix = if extra.filename.contains("_cloze") { "_cloze" } else { "_2" };
                let extra_path = path.with_file_name(format!("{}{}.csv", stem, suffix));
                write_file(&extra_path, extra.content.as_bytes())?;
                errln!("Wrote {}", extra_path.display());
            }
            None => errln!("Note: the cloze CSV ({}) is written only with --output.", extra.filename),
        }
    }
    Ok(())
}

/// Print `text` on stdout, or write it to `output`.
fn emit_text(output: Option<&Path>, text: &str) -> CliResult {
    match output {
        Some(path) => write_file(path, text.as_bytes()),
        None => {
            if text.ends_with('\n') {
                out!("{}", text);
            } else {
                outln!("{}", text);
            }
            Ok(())
        }
    }
}

fn write_file(path: &Path, bytes: &[u8]) -> CliResult {
    std::fs::write(path, bytes).map_err(|e| CliError::error(format!("Cannot write {}: {}", path.display(), e)))
}

/// The sense the text output shows: the selected one (the first, unless the
/// built-in word-selection data chose another), else the first.
fn shown_option(w: &ProcessedWord) -> Option<&simsapa_backend::db::dpd::LookupResult> {
    usize::try_from(w.selected_index)
        .ok()
        .and_then(|i| w.results.get(i))
        .or_else(|| w.results.first())
}

fn print_gloss_text(result: &AllParagraphsProcessingResult) {
    let is_tty = stdout_is_tty();
    let many = result.paragraphs.len() > 1;

    for (n, p) in result.paragraphs.iter().enumerate() {
        if many {
            if n > 0 {
                outln!();
            }
            outln!("Paragraph {}", p.paragraph_index + 1);
        }
        let width = p.words_data.iter().map(|w| w.original_word.chars().count()).max().unwrap_or(0);
        for w in &p.words_data {
            let meaning = shown_option(w)
                .map(|r| snippet_to_terminal(&r.summary, is_tty))
                .unwrap_or_default();
            let pad = width.saturating_sub(w.original_word.chars().count());
            let mut line = format!("{}{}  {}", w.original_word, " ".repeat(pad), meaning);
            if !w.deconstructions.is_empty() {
                let splits: Vec<&str> = w.deconstructions.iter().map(|d| d.words_joined.as_str()).collect();
                line.push_str(&format!("  [{}]", splits.join(" | ")));
            }
            outln!("{}", line.trim_end());
        }
        print_translations_text(p.ai_translations.as_slice());
        if !p.unrecognized_words.is_empty() {
            outln!("Unrecognized: {}", p.unrecognized_words.join(", "));
        }
    }
}

/// The text cells of a row and of its components, in order:
/// `[word, conf, grammar, contextual, definition, construction, root]`, the
/// construction in square brackets.
fn row_cells(row: &GlossRow, out: &mut Vec<[String; 7]>) {
    let word = format!("{}{}", "- ".repeat(row.depth as usize), row.word);
    let conf = row.confidence.map(|c| c.as_str().to_string()).unwrap_or_default();
    let construction = row.construction.trim();
    let construction = if construction.is_empty() { String::new() } else { format!("[{}]", construction) };
    out.push([
        word,
        conf,
        row.grammar.clone(),
        row.contextual.clone(),
        row.definition.clone(),
        construction,
        row.root.clone(),
    ]);
    for c in &row.components {
        row_cells(c, out);
    }
}

/// The cells of `row_cells()` as aligned text lines.
///
/// One line per word: word, confidence mark, grammar, the contextual
/// meaning, the definition; the contextual column is left out when no row
/// has a value in it.
///
/// Two lines per word: word, mark, the contextual meaning and the
/// definition; under them the grammar (under the contextual column) and the
/// construction and root (under the definition). A second line with nothing
/// on it is left out.
fn rows_text_lines(cells: &[[String; 7]], two_lines: bool) -> Vec<String> {
    let width = |i: usize| cells.iter().map(|c| c[i].chars().count()).max().unwrap_or(0);
    let pad = |text: &str, w: usize| format!("{}{}", text, " ".repeat(w - text.chars().count()));
    let word_w = width(0);
    let mut lines = Vec::new();
    if !two_lines {
        let (grammar_w, contextual_w) = (width(2), width(3));
        for c in cells {
            let mut line = format!("{}  {:1}  {}  ", pad(&c[0], word_w), c[1], pad(&c[2], grammar_w));
            if contextual_w > 0 {
                line.push_str(&format!("{}  ", pad(&c[3], contextual_w)));
            }
            line.push_str(&c[4]);
            lines.push(line.trim_end().to_string());
        }
        return lines;
    }
    // The column of the contextual meaning over the grammar.
    let middle_w = width(2).max(width(3));
    let middle = |text: &str| if middle_w > 0 { format!("{}  ", pad(text, middle_w)) } else { String::new() };
    for c in cells {
        let line1 = format!("{}  {:1}  {}{}", pad(&c[0], word_w), c[1], middle(&c[3]), c[4]);
        lines.push(line1.trim_end().to_string());
        let derivation: Vec<&str> = [c[5].as_str(), c[6].as_str()].into_iter().filter(|s| !s.is_empty()).collect();
        if c[2].is_empty() && derivation.is_empty() {
            continue;
        }
        let line2 = format!("{}  {:1}  {}{}", pad("", word_w), "", middle(&c[2]), derivation.join("  "));
        lines.push(line2.trim_end().to_string());
    }
    lines
}

/// The Word in Sentence result as aligned columns (`rows_text_lines()`), on
/// one or two lines per word; then the translations and the legend.
fn print_rows_text(result: &AllParagraphsProcessingResult, title: &str, no_ai_provider: bool, two_lines: bool) {
    if !title.is_empty() {
        outln!("{}", title);
        outln!();
    }
    let many = result.paragraphs.len() > 1;
    let mut any_conf = false;
    for (n, p) in result.paragraphs.iter().enumerate() {
        if many {
            if n > 0 {
                outln!();
            }
            outln!("Paragraph {}", p.paragraph_index + 1);
        }
        let mut cells = Vec::new();
        for row in &p.rows {
            row_cells(row, &mut cells);
        }
        any_conf |= cells.iter().any(|c| !c[1].is_empty());
        for line in rows_text_lines(&cells, two_lines) {
            outln!("{}", line);
        }
        if !p.variants.is_empty() {
            let variants: Vec<String> = p.variants.iter().map(|v| v.join("//")).collect();
            outln!("Variants: {}", variants.join(", "));
        }
        print_translations_text(p.ai_translations.as_slice());
        if !p.unrecognized_words.is_empty() {
            outln!("Unrecognized: {}", p.unrecognized_words.join(", "));
        }
    }
    if any_conf {
        outln!();
        for line in CONFIDENCE_LEGEND {
            outln!("{}", line);
        }
        if no_ai_provider {
            outln!("{}", NO_AI_PROVIDER_NOTE);
        }
    }
}

fn print_translations_text(translations: &[simsapa_backend::export_types::AiResponse]) {
    for t in translations {
        outln!();
        outln!("Translation ({}): {}", t.model_name, t.response.trim());
        if !t.literal_translation.trim().is_empty() {
            outln!("Literal Translation: {}", t.literal_translation.trim());
        }
    }
}

fn status_label(status: &FormLookupStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{:?}", status))
}

/// The status, with `(inflection table)` when the headwords come from DPD's
/// inflection tables rather than its lookup row.
fn result_status_label(r: &FormLookupResult) -> String {
    match r.source {
        Some(FormResultSource::InflectionTable) => format!("{} (inflection table)", status_label(&r.status)),
        None => status_label(&r.status),
    }
}

fn splits_text(r: &FormLookupResult) -> String {
    r.deconstructions.iter().map(|d| d.split.as_str()).collect::<Vec<_>>().join(" | ")
}

fn print_exact_text(result: &ExactGlossResult) {
    let many = result.paragraphs.len() > 1;
    for (n, p) in result.paragraphs.iter().enumerate() {
        if many {
            if n > 0 {
                outln!();
            }
            outln!("Paragraph {}", p.paragraph_index + 1);
        }
        for t in &p.tokens {
            let mut line = format!(
                "{:>3}  {}  {}  {} headword{}",
                t.token_index,
                t.original_word,
                result_status_label(&t.lookup),
                t.lookup.headwords.len(),
                if t.lookup.headwords.len() == 1 { "" } else { "s" },
            );
            let splits = splits_text(&t.lookup);
            if !splits.is_empty() {
                line.push_str(&format!("  [{}]", splits));
            }
            outln!("{}", line);
        }
    }
}

#[derive(Args, Debug)]
pub struct LookupArgs {
    /// The forms to look up, or `-` to read them from stdin, one per line
    /// (an empty line is looked up too, and answers `invalid`)
    #[arg(required = true, num_args = 1..)]
    pub forms: Vec<String>,
}

/// The forms these arguments stand for.
fn lookup_forms_input(forms: &[String]) -> Result<Vec<String>, CliError> {
    if forms.len() == 1 && forms[0] == "-" {
        let text = text_or_stdin(forms)?;
        Ok(text.lines().map(str::to_string).collect())
    } else {
        Ok(forms.to_vec())
    }
}

/// Requires the preamble to have run.
pub fn run_lookup(args: &LookupArgs, json: bool) -> CliResult {
    let forms = lookup_forms_input(&args.forms)?;
    validate_lookup_forms_request(Some(&forms), None).map_err(CliError::error)?;

    let dpd = &get_app_data().dbm.dpd;
    let results = dpd.lookup_forms_exact(&forms)?;

    if json {
        return print_json(&LookupFormsResponse {
            dpd_version: dpd.dpd_release_version(),
            results,
        });
    }

    for r in &results {
        let lemmas: Vec<&str> = r.headwords.iter().map(|h| h.lemma_1.as_str()).collect();
        let mut line = format!("{}  {}  {}", r.input, r.lookup_key, result_status_label(r));
        if !lemmas.is_empty() {
            line.push_str(&format!("  {}", lemmas.join(", ")));
        }
        let splits = splits_text(r);
        if !splits.is_empty() {
            line.push_str(&format!("  [{}]", splits));
        }
        outln!("{}", line);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_cells(rows: &[[&str; 7]]) -> Vec<[String; 7]> {
        rows.iter().map(|r| r.map(String::from)).collect()
    }

    #[test]
    fn test_rows_text_lines_two_lines() {
        let cells = text_cells(&[
            ["uppannaṁ", "A", "nt nom sg of uppanna", "arisen", "arisen; appeared", "[ud + √pad + na]", "√pad 3 ya"],
            ["- ud", "", "prefix", "", "up", "", ""],
            ["xyz", "", "", "", "not found", "", ""],
        ]);
        let middle = "nt nom sg of uppanna".len() + 2;
        assert_eq!(
            rows_text_lines(&cells, true),
            vec![
                format!("uppannaṁ  A  {}arisen; appeared", format!("{:middle$}", "arisen")),
                format!("{}nt nom sg of uppanna  [ud + √pad + na]  √pad 3 ya", " ".repeat(8 + 2 + 1 + 2)),
                format!("- ud{}up", " ".repeat(4 + 2 + 1 + 2 + middle)),
                format!("{}prefix", " ".repeat(8 + 2 + 1 + 2)),
                // No grammar, construction or root: no second line.
                format!("xyz{}not found", " ".repeat(5 + 2 + 1 + 2 + middle)),
            ]
        );
    }

    #[test]
    fn test_lines_per_word_validation() {
        let mut a = args("x");
        a.lines_per_word = Some(1);
        assert!(a.validate(false).is_ok());
        assert!(a.validate(true).is_err(), "--json");
        for format in [OutputFormatArg::AnkiCsv, OutputFormatArg::Json] {
            a.format = Some(format);
            let err = a.validate(false).unwrap_err().to_string();
            assert!(err.contains("one record per word"), "{err}");
        }
        a.format = None;
        a.output = Some("gloss.csv".into());
        assert!(a.validate(false).is_err(), "a .csv output is the Anki CSV");
        a.output = Some("gloss.docx".into());
        assert!(a.validate(false).is_ok());
        a.output = None;
        a.gloss_format = Some(GlossFormatArg::Headwords);
        assert!(a.validate(false).unwrap_err().to_string().contains("word-in-sentence"));
        a.gloss_format = None;
        a.exact = true;
        assert!(a.validate(false).is_err(), "--exact");
    }

    #[test]
    fn test_contextual_all_validation_and_parsing() {
        use clap::Parser;
        #[derive(clap::Parser)]
        struct Wrapper {
            #[command(flatten)]
            gloss: GlossArgs,
        }
        let flag = |argv: &[&str]| Wrapper::try_parse_from(argv).unwrap().gloss.contextual_all_flag();
        assert_eq!(flag(&["gloss", "x"]), None);
        assert_eq!(flag(&["gloss", "--contextual-all", "x"]), Some(true));
        assert_eq!(flag(&["gloss", "--no-contextual-all", "x"]), Some(false));
        // The last one wins.
        assert_eq!(flag(&["gloss", "--contextual-all", "--no-contextual-all", "x"]), Some(false));
        assert_eq!(flag(&["gloss", "--no-contextual-all", "--contextual-all", "x"]), Some(true));

        let mut a = args("x");
        a.contextual_all = true;
        assert!(a.validate(false).is_ok());
        assert!(a.validate(true).is_ok(), "--json shows the AI's answers too");
        a.no_ai = true;
        assert!(a.validate(false).unwrap_err().to_string().contains("--no-ai"));
        a.no_ai = false;
        a.exact = true;
        assert!(a.validate(false).unwrap_err().to_string().contains("--exact"));
        a.exact = false;
        a.gloss_format = Some(GlossFormatArg::Headwords);
        assert!(a.validate(false).unwrap_err().to_string().contains("word-in-sentence"));
        a.contextual_all = false;
        a.no_contextual_all = true;
        assert!(a.validate(false).unwrap_err().to_string().contains("word-in-sentence"));
    }

    #[test]
    fn test_lines_per_word_parses_one_or_two() {
        use clap::Parser;
        #[derive(clap::Parser)]
        struct Wrapper {
            #[command(flatten)]
            gloss: GlossArgs,
        }
        let parsed = Wrapper::try_parse_from(["gloss", "--lines-per-word", "1", "x"]).unwrap();
        assert_eq!(parsed.gloss.lines_per_word(), 1);
        let parsed = Wrapper::try_parse_from(["gloss", "x"]).unwrap();
        assert_eq!(parsed.gloss.lines_per_word(), 2);
        assert!(Wrapper::try_parse_from(["gloss", "--lines-per-word", "3", "x"]).is_err());
    }

    #[test]
    fn test_rows_text_lines_align_the_contextual_column() {
        let cells = text_cells(&[
            ["uppannaṁ", "A", "nt nom sg of uppanna", "arisen", "arisen; appeared", "", ""],
            ["te", "", "masc nom pl of ta", "", "those", "", ""],
        ]);
        assert_eq!(
            rows_text_lines(&cells, false),
            vec![
                "uppannaṁ  A  nt nom sg of uppanna  arisen  arisen; appeared".to_string(),
                format!("te{}masc nom pl of ta{}those", " ".repeat(6 + 2 + 1 + 2), " ".repeat(3 + 2 + 6 + 2)),
            ]
        );
    }

    #[test]
    fn test_rows_text_lines_without_contextual_values_leave_the_column_out() {
        let cells = text_cells(&[
            ["te", "N", "masc nom pl of ta", "", "those", "", ""],
            ["xyz", "", "", "", "not found", "", ""],
        ]);
        assert_eq!(
            rows_text_lines(&cells, false),
            vec!["te   N  masc nom pl of ta  those".to_string(), format!("xyz{}not found", " ".repeat(2 + 1 + 2 + 17 + 2))]
        );
    }

    #[test]
    fn test_flags_left_off_are_none() {
        let args = GlossArgs { text: vec!["x".into()], exact: true, ..Default::default() };
        let o = args.options();
        assert_eq!(o.exact, Some(true));
        assert_eq!(o.no_duplicates_globally, None);
        assert_eq!(o.skip_common, None);
        assert!(validate_gloss_exact_options(o.no_duplicates_globally, o.skip_common, o.common_words.as_ref()).is_ok());
    }

    #[test]
    fn test_exact_rejects_skip_common() {
        let args = GlossArgs { text: vec!["x".into()], exact: true, skip_common: true, ..Default::default() };
        let o = args.options();
        assert!(validate_gloss_exact_options(o.no_duplicates_globally, o.skip_common, o.common_words.as_ref()).is_err());
    }

    fn args(text: &str) -> GlossArgs {
        GlossArgs { text: vec![text.into()], ..Default::default() }
    }

    #[test]
    fn test_flag_combinations() {
        let mut a = args("x");
        a.exact = true;
        assert!(a.validate(false).is_ok());
        a.gloss_format = Some(GlossFormatArg::WordInSentence);
        assert!(a.validate(false).is_err());
        a.gloss_format = Some(GlossFormatArg::Headwords);
        a.format = Some(OutputFormatArg::Markdown);
        assert!(a.validate(false).is_err());

        let mut a = args("x");
        a.format = Some(OutputFormatArg::Docx);
        assert!(a.validate(false).is_err());
        a.output = Some("a.docx".into());
        assert!(a.validate(false).is_ok());
        assert!(a.validate(true).is_err(), "--json with --format");

        let mut a = args("x");
        a.no_ai = true;
        a.no_translate = true;
        assert!(a.validate(false).is_ok());
    }

    #[test]
    fn test_model_name_flags() {
        let mut a = args("x");
        a.model_name = Some("gemini-3-flash-preview".into());
        assert!(a.validate(false).is_ok());
        a.exact = true;
        assert!(a.validate(false).is_err(), "--exact makes no AI request");
    }

    /// The `--model-name` help and the bundled doc name every provider's API
    /// key variable.
    #[test]
    fn test_model_name_help_lists_api_key_env_vars() {
        use clap::CommandFactory;
        #[derive(clap::Parser)]
        struct Wrapper {
            #[command(flatten)]
            gloss: GlossArgs,
        }
        let mut cmd = Wrapper::command();
        let help = cmd.render_long_help().to_string();
        let providers: serde_json::Value =
            serde_json::from_str(include_str!("../../../assets/providers.json")).unwrap();
        let providers = providers.as_array().or_else(|| providers["providers"].as_array()).unwrap();
        for p in providers {
            let line = format!("{}: {}", p["name"].as_str().unwrap(), p["api_key_env_var_name"].as_str().unwrap());
            assert!(help.contains(&line), "--model-name help lacks `{}`", line);
            let row = format!("| {} | `{}` |", p["name"].as_str().unwrap(), p["api_key_env_var_name"].as_str().unwrap());
            assert!(
                crate::commands::docs::DOC_MARKDOWN.contains(&row),
                "the API keys table of docs/simsapa-cli-and-localhost-api.md lacks `{}`",
                row
            );
        }
    }

    #[test]
    fn test_provider_from_arg() {
        assert_eq!(provider_from_arg("gemini").unwrap(), "Gemini");
        assert_eq!(provider_from_arg(" OpenRouter ").unwrap(), "OpenRouter");
        assert_eq!(provider_from_arg("xai").unwrap(), "xAI");
        assert!(provider_from_arg("nosuch").is_err());
    }

    #[test]
    fn test_output_format_default() {
        let mut a = args("x");
        assert_eq!(a.output_format(true), OutputFormatArg::Text);
        assert_eq!(a.output_format(false), OutputFormatArg::Markdown);
        a.output = Some("dhp23.docx".into());
        assert_eq!(a.output_format(true), OutputFormatArg::Docx);
        a.output = Some("dhp23.unknown".into());
        assert_eq!(a.output_format(true), OutputFormatArg::Markdown);
        a.format = Some(OutputFormatArg::Html);
        assert_eq!(a.output_format(true), OutputFormatArg::Html);
    }

    #[test]
    fn test_progress_line() {
        let p = GlossAiProgress {
            stage: GlossAiStage::Translation { paragraph: 1, paragraph_count: 3 },
            model: "m".into(),
            message: "Request sent".into(),
            kind: "trying".into(),
        };
        assert_eq!(progress_line(&p), "Waiting for AI response (m): translation, paragraph 2 of 3…");
    }

    #[test]
    fn test_lookup_forms_from_arguments() {
        let forms = lookup_forms_input(&["a".into(), "b".into()]).unwrap();
        assert_eq!(forms, vec!["a", "b"]);
    }
}

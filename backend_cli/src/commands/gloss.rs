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
use simsapa_backend::anki_export::export_anki_csv;
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

    /// Make no AI request, even when an AI provider is enabled in the app
    #[arg(long)]
    pub no_ai: bool,

    /// Let the AI choose word senses, but make no translation
    #[arg(long)]
    pub no_translate: bool,

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
        if self.exact {
            if self.gloss_format == Some(GlossFormatArg::WordInSentence) {
                return Err(CliError::error("--exact needs --gloss-format headwords"));
            }
            if self.format.is_some_and(|f| f != OutputFormatArg::Text) || self.output.is_some() {
                return Err(CliError::error("--exact prints text or --json only"));
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

/// The paragraphs to gloss and the document title: a reference's passage, or
/// the text given.
fn gloss_input(args: &GlossArgs) -> Result<(Vec<String>, String), CliError> {
    let joined = args.text.join(" ");
    let is_reference = !(args.text.len() == 1 && args.text[0] == "-") && is_sutta_reference(&joined);
    if !is_reference {
        if args.edition.is_some() || args.paragraphs.is_some() {
            return Err(CliError::error("--edition and --paragraphs need a sutta reference"));
        }
        let text = text_or_stdin(&args.text)?;
        return Ok((split_gloss_paragraphs(&text), String::new()));
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
    Ok((source.paragraphs, title))
}

/// Requires the preamble to have run. `can_write` is false while the GUI or
/// another `simsapa` command is using the data folder: the AI's sense
/// choices are then used but not saved.
pub fn run_gloss(args: &GlossArgs, json: bool, can_write: bool) -> CliResult {
    args.validate(json)?;
    let (paragraphs, title) = gloss_input(args)?;
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

    let gloss_format = args.gloss_format();
    let input = AllParagraphsProcessingInput {
        paragraphs: paragraphs.clone(),
        options: gloss_word_processing_options(&opts, gloss_format),
    };
    let mut result = process_all_paragraphs(&input, &dbm.appdata, &dbm.dpd)
        .map_err(|e| CliError::error(format!("Glossing failed: {}", e)))?;

    let ai_ran = run_ai(args, &mut result, &paragraphs, can_write)?;

    if json {
        print_json(&GlossTextResponse::Default(result.clone()))?;
    } else {
        write_output(args, &result, &paragraphs, &title, gloss_format)?;
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

/// Run the AI steps the flags and the app settings ask for: word selection
/// when it is turned on in the Gloss tab's Word Selection dialog, and the
/// translation unless `--no-translate`. Returns whether anything was asked.
fn run_ai(
    args: &GlossArgs,
    result: &mut AllParagraphsProcessingResult,
    paragraphs: &[String],
    can_write: bool,
) -> Result<bool, CliError> {
    if args.no_ai || !result.success || result.paragraphs.is_empty() {
        return Ok(false);
    }
    if !ai_available() {
        errln!("Note: no AI provider is enabled in the Simsapa app, so no AI request is made (--no-ai skips this check).");
        return Ok(false);
    }
    let word_selection = {
        let settings = get_app_data().app_settings_cache.read().expect("Failed to read app settings");
        settings.gloss_word_selection_enabled
    };
    let options = GlossAiOptions { word_selection, translate: !args.no_translate, write_cache: can_write };
    if !options.word_selection && !options.translate {
        return Ok(false);
    }
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
    Ok(true)
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
) -> CliResult {
    let format = args.output_format(stdout_is_tty());
    let output = args.output.as_deref();
    match format {
        OutputFormatArg::Text => {
            match gloss_format {
                GlossFormat::Headwords => print_gloss_text(result),
                GlossFormat::WordInSentence => print_rows_text(result, title),
            }
            Ok(())
        }
        OutputFormatArg::Markdown | OutputFormatArg::Html | OutputFormatArg::Org => {
            let text_format = match format {
                OutputFormatArg::Html => TextFormat::Html,
                OutputFormatArg::Org => TextFormat::OrgMode,
                _ => TextFormat::Markdown,
            };
            let data = gloss_result_export_data(result, paragraphs, title, gloss_format);
            emit_text(output, &gloss_export_document(&data, text_format))
        }
        OutputFormatArg::Docx => {
            let path = output.ok_or_else(|| CliError::error("--format docx needs --output <path>"))?;
            let data = gloss_result_export_data(result, paragraphs, title, gloss_format);
            let bytes = generate_gloss_docx(&serde_json::to_string(&data)?)?;
            write_file(path, &bytes)
        }
        OutputFormatArg::AnkiCsv => {
            let data = gloss_result_export_data(result, paragraphs, title, gloss_format);
            write_anki_csv(output, &data)
        }
        OutputFormatArg::Json => {
            let session = session_export_json(args, result, paragraphs, title, gloss_format)?;
            emit_text(output, &session)
        }
    }
}

/// The `simsapa-gloss-session` export of the result, which the app's Gloss
/// tab opens with "Open JSON".
fn session_export_json(
    args: &GlossArgs,
    result: &AllParagraphsProcessingResult,
    paragraphs: &[String],
    title: &str,
    gloss_format: GlossFormat,
) -> Result<String, CliError> {
    let options = GlossExportOptions {
        text: paragraphs.join("\n\n"),
        title: title.to_string(),
        gloss_format,
        no_ai_provider: !get_app_data().has_enabled_ai_model(),
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

/// One text line per row, with its components indented under it:
/// `[word, conf, grammar, meaning]`.
fn row_cells(row: &GlossRow, out: &mut Vec<[String; 4]>) {
    let word = format!("{}{}", "- ".repeat(row.depth as usize), row.word);
    let conf = row.confidence.map(|c| c.as_str().to_string()).unwrap_or_default();
    out.push([word, conf, row.grammar.clone(), row.meaning.clone()]);
    for c in &row.components {
        row_cells(c, out);
    }
}

/// The Word in Sentence result as aligned columns: word, confidence mark,
/// grammar, meaning; then the translations and the legend.
fn print_rows_text(result: &AllParagraphsProcessingResult, title: &str) {
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
        let word_w = cells.iter().map(|c| c[0].chars().count()).max().unwrap_or(0);
        let grammar_w = cells.iter().map(|c| c[2].chars().count()).max().unwrap_or(0);
        for c in &cells {
            let line = format!(
                "{}{}  {:1}  {}{}  {}",
                c[0],
                " ".repeat(word_w - c[0].chars().count()),
                c[1],
                c[2],
                " ".repeat(grammar_w - c[2].chars().count()),
                c[3],
            );
            outln!("{}", line.trim_end());
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
        if !get_app_data().has_enabled_ai_model() {
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

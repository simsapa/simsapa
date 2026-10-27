//! `simsapa gloss` and `simsapa lookup`: the glossing pipeline of
//! `POST /gloss_text` and the exact DPD form lookup of `POST /lookup_forms`,
//! in-process, with the same request handling and the same `--json` output.
//! See docs/simsapa-cli-and-localhost-api.md §16 and §19.

use std::io::Read;

use clap::Args;

use simsapa_api::routes::gloss::{
    GlossTextOptions, GlossTextResponse, LookupFormsResponse, gloss_word_processing_options, split_gloss_paragraphs,
};
use simsapa_backend::get_app_data;
use simsapa_backend::helpers::{process_all_paragraphs, process_all_paragraphs_exact};
use simsapa_backend::lookup_forms::{validate_gloss_exact_options, validate_lookup_forms_request};
use simsapa_backend::types::{
    AllParagraphsProcessingInput, AllParagraphsProcessingResult, ExactGlossResult, FormLookupResult, FormLookupStatus,
    ProcessedWord,
};

use crate::exit::{CliError, CliResult};
use crate::output::{outln, print_json, snippet_to_terminal, stdout_is_tty};

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

#[derive(Args, Debug)]
pub struct GlossArgs {
    /// The Pāli text, or `-` to read it from stdin. Paragraphs are separated
    /// by a blank line.
    #[arg(required = true, num_args = 1..)]
    pub text: Vec<String>,

    /// One exact DPD form lookup per word, in order, instead of the
    /// dictionary options (`/gloss_text` with `"exact": true`)
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
        }
    }
}

/// Requires the preamble to have run.
pub fn run_gloss(args: &GlossArgs, json: bool) -> CliResult {
    let text = text_or_stdin(&args.text)?;
    let paragraphs = split_gloss_paragraphs(&text);
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

    let input = AllParagraphsProcessingInput {
        paragraphs,
        options: gloss_word_processing_options(&opts),
    };
    let result = process_all_paragraphs(&input, &dbm.appdata, &dbm.dpd)
        .map_err(|e| CliError::error(format!("Glossing failed: {}", e)))?;
    if json {
        return print_json(&GlossTextResponse::Default(result));
    }
    print_gloss_text(&result);
    Ok(())
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
        if !p.unrecognized_words.is_empty() {
            outln!("Unrecognized: {}", p.unrecognized_words.join(", "));
        }
    }
}

fn status_label(status: &FormLookupStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{:?}", status))
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
                status_label(&t.lookup.status),
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
        let mut line = format!("{}  {}  {}", r.input, r.lookup_key, status_label(&r.status));
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
        let args = GlossArgs {
            text: vec!["x".into()],
            exact: true,
            no_duplicates_globally: false,
            skip_common: false,
            common_words: None,
        };
        let o = args.options();
        assert_eq!(o.exact, Some(true));
        assert_eq!(o.no_duplicates_globally, None);
        assert_eq!(o.skip_common, None);
        assert!(validate_gloss_exact_options(o.no_duplicates_globally, o.skip_common, o.common_words.as_ref()).is_ok());
    }

    #[test]
    fn test_exact_rejects_skip_common() {
        let args = GlossArgs {
            text: vec!["x".into()],
            exact: true,
            no_duplicates_globally: false,
            skip_common: true,
            common_words: None,
        };
        let o = args.options();
        assert!(validate_gloss_exact_options(o.no_duplicates_globally, o.skip_common, o.common_words.as_ref()).is_err());
    }

    #[test]
    fn test_lookup_forms_from_arguments() {
        let forms = lookup_forms_input(&["a".into(), "b".into()]).unwrap();
        assert_eq!(forms, vec!["a", "b"]);
    }
}

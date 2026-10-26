//! `simsapa word`: one dictionary entry by uid, resolved with
//! `AppData::resolve_word_uid()`, so it takes the same uid forms as
//! `/word.json` (`dhamma 1.01`, `34626/dpd`, `dhamma-1-01/dpd`). `--json` is
//! the `/word.json` body, `--html` the `/word_html` page.

use clap::Args;
use serde::Serialize;

use simsapa_backend::get_app_data;

use crate::exit::{CliError, CliResult};
use crate::output::{html_to_text, print_json};

#[derive(Args, Debug)]
pub struct WordArgs {
    /// The word's uid, e.g. "dhamma 1.01", 34626/dpd or dhamma-1-01/dpd
    pub uid: String,

    /// Print the rendered HTML page, as `/word_html` serves it
    #[arg(long)]
    pub html: bool,
}

#[derive(Serialize)]
struct WordHtml<'a> {
    uid: &'a str,
    html: String,
}

/// The fields the text output shows, in order, when the entry has them.
const TEXT_FIELDS: [(&str, &str); 18] = [
    ("lemma_1", "Lemma"),
    ("word", "Word"),
    ("bold", "Word"),
    ("dict_label", "Dictionary"),
    ("pos", "Part of speech"),
    ("grammar", "Grammar"),
    ("meaning_1", "Meaning"),
    ("meaning_lit", "Literally"),
    ("meaning_2", "Meaning (2)"),
    ("root_key", "Root"),
    ("construction_summary", "Construction"),
    ("construction", "Construction (parts)"),
    ("sanskrit", "Sanskrit"),
    ("summary", "Summary"),
    ("book", "Book"),
    ("title", "Title"),
    ("commentary", "Commentary"),
    ("definition_plain", "Definition"),
];

fn not_found(uid: &str) -> CliError {
    CliError::not_found(format!("word not found: {}", uid))
}

/// Requires the preamble to have run.
pub fn run(args: &WordArgs, json: bool) -> CliResult {
    let app_data = get_app_data();
    let resolved = app_data.resolve_word_uid(&args.uid).ok_or_else(|| not_found(&args.uid))?;

    if args.html {
        let html = app_data.render_word_html_by_uid("", &args.uid);
        if json {
            return print_json(&WordHtml { uid: resolved.canonical_uid(), html });
        }
        println!("{}", html);
        return Ok(());
    }

    let body = app_data.word_json_with_extras(&resolved)?;
    if json {
        // `/word.json`'s default body: a one-element array.
        return print_json(&serde_json::Value::Array(vec![body]));
    }

    println!("{}", resolved.canonical_uid());
    let mut shown: Vec<String> = Vec::new();
    for (key, label) in TEXT_FIELDS {
        let Some(value) = body.get(key) else { continue };
        let text = match value {
            serde_json::Value::String(s) => html_to_text(s),
            serde_json::Value::Number(n) => n.to_string(),
            _ => continue,
        };
        // `construction` often repeats `construction_summary`.
        if text.trim().is_empty() || shown.contains(&text) {
            continue;
        }
        if text.contains('\n') {
            println!("{}:\n{}", label, text);
        } else {
            println!("{}: {}", label, text);
        }
        shown.push(text);
    }
    Ok(())
}

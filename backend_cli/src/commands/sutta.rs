//! `simsapa sutta`: one sutta by uid or reference.
//!
//! - text (the default): title, uid and the text, from the same content block
//!   the reader renders, in the Solo layout;
//! - `--html`: the `/sutta_html` page, through the route's own
//!   `sutta_html_response()`;
//! - `--content-json`: the `/sutta.json` body, the stored Bilara JSON verbatim;
//! - `--bilara`: the `/sutta_bilara.json` body.
//!
//! The text and HTML forms resolve as the HTML routes do (verse references,
//! `/pli/ms` and range fallbacks); the two JSON forms resolve strictly, as
//! their routes do, and never substitute the Pāli for a missing translation.

use clap::Args;
use serde::Serialize;

use simsapa_api::routes::suttas::{sutta_html_response, sutta_json_no_content_message, sutta_json_not_found_message};
use simsapa_api::lookup_sutta_with_fallback;
use simsapa_backend::app_settings::SuttaLayout;
use simsapa_backend::errln;
use simsapa_backend::get_app_data;
use simsapa_backend::helpers::convert_verse_ref_to_sutta_uid;
use simsapa_backend::sutta_display::SuttaDisplayOverrides;
use simsapa_backend::sutta_json::{SuttaBilaraJson, SuttaContentJson, get_sutta_bilara_json, get_sutta_content_json};

use crate::exit::{CliError, CliResult};
use crate::output::{html_to_text, outln, print_json};

#[derive(Args, Debug)]
pub struct SuttaArgs {
    /// The sutta's uid or reference, e.g. mn1, mn1/en/sujato, dhp182
    pub uid: String,

    /// Print the rendered HTML page, as `/sutta_html` serves it
    #[arg(long, group = "sutta_format")]
    pub html: bool,

    /// Print the title, uid and plain text (the default)
    #[arg(long, group = "sutta_format")]
    pub text: bool,

    /// Print the SuttaCentral (Bilara) JSON, exactly as `/sutta.json` serves it
    #[arg(long, group = "sutta_format")]
    pub content_json: bool,

    /// Print the Bilara JSON with its HTML template, variant readings,
    /// comments and glosses, exactly as `/sutta_bilara.json` serves it
    #[arg(long, group = "sutta_format")]
    pub bilara: bool,
}

#[derive(Serialize)]
struct SuttaText {
    uid: String,
    sutta_ref: String,
    title: String,
    language: String,
    source_uid: Option<String>,
    text: String,
}

#[derive(Serialize)]
struct SuttaHtml {
    uid: String,
    html: String,
}

fn not_found(uid: &str) -> CliError {
    CliError::not_found(sutta_json_not_found_message(uid))
}

/// Requires the preamble to have run.
pub fn run(args: &SuttaArgs, json: bool) -> CliResult {
    if args.content_json {
        return run_content_json(&args.uid);
    }
    if args.bilara {
        return run_bilara(&args.uid);
    }

    let dbm = &get_app_data().dbm;
    let processed_uid = convert_verse_ref_to_sutta_uid(&args.uid);
    let sutta = lookup_sutta_with_fallback(dbm, &processed_uid).ok_or_else(|| not_found(&args.uid))?;

    if args.html {
        let (status, html) = sutta_html_response("", &sutta.uid, None, None, None, None, dbm);
        if status.code != 200 {
            return Err(CliError::error(html.0));
        }
        if json {
            return print_json(&SuttaHtml { uid: sutta.uid, html: html.0 });
        }
        outln!("{}", html.0);
        return Ok(());
    }

    // The sutta alone in the Solo layout, so a translation is not interleaved
    // with its Pāli.
    let app_data = get_app_data();
    let overrides = SuttaDisplayOverrides {
        layout: Some(SuttaLayout::Solo),
        columns: Some(vec![sutta.uid.clone()]),
        show_references: Some(false),
        ..Default::default()
    };
    let options = app_data.resolve_sutta_display_options(&sutta, &overrides);
    let block = app_data.render_sutta_content_block(&sutta, &options)?;
    let text = html_to_text(&block);

    let title = sutta.title.clone().unwrap_or_default();
    if json {
        return print_json(&SuttaText {
            uid: sutta.uid,
            sutta_ref: sutta.sutta_ref,
            title,
            language: sutta.language,
            source_uid: sutta.source_uid,
            text,
        });
    }

    outln!("{}", if title.is_empty() { &sutta.sutta_ref } else { &title });
    outln!("{}", sutta.uid);
    outln!();
    outln!("{}", text);
    Ok(())
}

/// Print a stored JSON body unchanged: never through a `serde_json::Value`,
/// which would sort the segment ids and scramble the text order.
fn print_verbatim(found_uid: &str, body: &str) -> CliResult {
    errln!("{}", found_uid);
    outln!("{}", body);
    Ok(())
}

fn run_content_json(uid: &str) -> CliResult {
    match get_sutta_content_json(&get_app_data().dbm.appdata, uid) {
        SuttaContentJson::Found { uid: found_uid, content_json } => print_verbatim(&found_uid, &content_json),
        SuttaContentJson::NoContentJson { uid: found_uid } => {
            Err(CliError::not_found(sutta_json_no_content_message(&found_uid)))
        }
        SuttaContentJson::NotFound => Err(not_found(uid)),
    }
}

fn run_bilara(uid: &str) -> CliResult {
    match get_sutta_bilara_json(&get_app_data().dbm.appdata, uid) {
        SuttaBilaraJson::Found { uid: found_uid, json } => print_verbatim(&found_uid, &json),
        SuttaBilaraJson::NoContentJson { uid: found_uid } => {
            Err(CliError::not_found(sutta_json_no_content_message(&found_uid)))
        }
        SuttaBilaraJson::NotFound => Err(not_found(uid)),
    }
}

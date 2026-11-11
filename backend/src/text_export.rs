//! Text exports (HTML / Markdown / Org-Mode) for the Gloss and Prompts tabs.
//!
//! The formatting was ported from the QML `format_paragraph_*` /
//! `message_as_*` helpers so it can be unit-tested against fixed JSON. The
//! input JSON is assembled in QML (`gloss_export_data()` /
//! `chat_export_data()`); the same structs are shared with the DOCX exporter
//! (see [`crate::export_types`]).

use anyhow::{bail, Context, Result};
use lazy_static::lazy_static;
use regex::Regex;

use crate::export_types::{
    confidence_cell, AiResponse, ChatExportData, ChatMessage, GlossExportData, GlossExportParagraph,
    GlossExportRow, GlossExportVocabItem, WORD_TABLE_COLUMNS, WORD_TABLE_SHADE_HEX,
    word_table_row_shades,
};
use crate::gloss_rows::{CONFIDENCE_LEGEND, CONFIDENCE_LEGEND_LEAD, NO_AI_PROVIDER_NOTE};
use crate::types::GlossFormat;
use crate::markdown_convert::markdown_to_orgmode;
use crate::prompt_utils::markdown_to_html;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextFormat {
    Html,
    Markdown,
    OrgMode,
}

impl TextFormat {
    pub fn parse(name: &str) -> Result<TextFormat> {
        match name.to_lowercase().as_str() {
            "html" => Ok(TextFormat::Html),
            "markdown" | "md" => Ok(TextFormat::Markdown),
            "orgmode" | "org" | "org-mode" => Ok(TextFormat::OrgMode),
            other => bail!("Unknown text export format: {}", other),
        }
    }
}

/// Collapse runs of 3+ newlines into 2 and trim surrounding whitespace,
/// mirroring the QML `.trim().replace(/\n\n\n+/g, "\n\n")`.
fn normalize(text: &str) -> String {
    lazy_static! {
        static ref RE_BLANK_LINES: Regex = Regex::new(r"\n{3,}").unwrap();
    }
    RE_BLANK_LINES.replace_all(text.trim(), "\n\n").to_string()
}

// --- Summary markup conversion --------------------------------------------

/// Convert a DPD summary (`<b>`/`<i>` markup) to Markdown emphasis, escaping
/// literal asterisks first.
fn summary_html_to_md(text: &str) -> String {
    text.replace('*', "&ast;")
        .replace("<i>", "*")
        .replace("</i>", "*")
        .replace("<b>", "**")
        .replace("</b>", "**")
}

/// Convert a DPD summary (`<b>`/`<i>` markup) to Org-Mode emphasis, escaping
/// literal asterisks first.
fn summary_html_to_orgmode(text: &str) -> String {
    text.replace('*', "&ast;")
        .replace("<i>", "/")
        .replace("</i>", "/")
        .replace("<b>", "*")
        .replace("</b>", "*")
}

// --- Shared pieces ---------------------------------------------------------

/// Escape text for HTML element content.
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// A heading of the given level (1 = document title).
fn heading(format: TextFormat, level: usize, text: &str) -> String {
    match format {
        TextFormat::Html => format!("\n<h{level}>{}</h{level}>\n", escape_html(text)),
        TextFormat::Markdown => format!("\n{} {}\n", "#".repeat(level), text),
        TextFormat::OrgMode => format!("\n{} {}\n", "*".repeat(level), text),
    }
}

/// The confidence legend (docs/gloss-ai-word-selection.md §5), with the
/// no-AI note under it when no AI provider was enabled.
fn confidence_legend(format: TextFormat, no_ai_provider: bool) -> String {
    let mut out = String::from("\n");
    match format {
        TextFormat::Html => {
            out.push_str(&format!("<p>{}</p>\n<ul>\n", escape_html(CONFIDENCE_LEGEND_LEAD)));
            for line in CONFIDENCE_LEGEND {
                out.push_str(&format!("<li>{}</li>\n", escape_html(line)));
            }
            out.push_str("</ul>\n");
            if no_ai_provider {
                out.push_str(&format!("<p>{}</p>\n", escape_html(NO_AI_PROVIDER_NOTE)));
            }
        }
        TextFormat::Markdown | TextFormat::OrgMode => {
            out.push_str(&format!("{}\n\n", CONFIDENCE_LEGEND_LEAD));
            for line in CONFIDENCE_LEGEND {
                out.push_str(&format!("- {}\n", line));
            }
            if no_ai_provider {
                out.push_str(&format!("\n{}\n", NO_AI_PROVIDER_NOTE));
            }
        }
    }
    out
}

// --- Gloss: Word in Sentence ------------------------------------------------

/// A paragraph's text as written lines.
fn passage_text(paragraph: &GlossExportParagraph, format: TextFormat) -> String {
    let text = paragraph.display_text();
    match format {
        TextFormat::Html => format!("\n<p>{}</p>\n", escape_html(&text).replace('\n', "<br>\n")),
        TextFormat::Markdown | TextFormat::OrgMode => format!("\n{}\n", text),
    }
}

/// A labelled translation text (`**Translation:** …`).
fn labelled_translation(format: TextFormat, label: &str, text: &str) -> String {
    match format {
        TextFormat::Html => format!("\n<p><b>{}:</b></p>\n{}\n", label, markdown_to_html(text.trim())),
        TextFormat::Markdown => format!("\n**{}:** {}\n", label, text.trim()),
        TextFormat::OrgMode => format!("\n*{}:* {}\n", label, markdown_to_orgmode(text.trim()).trim()),
    }
}

/// `### English Translation` with each model's Translation and Literal
/// Translation; a model heading only when there are several. Empty without
/// a translation. A translation saved before the two-part translation has no
/// Literal Translation line.
fn word_in_sentence_translations(paragraph: &GlossExportParagraph, format: TextFormat) -> String {
    let translations: Vec<&AiResponse> = paragraph
        .ai_translations
        .iter()
        .filter(|t| !t.response.trim().is_empty() || !t.literal_translation.trim().is_empty())
        .collect();
    if translations.is_empty() {
        return String::new();
    }
    let mut out = heading(format, 3, "English Translation");
    for t in &translations {
        if translations.len() > 1 {
            out.push_str(&heading(format, 4, &format!("{}{}", t.model_name, t.selected_suffix())));
        }
        out.push_str(&labelled_translation(format, "Translation", &t.response));
        if !t.literal_translation.trim().is_empty() {
            out.push_str(&labelled_translation(format, "Literal Translation", &t.literal_translation));
        }
    }
    out
}

/// `### Variants`, one `a//b` line per group, as in DPD's reports. Empty
/// without groups.
fn word_in_sentence_variants(paragraph: &GlossExportParagraph, format: TextFormat) -> String {
    if paragraph.variants.is_empty() {
        return String::new();
    }
    let mut out = heading(format, 3, "Variants");
    for group in &paragraph.variants {
        let line = group.join("//");
        match format {
            TextFormat::Html => out.push_str(&format!("\n<p>{}</p>\n", escape_html(&line))),
            TextFormat::Markdown | TextFormat::OrgMode => out.push_str(&format!("\n{}\n", line)),
        }
    }
    out
}

/// A table cell's text on one line, with the column separator escaped.
fn table_cell_text(text: &str, format: TextFormat) -> String {
    let one_line = text.replace(['\r', '\n'], " ");
    match format {
        TextFormat::Html => escape_html(&one_line),
        TextFormat::Markdown => one_line.replace('|', "\\|"),
        TextFormat::OrgMode => one_line.replace('|', "\\vert{}"),
    }
}

/// The Word-by-Word table (`ID | Word in Sentence | … | Conf.`).
fn word_table(rows: &[GlossExportRow], format: TextFormat) -> String {
    let mut out = String::from("\n");
    match format {
        TextFormat::Html => {
            out.push_str("<table>\n<thead><tr>");
            for col in WORD_TABLE_COLUMNS {
                out.push_str(&format!("<th>{}</th>", escape_html(col)));
            }
            out.push_str("</tr></thead>\n<tbody>\n");
            for (row, shaded) in rows.iter().zip(word_table_row_shades(rows)) {
                if shaded {
                    out.push_str(&format!("<tr style=\"background-color: #{WORD_TABLE_SHADE_HEX};\">"));
                } else {
                    out.push_str("<tr>");
                }
                for cell in row.cells() {
                    out.push_str(&format!("<td>{}</td>", table_cell_text(&cell, format)));
                }
                out.push_str("</tr>\n");
            }
            out.push_str("</tbody></table>\n");
        }
        TextFormat::Markdown => {
            out.push_str(&format!("| {} |\n", WORD_TABLE_COLUMNS.join(" | ")));
            out.push_str(&format!("|{}\n", " :--- |".repeat(WORD_TABLE_COLUMNS.len())));
            for row in rows {
                let cells: Vec<String> = row.cells().iter().map(|c| table_cell_text(c, format)).collect();
                out.push_str(&format!("| {} |\n", cells.join(" | ")));
            }
        }
        TextFormat::OrgMode => {
            out.push_str(&format!("| {} |\n", WORD_TABLE_COLUMNS.join(" | ")));
            out.push_str(&format!("|{}|\n", vec!["---"; WORD_TABLE_COLUMNS.len()].join("+")));
            for row in rows {
                let cells: Vec<String> = row.cells().iter().map(|c| table_cell_text(c, format)).collect();
                out.push_str(&format!("| {} |\n", cells.join(" | ")));
            }
        }
    }
    out
}

/// One paragraph of a Word in Sentence document: passage, translations,
/// variants, the table (DPD's report order, docs/dpd-analysis-procedure.md
/// §1.7).
fn word_in_sentence_paragraph(paragraph: &GlossExportParagraph, format: TextFormat) -> String {
    let mut out = passage_text(paragraph, format);
    out.push_str(&word_in_sentence_translations(paragraph, format));
    out.push_str(&word_in_sentence_variants(paragraph, format));
    out.push_str(&heading(format, 3, "Word-by-Word Analysis"));
    out.push_str(&word_table(&paragraph.rows, format));
    out
}

/// The body of a Word in Sentence document: the title, each paragraph (under
/// `Paragraph <n>` when there are several), the legend.
fn word_in_sentence_document_body(data: &GlossExportData, format: TextFormat) -> String {
    let mut out = heading(format, 1, &data.document_title());
    let several = data.paragraphs.len() > 1;
    for (i, paragraph) in data.paragraphs.iter().enumerate() {
        if several {
            out.push_str(&heading(format, 2, &format!("Paragraph {}", i + 1)));
        }
        out.push_str(&word_in_sentence_paragraph(paragraph, format));
    }
    out.push_str(&confidence_legend(format, data.no_ai_provider));
    out
}

// --- Gloss: Headwords per-paragraph -----------------------------------------

/// A Headwords translation: the response as a quote, as before the
/// two-part translation; with a Literal Translation, both under labels.
fn headwords_translation(trans: &AiResponse, format: TextFormat) -> String {
    let title = format!("{}{}", trans.model_name, trans.selected_suffix());
    let has_literal = !trans.literal_translation.trim().is_empty();
    let quote = |text: &str| -> String {
        match format {
            TextFormat::Html => format!("<blockquote>{}</blockquote>\n", markdown_to_html(text)),
            TextFormat::Markdown => format!("\n> {}\n", text.replace('\n', "\n> ")),
            TextFormat::OrgMode => format!("\n{}\n", markdown_to_orgmode(text)),
        }
    };
    let label = |text: &str| -> String {
        match format {
            TextFormat::Html => format!("<p><b>{}:</b></p>\n", text),
            TextFormat::Markdown => format!("\n**{}:**\n", text),
            TextFormat::OrgMode => format!("\n*{}:*\n", text),
        }
    };
    let mut out = match format {
        TextFormat::Html => format!("<h3>{}</h3>\n", title),
        TextFormat::Markdown => format!("\n### {}\n", title),
        TextFormat::OrgMode => format!("\n*** {}\n", title),
    };
    if has_literal {
        out.push_str(&label("Translation"));
    }
    out.push_str(&quote(&trans.response));
    if has_literal {
        out.push_str(&label("Literal Translation"));
        out.push_str(&quote(&trans.literal_translation));
    }
    out
}

fn headwords_translations(paragraph: &GlossExportParagraph, format: TextFormat) -> String {
    if paragraph.ai_translations.is_empty() {
        return String::new();
    }
    let mut out = match format {
        TextFormat::Html => String::from("\n<p><b>AI Translations</b></p>\n"),
        TextFormat::Markdown => String::from("\n**AI Translations**\n"),
        TextFormat::OrgMode => String::from("\n*AI Translations*\n"),
    };
    for trans in &paragraph.ai_translations {
        out.push_str(&headwords_translation(trans, format));
    }
    out
}

/// The Conf. cell of a vocabulary item, when the document has the column.
fn vocab_conf(item: &GlossExportVocabItem, show_conf: bool) -> Option<&'static str> {
    show_conf.then(|| confidence_cell(item.confidence))
}

fn gloss_paragraph_html(paragraph: &GlossExportParagraph, show_conf: bool) -> String {
    let para_text = format!(
        "\n<blockquote>\n{}\n</blockquote>\n",
        paragraph.text.replace('\n', "<br>\n")
    );

    let mut table_rows = String::new();
    for res in &paragraph.vocabulary {
        let conf = vocab_conf(res, show_conf).map(|c| format!("<td> {} </td>", c)).unwrap_or_default();
        table_rows.push_str(&format!(
            "<tr><td> <b>{}</b> </td><td> {} </td>{}</tr>\n",
            res.word, res.summary, conf
        ));
    }

    let ai_section = headwords_translations(paragraph, TextFormat::Html);

    format!("\n{para_text}\n\n{ai_section}\n\n<table><tbody>\n{table_rows}\n</tbody></table>\n")
}

fn gloss_paragraph_markdown(paragraph: &GlossExportParagraph, show_conf: bool) -> String {
    let para_text = format!("\n> {}", paragraph.text.replace('\n', "\n> "));

    let mut table_rows = String::new();
    for res in &paragraph.vocabulary {
        let conf = vocab_conf(res, show_conf).map(|c| format!(" {} |", c)).unwrap_or_default();
        table_rows.push_str(&format!(
            "| **{}** | {} |{}\n",
            res.word,
            summary_html_to_md(&res.summary),
            conf
        ));
    }

    let ai_section = headwords_translations(paragraph, TextFormat::Markdown);

    let header = if show_conf {
        "|    |    |    |\n|----|----|----|\n"
    } else {
        "|    |    |\n|----|----|\n"
    };
    format!("\n{para_text}\n\n{ai_section}\n\n{header}{table_rows}\n")
}

fn gloss_paragraph_orgmode(paragraph: &GlossExportParagraph, show_conf: bool) -> String {
    let para_text = format!("\n#+begin_quote\n{}\n#+end_quote\n", paragraph.text);

    let mut table_rows = String::new();
    for res in &paragraph.vocabulary {
        let conf = vocab_conf(res, show_conf).map(|c| format!(" {} |", c)).unwrap_or_default();
        table_rows.push_str(&format!(
            "| *{}* | {} |{}\n",
            res.word,
            summary_html_to_orgmode(&res.summary),
            conf
        ));
    }

    let ai_section = headwords_translations(paragraph, TextFormat::OrgMode);

    format!("\n{para_text}\n\n{ai_section}\n\n{table_rows}\n")
}

fn gloss_paragraph_format(paragraph: &GlossExportParagraph, format: TextFormat, show_conf: bool) -> String {
    match format {
        TextFormat::Html => gloss_paragraph_html(paragraph, show_conf),
        TextFormat::Markdown => gloss_paragraph_markdown(paragraph, show_conf),
        TextFormat::OrgMode => gloss_paragraph_orgmode(paragraph, show_conf),
    }
}

/// The single "Paragraphs" section heading emitted once before the paragraph
/// iteration (replacing the former per-paragraph "Paragraph N" headings).
fn paragraphs_heading(format: TextFormat) -> &'static str {
    match format {
        TextFormat::Html => "\n<h2>Paragraphs</h2>\n",
        TextFormat::Markdown => "\n## Paragraphs\n",
        TextFormat::OrgMode => "\n** Paragraphs\n",
    }
}

/// The body of a Headwords document.
fn headwords_document_body(data: &GlossExportData, format: TextFormat) -> String {
    let show_conf = data.shows_confidence();
    let mut out = String::new();
    let title = data.document_title();
    match format {
        TextFormat::Html => {
            let main_text = format!(
                "\n<blockquote>\n{}\n</blockquote>\n",
                data.text.replace('\n', "<br>\n")
            );
            out.push_str(&format!("<h1>{}</h1>\n\n{}\n", escape_html(&title), main_text));
        }
        TextFormat::Markdown => {
            let main_text = format!("\n> {}", data.text.replace('\n', "\n> "));
            out.push_str(&format!("\n# {}\n\n{}\n", title, main_text));
        }
        TextFormat::OrgMode => {
            let main_text = format!("\n#+begin_quote\n{}\n#+end_quote\n", data.text);
            out.push_str(&format!("\n* {}\n\n{}\n", title, main_text));
        }
    }
    if !data.paragraphs.is_empty() {
        out.push_str(paragraphs_heading(format));
    }
    for para in &data.paragraphs {
        out.push_str(&gloss_paragraph_format(para, format, show_conf));
    }
    if show_conf {
        out.push_str(&confidence_legend(format, data.no_ai_provider));
    }
    out
}

// --- Gloss: full document -------------------------------------------------

/// Render a full gloss export document in the given format, in the layout of
/// its `gloss_format`.
pub fn gloss_export(gloss_json: &str, format: &str) -> Result<String> {
    let format = TextFormat::parse(format)?;
    let data: GlossExportData =
        serde_json::from_str(gloss_json).context("Failed to parse gloss export JSON")?;
    Ok(gloss_export_document(&data, format))
}

/// Render a full gloss export document from the export data.
pub fn gloss_export_document(data: &GlossExportData, format: TextFormat) -> String {
    let body = match data.gloss_format {
        GlossFormat::WordInSentence => word_in_sentence_document_body(data, format),
        GlossFormat::Headwords => headwords_document_body(data, format),
    };
    let out = match format {
        TextFormat::Html => format!(
            concat!(
                "\n<!doctype html>\n<html>\n<head>\n",
                "    <meta charset=\"utf-8\">\n",
                "    <meta http-equiv=\"x-ua-compatible\" content=\"ie=edge\">\n",
                "    <title>{}</title>\n",
                "    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n",
                "</head>\n<body>\n{}\n</body>\n</html>"
            ),
            escape_html(&data.document_title()),
            body
        ),
        TextFormat::Markdown | TextFormat::OrgMode => body,
    };
    normalize(&out)
}

/// Render a single gloss paragraph fragment (for per-paragraph "Copy As...").
/// A paragraph with Word in Sentence rows gets that layout, without the
/// title; the legend follows a table with confidence values.
///
/// `_paragraph_number` is retained for bridge/API compatibility but no longer
/// drives a "Paragraph N" heading (the fragment is now just the content).
pub fn gloss_paragraph_export(
    paragraph_json: &str,
    _paragraph_number: usize,
    format: &str,
) -> Result<String> {
    let format = TextFormat::parse(format)?;
    let paragraph: GlossExportParagraph =
        serde_json::from_str(paragraph_json).context("Failed to parse gloss paragraph JSON")?;
    let out = if !paragraph.rows.is_empty() {
        let mut out = word_in_sentence_paragraph(&paragraph, format);
        out.push_str(&confidence_legend(format, false));
        out
    } else {
        let show_conf = paragraph.has_confidence();
        let mut out = gloss_paragraph_format(&paragraph, format, show_conf);
        if show_conf {
            out.push_str(&confidence_legend(format, false));
        }
        out
    };
    Ok(normalize(&out))
}

// --- Chat: per-message ----------------------------------------------------

fn chat_message_html(msg: &ChatMessage) -> String {
    match msg.role.as_str() {
        "system" | "user" => {
            let heading = if msg.role == "system" { "System" } else { "User" };
            format!(
                "\n<h2>{}</h2>\n<blockquote>{}</blockquote>\n",
                heading,
                msg.content.replace('\n', "<br>\n")
            )
        }
        "assistant" => {
            let mut out = String::from("\n<h2>Assistant</h2>\n");
            for resp in &msg.responses {
                let html = markdown_to_html(&resp.response);
                out.push_str(&format!(
                    "<h3>{}{}</h3>\n<blockquote>{}</blockquote>\n",
                    resp.model_name,
                    resp.selected_suffix(),
                    html
                ));
            }
            out
        }
        _ => String::new(),
    }
}

fn chat_message_markdown(msg: &ChatMessage) -> String {
    match msg.role.as_str() {
        "system" | "user" => {
            let heading = if msg.role == "system" { "System" } else { "User" };
            format!(
                "\n## {}\n\n> {}\n",
                heading,
                msg.content.replace('\n', "\n> ")
            )
        }
        "assistant" => {
            let mut out = String::from("\n## Assistant\n");
            for resp in &msg.responses {
                out.push_str(&format!(
                    "\n### {}{}\n\n> {}\n",
                    resp.model_name,
                    resp.selected_suffix(),
                    resp.response.replace('\n', "\n> ")
                ));
            }
            out
        }
        _ => String::new(),
    }
}

fn chat_message_orgmode(msg: &ChatMessage) -> String {
    match msg.role.as_str() {
        "system" | "user" => {
            let heading = if msg.role == "system" { "System" } else { "User" };
            format!("\n** {}\n\n#+begin_quote\n{}\n#+end_quote\n", heading, msg.content)
        }
        "assistant" => {
            let mut out = String::from("\n** Assistant\n");
            for resp in &msg.responses {
                out.push_str(&format!(
                    "\n*** {}{}\n\n{}\n",
                    resp.model_name,
                    resp.selected_suffix(),
                    markdown_to_orgmode(&resp.response)
                ));
            }
            out
        }
        _ => String::new(),
    }
}

fn chat_message_format(msg: &ChatMessage, format: TextFormat) -> String {
    match format {
        TextFormat::Html => chat_message_html(msg),
        TextFormat::Markdown => chat_message_markdown(msg),
        TextFormat::OrgMode => chat_message_orgmode(msg),
    }
}

// --- Chat: full document --------------------------------------------------

/// Render a full chat export document in the given format.
pub fn chat_export(chat_json: &str, format: &str) -> Result<String> {
    let format = TextFormat::parse(format)?;
    let data: ChatExportData =
        serde_json::from_str(chat_json).context("Failed to parse chat export JSON")?;

    let mut out = String::new();
    match format {
        TextFormat::Html => {
            out.push_str(concat!(
                "\n<!doctype html>\n<html>\n<head>\n",
                "    <meta charset=\"utf-8\">\n",
                "    <meta http-equiv=\"x-ua-compatible\" content=\"ie=edge\">\n",
                "    <title>Chat Export</title>\n",
                "    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n",
                "</head>\n<body>\n<h1>Chat Export</h1>\n"
            ));
            for msg in &data.messages {
                out.push_str(&chat_message_format(msg, format));
            }
            out.push_str("\n</body>\n</html>");
        }
        TextFormat::Markdown => {
            out.push_str("# Chat Export\n");
            for msg in &data.messages {
                out.push_str(&chat_message_format(msg, format));
            }
        }
        TextFormat::OrgMode => {
            out.push_str("* Chat Export\n");
            for msg in &data.messages {
                out.push_str(&chat_message_format(msg, format));
            }
        }
    }

    Ok(normalize(&out))
}

/// Render a single chat message fragment (for per-message "Copy As...").
pub fn chat_message_export(message_json: &str, format: &str) -> Result<String> {
    let format = TextFormat::parse(format)?;
    let msg: ChatMessage =
        serde_json::from_str(message_json).context("Failed to parse chat message JSON")?;
    Ok(normalize(&chat_message_format(&msg, format)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gloss_json() -> String {
        serde_json::json!({
            "text": "Evaṁ me sutaṁ.\nEkaṁ samayaṁ.",
            "paragraphs": [
                {
                    "text": "Evaṁ me sutaṁ.",
                    "vocabulary": [
                        {"word": "evaṁ", "summary": "thus; this <b>way</b>"},
                        {"word": "sutaṁ", "summary": "heard <i>(pp)</i>"}
                    ],
                    "ai_translations": [
                        {"model_name": "gemini", "response": "Thus have I heard.", "is_selected": true},
                        {"model_name": "gpt", "response": "So I heard.", "is_selected": false}
                    ]
                }
            ]
        })
        .to_string()
    }

    fn chat_json() -> String {
        serde_json::json!({
            "messages": [
                {"role": "system", "content": "You are a Pāli tutor.", "responses": []},
                {"role": "user", "content": "Translate evaṁ.", "responses": []},
                {"role": "assistant", "content": "", "responses": [
                    {"model_name": "gemini", "response": "* thus\n* so", "is_selected": true},
                    {"model_name": "gpt", "response": "thus", "is_selected": false}
                ]}
            ]
        })
        .to_string()
    }

    #[test]
    fn gloss_html_has_no_removed_headers() {
        let out = gloss_export(&gloss_json(), "html").unwrap();
        assert!(out.contains("<h1>Gloss Export</h1>"));
        // One "Paragraphs" section heading, no per-paragraph "Paragraph N".
        assert!(out.contains("<h2>Paragraphs</h2>"));
        assert!(!out.contains("Paragraph 1"));
        // "AI Translations" is bold text, not a heading.
        assert!(out.contains("<p><b>AI Translations</b></p>"));
        assert!(out.contains("gemini (selected)"));
        assert!(out.contains("<table><tbody>"));
        assert!(out.contains("<b>evaṁ</b>"));
        // Removed clutter.
        assert!(!out.contains("Vocabulary"));
        assert!(!out.contains("Dictionary definitions from DPD"));
        // AI response was rendered from markdown.
        assert!(out.contains("Thus have I heard."));
    }

    #[test]
    fn gloss_markdown_table_and_no_headers() {
        let out = gloss_export(&gloss_json(), "markdown").unwrap();
        assert!(out.contains("# Gloss Export"));
        assert!(out.contains("## Paragraphs"));
        assert!(!out.contains("## Paragraph 1"));
        assert!(out.contains("| **evaṁ** | thus; this **way** |"));
        // "AI Translations" is bold text, not a heading.
        assert!(out.contains("**AI Translations**"));
        assert!(out.contains("### gemini (selected)"));
        assert!(!out.contains("### Vocabulary"));
        assert!(!out.contains("Dictionary definitions from DPD"));
    }

    #[test]
    fn gloss_orgmode_escapes_and_converts_response() {
        let out = gloss_export(&gloss_json(), "orgmode").unwrap();
        assert!(out.contains("* Gloss Export"));
        assert!(out.contains("** Paragraphs"));
        assert!(!out.contains("** Paragraph 1"));
        assert!(out.contains("| *evaṁ* | thus; this *way* |"));
        // "AI Translations" is bold text, not a headline.
        assert!(out.contains("*AI Translations*"));
        // The AI response is converted to Org markup, not wrapped in a
        // markdown src block.
        assert!(!out.contains("#+begin_src markdown"));
        assert!(out.contains("Thus have I heard."));
        assert!(!out.contains("*** Vocabulary"));
    }

    #[test]
    fn gloss_paragraph_fragment() {
        let data: GlossExportData = serde_json::from_str(&gloss_json()).unwrap();
        let para_json = serde_json::to_string(&serde_json::json!({
            "text": data.paragraphs[0].text,
            "vocabulary": [{"word": "evaṁ", "summary": "thus"}],
            "ai_translations": []
        }))
        .unwrap();
        let out = gloss_paragraph_export(&para_json, 3, "markdown").unwrap();
        // The per-paragraph fragment no longer carries a "Paragraph N" heading.
        assert!(!out.contains("Paragraph 3"));
        assert!(out.contains("| **evaṁ** | thus |"));
        assert!(!out.contains("Gloss Export"));
    }

    #[test]
    fn chat_markdown_roles_and_bullets() {
        let out = chat_export(&chat_json(), "markdown").unwrap();
        assert!(out.contains("# Chat Export"));
        assert!(out.contains("## System"));
        assert!(out.contains("## User"));
        assert!(out.contains("## Assistant"));
        assert!(out.contains("### gemini (selected)"));
        assert!(out.contains("> You are a Pāli tutor."));
    }

    #[test]
    fn chat_orgmode_converts_response_to_org() {
        let out = chat_export(&chat_json(), "orgmode").unwrap();
        assert!(out.contains("* Chat Export"));
        assert!(out.contains("** System"));
        assert!(!out.contains("#+begin_src markdown"));
        // The "* thus / * so" markdown list becomes an Org `- ` list.
        assert!(out.contains("- thus"));
        assert!(out.contains("- so"));
    }

    #[test]
    fn chat_html_renders_assistant_markdown() {
        let out = chat_export(&chat_json(), "html").unwrap();
        assert!(out.contains("<h1>Chat Export</h1>"));
        assert!(out.contains("<h2>Assistant</h2>"));
        assert!(out.contains("<h3>gemini (selected)</h3>"));
        // Markdown list rendered to HTML.
        assert!(out.contains("<li>thus</li>"));
    }

    #[test]
    fn chat_message_fragment_single() {
        let msg_json = serde_json::json!({
            "role": "user", "content": "Hello", "responses": []
        })
        .to_string();
        let out = chat_message_export(&msg_json, "markdown").unwrap();
        assert!(out.contains("## User"));
        assert!(out.contains("> Hello"));
        assert!(!out.contains("Chat Export"));
    }

    /// DHP 23's first rows (from DPD's example report), with one model's
    /// two-part translation.
    fn word_in_sentence_json() -> String {
        serde_json::json!({
            "text": "te jhāyino sātatikā,\nniccaṁ daḷhaparakkamā,",
            "gloss_format": "word_in_sentence",
            "title": "DHP23",
            "paragraphs": [{
                "text": "te jhāyino sātatikā,\nniccaṁ daḷhaparakkamā,",
                "ai_translations": [{
                    "model_name": "gemini",
                    "response": "Those wise ones, meditative, persevering.",
                    "literal_translation": "Those meditators, persevering.",
                    "is_selected": true
                }],
                "rows": [
                    {"word": "te", "depth": 0, "id": 29188, "grammar": "masc nom pl of ta", "meaning": "those", "construction": "", "root": "", "confidence": "A"},
                    {"word": "jhāyino", "depth": 0, "id": 28829, "grammar": "masc nom pl of jhāyī", "meaning": "meditators", "construction": "√jhā + ī", "root": "√jhā 3 ya (think, meditate)", "confidence": null},
                    {"word": "daḷhaparakkamā", "depth": 0, "id": 32215, "grammar": "adj, masc nom pl of daḷhaparakkama", "meaning": "having firm effort", "construction": "daḷha + parakkama", "root": "", "confidence": "N"},
                    {"word": "daḷha", "depth": 1, "id": 32201, "grammar": "pp", "meaning": "firm | strong", "construction": "√dah + ta", "root": "√dah 1 a (be strong)", "confidence": "H"},
                    {"word": "xyz", "depth": 0, "id": null, "grammar": "", "meaning": "not found", "construction": "", "root": "", "confidence": null}
                ]
            }]
        })
        .to_string()
    }

    #[test]
    fn word_in_sentence_markdown_has_dpd_report_structure() {
        let out = gloss_export(&word_in_sentence_json(), "markdown").unwrap();
        let expected = concat!(
            "# Analysis of: DHP23\n\n",
            "te jhāyino sātatikā,\nniccaṁ daḷhaparakkamā,\n\n",
            "### English Translation\n\n",
            "**Translation:** Those wise ones, meditative, persevering.\n\n",
            "**Literal Translation:** Those meditators, persevering.\n\n",
            "### Word-by-Word Analysis\n\n",
            "| ID | Word in Sentence | Grammar | Meaning | Construction | Root | Conf. |\n",
            "| :--- | :--- | :--- | :--- | :--- | :--- | :--- |\n",
            "| 29188 | te | masc nom pl of ta | those |  |  | A |\n",
            "| 28829 | jhāyino | masc nom pl of jhāyī | meditators | √jhā + ī | √jhā 3 ya (think, meditate) |  |\n",
            "| 32215 | daḷhaparakkamā | adj, masc nom pl of daḷhaparakkama | having firm effort | daḷha + parakkama |  | N |\n",
            "| 32201 | - daḷha | pp | firm \\| strong | √dah + ta | √dah 1 a (be strong) | H |\n",
            "|  | xyz |  | not found |  |  |  |\n\n",
            "Confidence marks:\n\n",
            "- N: Not checked",
        );
        assert!(out.starts_with(expected), "got:\n{}", out);
        assert!(out.contains("- H: Human-checked"));
        // No Variants without `//` groups, no no-AI note unless asked for.
        assert!(!out.contains("### Variants"));
        assert!(!out.contains(NO_AI_PROVIDER_NOTE));
        assert!(!out.contains("Gloss Export"));
    }

    #[test]
    fn word_in_sentence_without_translation_has_no_ai_note_and_variants() {
        let mut data: GlossExportData = serde_json::from_str(&word_in_sentence_json()).unwrap();
        data.no_ai_provider = true;
        data.title = String::new();
        data.paragraphs[0].ai_translations.clear();
        data.paragraphs[0].text = "yena lobhena luddhāse, sattā gacchan'ti//gacchanti duggatiṁ".to_string();
        data.paragraphs[0].variants = vec![vec!["gacchan'ti".to_string(), "gacchanti".to_string()]];
        let out = gloss_export_document(&data, TextFormat::Markdown);
        // The title falls back to the text's first words.
        assert!(out.starts_with("# Analysis of: te jhāyino sātatikā, niccaṁ…\n"));
        assert!(!out.contains("English Translation"));
        // The passage shows the glossed spelling; Variants lists the group.
        assert!(out.contains("\nyena lobhena luddhāse, sattā gacchan'ti duggatiṁ\n"));
        let variants = out.find("### Variants\n\ngacchan'ti//gacchanti\n").unwrap();
        assert!(variants < out.find("### Word-by-Word Analysis").unwrap());
        assert!(out.ends_with(NO_AI_PROVIDER_NOTE));
    }

    #[test]
    fn word_in_sentence_paragraphs_get_headings_when_several() {
        let mut data: GlossExportData = serde_json::from_str(&word_in_sentence_json()).unwrap();
        data.paragraphs.push(data.paragraphs[0].clone());
        let out = gloss_export_document(&data, TextFormat::Markdown);
        assert!(out.contains("## Paragraph 1\n"));
        assert!(out.contains("## Paragraph 2\n"));
        // One legend, at the end.
        assert_eq!(out.matches("- N: Not checked").count(), 1);
    }

    #[test]
    fn word_in_sentence_html_and_org() {
        let html = gloss_export(&word_in_sentence_json(), "html").unwrap();
        assert!(html.contains("<title>Analysis of: DHP23</title>"));
        assert!(html.contains("<h1>Analysis of: DHP23</h1>"));
        assert!(html.contains("<h3>English Translation</h3>"));
        assert!(html.contains("<th>Conf.</th>"));
        assert!(html.contains("<td>- daḷha</td><td>pp</td><td>firm | strong</td>"));
        // te: shaded; jhāyino: plain; daḷhaparakkamā and its component: shaded; xyz: plain.
        let shaded = format!("<tr style=\"background-color: #{WORD_TABLE_SHADE_HEX};\">");
        assert_eq!(html.matches(&shaded).count(), 3);
        assert!(html.contains(&format!("{shaded}<td>32201</td><td>- daḷha</td>")));
        assert!(html.contains("<tr><td></td><td>xyz</td>"));
        assert!(html.contains("<li>A: AI-checked"));

        let org = gloss_export(&word_in_sentence_json(), "org").unwrap();
        assert!(org.starts_with("* Analysis of: DHP23\n"));
        assert!(org.contains("*** English Translation"));
        assert!(org.contains("*Translation:* Those wise ones"));
        assert!(org.contains("| ID | Word in Sentence | Grammar | Meaning | Construction | Root | Conf. |\n|---+---+---+---+---+---+---|\n"));
        assert!(org.contains("| 32201 | - daḷha | pp | firm \\vert{} strong |"));
    }

    #[test]
    fn word_in_sentence_paragraph_fragment() {
        let data: GlossExportData = serde_json::from_str(&word_in_sentence_json()).unwrap();
        let para_json = serde_json::to_string(&data.paragraphs[0]).unwrap();
        let out = gloss_paragraph_export(&para_json, 1, "markdown").unwrap();
        assert!(out.starts_with("te jhāyino"));
        assert!(out.contains("| 29188 | te |"));
        assert!(out.contains("- N: Not checked"));
        assert!(!out.contains("Analysis of"));
    }

    #[test]
    fn headwords_gains_conf_column_legend_and_literal_translation() {
        let mut data: GlossExportData = serde_json::from_str(&gloss_json()).unwrap();
        data.paragraphs[0].vocabulary[0].confidence = Some(crate::gloss_rows::Confidence::A);
        data.paragraphs[0].ai_translations[0].literal_translation = "Thus by me heard.".to_string();
        let out = gloss_export_document(&data, TextFormat::Markdown);
        assert!(out.starts_with("# Gloss Export"));
        assert!(out.contains("|    |    |    |\n|----|----|----|\n| **evaṁ** | thus; this **way** | A |\n| **sutaṁ** | heard *(pp)* |  |"));
        assert!(out.contains("### gemini (selected)\n\n**Translation:**\n\n> Thus have I heard.\n\n**Literal Translation:**\n\n> Thus by me heard."));
        // A translation without a literal one keeps the old layout.
        assert!(out.contains("### gpt\n\n> So I heard."));
        assert!(out.contains("- A: AI-checked"));

        // Without confidence values the old two-column layout stays.
        let old = gloss_export(&gloss_json(), "markdown").unwrap();
        assert!(old.contains("|    |    |\n|----|----|\n| **evaṁ** | thus; this **way** |\n"));
        assert!(!old.contains("Not checked"));
    }

    #[test]
    fn unknown_format_is_error() {
        assert!(gloss_export("{}", "pdf").is_err());
        assert!(chat_export("{}", "").is_err());
    }

    #[test]
    fn invalid_json_is_error() {
        assert!(gloss_export("not json", "html").is_err());
        assert!(chat_export("not json", "html").is_err());
    }
}

//! Text exports (HTML / Markdown / Org-Mode) for the Gloss and Prompts tabs.
//!
//! The formatting was ported from the QML `format_paragraph_*` /
//! `message_as_*` helpers so it can be unit-tested against fixed JSON. The
//! input JSON is collected in QML (`gloss_export_data()` /
//! `chat_export_data()`); the same structs are shared with the DOCX exporter
//! (see [`crate::export_types`]).

use anyhow::{bail, Context, Result};
use lazy_static::lazy_static;
use regex::Regex;

use crate::export_types::{
    ChatExportData, ChatMessage, GlossExportData, GlossExportParagraph,
};
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

// --- Gloss: per-paragraph -------------------------------------------------

fn gloss_paragraph_html(paragraph: &GlossExportParagraph, number: usize) -> String {
    let para_text = format!(
        "\n<blockquote>\n{}\n</blockquote>\n",
        paragraph.text.replace('\n', "<br>\n")
    );

    let mut table_rows = String::new();
    for res in &paragraph.vocabulary {
        table_rows.push_str(&format!(
            "<tr><td> <b>{}</b> </td><td> {} </td></tr>\n",
            res.word, res.summary
        ));
    }

    let mut ai_section = String::new();
    if !paragraph.ai_translations.is_empty() {
        ai_section.push_str("\n<h3>AI Translations</h3>\n");
        for trans in &paragraph.ai_translations {
            let html = markdown_to_html(&trans.response);
            ai_section.push_str(&format!(
                "<h4>{}{}</h4>\n<blockquote>{}</blockquote>\n",
                trans.model_name,
                trans.selected_suffix(),
                html
            ));
        }
    }

    format!(
        "\n<h2>Paragraph {number}</h2>\n\n{para_text}\n\n{ai_section}\n\n<table><tbody>\n{table_rows}\n</tbody></table>\n"
    )
}

fn gloss_paragraph_markdown(paragraph: &GlossExportParagraph, number: usize) -> String {
    let para_text = format!("\n> {}", paragraph.text.replace('\n', "\n> "));

    let mut table_rows = String::new();
    for res in &paragraph.vocabulary {
        table_rows.push_str(&format!(
            "| **{}** | {} |\n",
            res.word,
            summary_html_to_md(&res.summary)
        ));
    }

    let mut ai_section = String::new();
    if !paragraph.ai_translations.is_empty() {
        ai_section.push_str("\n### AI Translations\n");
        for trans in &paragraph.ai_translations {
            ai_section.push_str(&format!(
                "\n#### {}{}\n\n> {}\n",
                trans.model_name,
                trans.selected_suffix(),
                trans.response.replace('\n', "\n> ")
            ));
        }
    }

    format!(
        "\n## Paragraph {number}\n\n{para_text}\n\n{ai_section}\n\n|    |    |\n|----|----|\n{table_rows}\n"
    )
}

fn gloss_paragraph_orgmode(paragraph: &GlossExportParagraph, number: usize) -> String {
    let para_text = format!("\n#+begin_quote\n{}\n#+end_quote\n", paragraph.text);

    let mut table_rows = String::new();
    for res in &paragraph.vocabulary {
        table_rows.push_str(&format!(
            "| *{}* | {} |\n",
            res.word,
            summary_html_to_orgmode(&res.summary)
        ));
    }

    let mut ai_section = String::new();
    if !paragraph.ai_translations.is_empty() {
        ai_section.push_str("\n*** AI Translations\n");
        for trans in &paragraph.ai_translations {
            ai_section.push_str(&format!(
                "\n**** {}{}\n\n{}\n",
                trans.model_name,
                trans.selected_suffix(),
                markdown_to_orgmode(&trans.response)
            ));
        }
    }

    format!("\n** Paragraph {number}\n\n{para_text}\n\n{ai_section}\n\n{table_rows}\n")
}

fn gloss_paragraph_format(
    paragraph: &GlossExportParagraph,
    number: usize,
    format: TextFormat,
) -> String {
    match format {
        TextFormat::Html => gloss_paragraph_html(paragraph, number),
        TextFormat::Markdown => gloss_paragraph_markdown(paragraph, number),
        TextFormat::OrgMode => gloss_paragraph_orgmode(paragraph, number),
    }
}

// --- Gloss: full document -------------------------------------------------

/// Render a full gloss export document in the given format.
pub fn gloss_export(gloss_json: &str, format: &str) -> Result<String> {
    let format = TextFormat::parse(format)?;
    let data: GlossExportData =
        serde_json::from_str(gloss_json).context("Failed to parse gloss export JSON")?;

    let mut out = String::new();
    match format {
        TextFormat::Html => {
            let main_text = format!(
                "\n<blockquote>\n{}\n</blockquote>\n",
                data.text.replace('\n', "<br>\n")
            );
            out.push_str(&format!(
                concat!(
                    "\n<!doctype html>\n<html>\n<head>\n",
                    "    <meta charset=\"utf-8\">\n",
                    "    <meta http-equiv=\"x-ua-compatible\" content=\"ie=edge\">\n",
                    "    <title>Gloss Export</title>\n",
                    "    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n",
                    "</head>\n<body>\n<h1>Gloss Export</h1>\n\n{}\n"
                ),
                main_text
            ));
            for (i, para) in data.paragraphs.iter().enumerate() {
                out.push_str(&gloss_paragraph_format(para, i + 1, format));
            }
            out.push_str("\n</body>\n</html>");
        }
        TextFormat::Markdown => {
            let main_text = format!("\n> {}", data.text.replace('\n', "\n> "));
            out.push_str(&format!("\n# Gloss Export\n\n{}\n", main_text));
            for (i, para) in data.paragraphs.iter().enumerate() {
                out.push_str(&gloss_paragraph_format(para, i + 1, format));
            }
        }
        TextFormat::OrgMode => {
            let main_text = format!("\n#+begin_quote\n{}\n#+end_quote\n", data.text);
            out.push_str(&format!("\n* Gloss Export\n\n{}\n", main_text));
            for (i, para) in data.paragraphs.iter().enumerate() {
                out.push_str(&gloss_paragraph_format(para, i + 1, format));
            }
        }
    }

    Ok(normalize(&out))
}

/// Render a single gloss paragraph fragment (for per-paragraph "Copy As...").
pub fn gloss_paragraph_export(
    paragraph_json: &str,
    paragraph_number: usize,
    format: &str,
) -> Result<String> {
    let format = TextFormat::parse(format)?;
    let paragraph: GlossExportParagraph =
        serde_json::from_str(paragraph_json).context("Failed to parse gloss paragraph JSON")?;
    Ok(normalize(&gloss_paragraph_format(
        &paragraph,
        paragraph_number,
        format,
    )))
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
        assert!(out.contains("<h2>Paragraph 1</h2>"));
        assert!(out.contains("<h3>AI Translations</h3>"));
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
        assert!(out.contains("## Paragraph 1"));
        assert!(out.contains("| **evaṁ** | thus; this **way** |"));
        assert!(out.contains("#### gemini (selected)"));
        assert!(!out.contains("### Vocabulary"));
        assert!(!out.contains("Dictionary definitions from DPD"));
    }

    #[test]
    fn gloss_orgmode_escapes_and_converts_response() {
        let out = gloss_export(&gloss_json(), "orgmode").unwrap();
        assert!(out.contains("* Gloss Export"));
        assert!(out.contains("** Paragraph 1"));
        assert!(out.contains("| *evaṁ* | thus; this *way* |"));
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
        assert!(out.starts_with("## Paragraph 3"));
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

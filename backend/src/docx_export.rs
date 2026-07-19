//! DOCX export for the Gloss and Prompts tabs.
//!
//! The document is generated from an embedded template
//! (`assets/docx-template/gloss-template.docx`) analogous to pandoc's
//! `--reference-doc`: the template's parts (`word/styles.xml` etc.) are kept
//! intact and only `word/document.xml` is replaced with generated content.
//!
//! Named paragraph styles defined in the template (`w:styleId` / UI name):
//! - `Title` / "Title" — document title
//! - `Heading1` / "Heading 1" — per-paragraph / per-message header
//!   ("Paragraph N", "System" / "User" / "Assistant")
//! - `Heading2` / "Heading 2" — section headers ("AI Translations", model names)
//! - `BodyText` / "Body Text" — Pāli text, chat content and AI responses
//! - `VocabEntry` / "Vocab Entry" — vocabulary table cells
//!
//! The input is the Gloss tab's `gloss_export_data()` JSON or the Prompts tab's
//! `chat_export_data()` JSON (the same data the HTML / Markdown / Org-Mode
//! exports derive from). The shared structs live in [`crate::export_types`].

use std::io::{Cursor, Write};

use anyhow::{Context, Result};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use markdown::mdast::{Code, List, Node, Table};

use crate::export_types::{
    ChatExportData, ChatMessage, GlossExportData, GlossExportParagraph, GlossExportVocabItem,
};
use crate::markdown_convert::{inline_runs, node_plain_text, parse_response, InlineRun};

static TEMPLATE_DOCX: &[u8] = include_bytes!("../../assets/docx-template/gloss-template.docx");

/// Generate the DOCX bytes for a gloss export JSON (`gloss_export_data()` shape).
pub fn generate_gloss_docx(gloss_json: &str) -> Result<Vec<u8>> {
    let data: GlossExportData =
        serde_json::from_str(gloss_json).context("Failed to parse gloss export JSON")?;
    let document_xml = generate_document_xml(&data);
    replace_document_xml(TEMPLATE_DOCX, &document_xml)
}

/// Generate the DOCX bytes for a chat export JSON (`chat_export_data()` shape).
pub fn generate_chat_docx(chat_json: &str) -> Result<Vec<u8>> {
    let data: ChatExportData =
        serde_json::from_str(chat_json).context("Failed to parse chat export JSON")?;
    let document_xml = generate_chat_document_xml(&data);
    replace_document_xml(TEMPLATE_DOCX, &document_xml)
}

/// Copy every part of the template archive except `word/document.xml`, which
/// is replaced with the generated content.
fn replace_document_xml(template_bytes: &[u8], document_xml: &str) -> Result<Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(template_bytes))
        .context("Failed to open the embedded DOCX template")?;
    let mut out = ZipWriter::new(Cursor::new(Vec::new()));

    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i)?;
        if entry.name() == "word/document.xml" {
            continue;
        }
        out.raw_copy_file(entry)?;
    }

    out.start_file("word/document.xml", SimpleFileOptions::default())?;
    out.write_all(document_xml.as_bytes())?;

    let cursor = out.finish().context("Failed to finalize the DOCX archive")?;
    Ok(cursor.into_inner())
}

/// Wrap the generated `<w:p>` / `<w:tbl>` runs in the document/section skeleton.
fn wrap_document_body(body: &str) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{}"#,
            r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/>"#,
            r#"<w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1134" w:header="708" w:footer="708" w:gutter="0"/>"#,
            r#"</w:sectPr></w:body></w:document>"#,
        ),
        body
    )
}

fn generate_document_xml(data: &GlossExportData) -> String {
    let mut body = String::new();

    body.push_str(&styled_paragraph("Title", &[run(false, false, "Gloss Export")]));

    for line in data.text.lines().filter(|l| !l.trim().is_empty()) {
        body.push_str(&styled_paragraph("BodyText", &[run(false, false, line.trim())]));
    }

    for (i, paragraph) in data.paragraphs.iter().enumerate() {
        body.push_str(&format_paragraph(paragraph, i + 1));
    }

    wrap_document_body(&body)
}

fn generate_chat_document_xml(data: &ChatExportData) -> String {
    let mut body = String::new();

    body.push_str(&styled_paragraph("Title", &[run(false, false, "Chat Export")]));

    for message in &data.messages {
        body.push_str(&format_message(message));
    }

    wrap_document_body(&body)
}

fn format_message(message: &ChatMessage) -> String {
    let mut out = String::new();

    let heading = match message.role.as_str() {
        "system" => "System",
        "user" => "User",
        "assistant" => "Assistant",
        _ => return out,
    };
    out.push_str(&styled_paragraph("Heading1", &[run(false, false, heading)]));

    if message.role == "assistant" {
        for resp in &message.responses {
            out.push_str(&styled_paragraph(
                "Heading2",
                &[run(false, false, &format!("{}{}", resp.model_name, resp.selected_suffix()))],
            ));
            out.push_str(&markdown_to_docx_body(&resp.response));
        }
    } else {
        for line in message.content.lines().filter(|l| !l.trim().is_empty()) {
            out.push_str(&styled_paragraph("BodyText", &[run(false, false, line.trim())]));
        }
    }

    out
}

fn format_paragraph(paragraph: &GlossExportParagraph, number: usize) -> String {
    let mut out = String::new();

    out.push_str(&styled_paragraph(
        "Heading1",
        &[run(false, false, &format!("Paragraph {}", number))],
    ));

    for line in paragraph.text.lines().filter(|l| !l.trim().is_empty()) {
        out.push_str(&styled_paragraph("BodyText", &[run(false, false, line.trim())]));
    }

    if !paragraph.ai_translations.is_empty() {
        out.push_str(&styled_paragraph("Heading2", &[run(false, false, "AI Translations")]));
        for trans in &paragraph.ai_translations {
            out.push_str(&styled_paragraph(
                "BodyText",
                &[run(true, false, &format!("{}{}", trans.model_name, trans.selected_suffix()))],
            ));
            out.push_str(&markdown_to_docx_body(&trans.response));
        }
    }

    if !paragraph.vocabulary.is_empty() {
        out.push_str(&format_vocab_table(&paragraph.vocabulary));
    }

    out
}

/// Render the vocabulary as a two-column table (word | definition), mirroring
/// the Markdown / Org-Mode exports.
fn format_vocab_table(vocabulary: &[GlossExportVocabItem]) -> String {
    let mut rows = String::new();
    for vocab in vocabulary {
        let word_cell = table_cell("VocabEntry", "2500", "dxa", &[run(true, false, &vocab.word)]);
        let summary_cell =
            table_cell("VocabEntry", "7000", "dxa", &summary_html_to_runs(&vocab.summary));
        rows.push_str(&format!("<w:tr>{}{}</w:tr>", word_cell, summary_cell));
    }

    bordered_table(&rows)
}

/// Wrap pre-built `<w:tr>` rows in an auto-width, single-bordered table.
/// Used by the vocabulary table (cells carry explicit dxa widths).
fn bordered_table(rows: &str) -> String {
    bordered_table_with(r#"<w:tblW w:w="0" w:type="auto"/>"#, "", rows)
}

/// Single-bordered table with a caller-chosen `w:tblW` and optional
/// `w:tblGrid`. Shared by the vocabulary table and markdown tables.
fn bordered_table_with(tblw: &str, grid: &str, rows: &str) -> String {
    format!(
        concat!(
            r#"<w:tbl><w:tblPr>"#,
            "{}",
            r#"<w:tblBorders>"#,
            r#"<w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#,
            r#"<w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#,
            r#"<w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#,
            r#"<w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#,
            r#"<w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#,
            r#"<w:insideV w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#,
            r#"</w:tblBorders>"#,
            r#"</w:tblPr>{}{}</w:tbl>"#,
        ),
        tblw, grid, rows
    )
}

fn table_cell(style_id: &str, width_twips: &str, width_type: &str, runs: &[String]) -> String {
    format!(
        concat!(
            r#"<w:tc><w:tcPr><w:tcW w:w="{}" w:type="{}"/></w:tcPr>"#,
            r#"<w:p><w:pPr><w:pStyle w:val="{}"/></w:pPr>{}</w:p></w:tc>"#,
        ),
        width_twips,
        width_type,
        style_id,
        runs.concat()
    )
}

fn styled_paragraph(style_id: &str, runs: &[String]) -> String {
    styled_paragraph_indent(style_id, 0, runs)
}

/// Paragraph with an optional left indent (`w:ind`), used for nested list
/// levels and blockquotes. An indent of 0 emits no `w:ind` element.
fn styled_paragraph_indent(style_id: &str, indent_twips: u32, runs: &[String]) -> String {
    let ind = if indent_twips == 0 {
        String::new()
    } else {
        format!(r#"<w:ind w:left="{}"/>"#, indent_twips)
    };
    format!(
        r#"<w:p><w:pPr><w:pStyle w:val="{}"/>{}</w:pPr>{}</w:p>"#,
        style_id,
        ind,
        runs.concat()
    )
}

fn run(bold: bool, italic: bool, text: &str) -> String {
    run_props(bold, italic, false, text).unwrap_or_default()
}

/// A styled text run. `code` switches the run to a monospace font. Newlines in
/// `text` become `<w:br/>` elements inside the run. Returns `None` for empty
/// text.
fn run_props(bold: bool, italic: bool, code: bool, text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    let mut rpr = String::new();
    if code {
        // rFonts must precede w:b / w:i in the CT_RPr element order.
        rpr.push_str(r#"<w:rFonts w:ascii="Consolas" w:hAnsi="Consolas" w:cs="Courier New"/>"#);
    }
    if bold {
        rpr.push_str("<w:b/>");
    }
    if italic {
        rpr.push_str("<w:i/>");
    }
    let rpr = if rpr.is_empty() {
        String::new()
    } else {
        format!("<w:rPr>{}</w:rPr>", rpr)
    };
    let segments: Vec<String> = text
        .split('\n')
        .map(|seg| format!(r#"<w:t xml:space="preserve">{}</w:t>"#, escape_xml(seg)))
        .collect();
    Some(format!("<w:r>{}{}</w:r>", rpr, segments.join("<w:br/>")))
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Decode the small set of HTML entities that occur in gloss summaries so
/// they are not double-escaped into literal `&amp;amp;` in the XML.
fn decode_html_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Convert a gloss summary (plain text with optional `<b>`/`<i>` markup, as
/// produced by the DPD lookup) into DOCX runs. Any other tags are stripped.
fn summary_html_to_runs(summary: &str) -> Vec<String> {
    let mut runs: Vec<String> = Vec::new();
    let mut bold: i32 = 0;
    let mut italic: i32 = 0;
    let mut text = String::new();
    let mut rest = summary;

    let flush = |text: &mut String, bold: i32, italic: i32, runs: &mut Vec<String>| {
        if !text.is_empty() {
            runs.push(run(bold > 0, italic > 0, &decode_html_entities(text)));
            text.clear();
        }
    };

    while let Some(open) = rest.find('<') {
        text.push_str(&rest[..open]);
        rest = &rest[open..];
        let Some(close) = rest.find('>') else {
            // Unterminated tag: keep as literal text.
            text.push_str(rest);
            rest = "";
            break;
        };
        let tag = rest[1..close].trim().to_lowercase();
        rest = &rest[close + 1..];
        match tag.as_str() {
            "b" | "strong" => {
                flush(&mut text, bold, italic, &mut runs);
                bold += 1;
            }
            "/b" | "/strong" => {
                flush(&mut text, bold, italic, &mut runs);
                bold -= 1;
            }
            "i" | "em" => {
                flush(&mut text, bold, italic, &mut runs);
                italic += 1;
            }
            "/i" | "/em" => {
                flush(&mut text, bold, italic, &mut runs);
                italic -= 1;
            }
            _ => {
                // Strip unknown tags but keep their surrounding text.
            }
        }
    }
    text.push_str(rest);
    flush(&mut text, bold, italic, &mut runs);

    runs
}

// --- Markdown → OOXML body emitter -----------------------------------------

/// Twips of left indent per nesting level (lists, blockquotes).
const INDENT_STEP_TWIPS: u32 = 360;

/// Convert a Markdown AI/assistant response to OOXML `<w:p>` / `<w:tbl>`
/// fragments. On parse failure, falls back to one `BodyText` paragraph per
/// non-empty raw line.
pub fn markdown_to_docx_body(text: &str) -> String {
    match parse_response(text) {
        Some(Node::Root(root)) => root.children.iter().map(|n| docx_block(n, 0)).collect(),
        _ => docx_raw_fallback(text),
    }
}

/// Raw-text fallback when the response cannot be parsed as Markdown.
fn docx_raw_fallback(text: &str) -> String {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| styled_paragraph("BodyText", &[run(false, false, l.trim())]))
        .collect()
}

fn docx_block(node: &Node, level: usize) -> String {
    let indent = INDENT_STEP_TWIPS * level as u32;
    match node {
        Node::Paragraph(p) => {
            styled_paragraph_indent("BodyText", indent, &inline_runs_xml(&inline_runs(&p.children)))
        }
        // No Word Heading styles for response content: a bold BodyText
        // paragraph, so response headings cannot interleave with the
        // exporter's own document outline.
        Node::Heading(h) => styled_paragraph_indent(
            "BodyText",
            indent,
            &inline_runs_xml_bold(&inline_runs(&h.children)),
        ),
        Node::List(list) => docx_list(list, level),
        Node::Code(code) => docx_code(code, indent),
        Node::Table(table) => docx_markdown_table(table),
        Node::Blockquote(quote) => quote
            .children
            .iter()
            .map(|child| docx_block(child, level + 1))
            .collect(),
        Node::ThematicBreak(_) => concat!(
            r#"<w:p><w:pPr><w:pStyle w:val="BodyText"/>"#,
            r#"<w:pBdr><w:bottom w:val="single" w:sz="6" w:space="1" w:color="auto"/></w:pBdr>"#,
            r#"</w:pPr></w:p>"#,
        )
        .to_string(),
        other => {
            let text = node_plain_text(other);
            text.lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| styled_paragraph_indent("BodyText", indent, &[run(false, false, l.trim())]))
                .collect()
        }
    }
}

/// Render flattened inline runs as OOXML run elements. Links become
/// `text (url)`; autolinks (text == url) just the url.
fn inline_runs_xml(runs: &[InlineRun]) -> Vec<String> {
    runs.iter().filter_map(inline_run_xml).collect()
}

/// Same, with bold forced on every run (headings, table header row).
fn inline_runs_xml_bold(runs: &[InlineRun]) -> Vec<String> {
    runs.iter()
        .filter_map(|r| inline_run_xml(&InlineRun { bold: true, ..r.clone() }))
        .collect()
}

fn inline_run_xml(r: &InlineRun) -> Option<String> {
    let text = match &r.link_url {
        Some(url) if is_autolink(&r.text, url) => r.text.clone(),
        Some(url) => format!("{} ({})", r.text, url),
        None => r.text.clone(),
    };
    run_props(r.bold, r.italic, r.code, &text)
}

/// GFM autolinks carry no distinct link text: the url is the text, possibly
/// with an inferred `http://` / `mailto:` prefix (`www.example.com`,
/// `<user@x.y>`). Repeating the url in parentheses would just be noise.
fn is_autolink(text: &str, url: &str) -> bool {
    url == text
        || ["http://", "https://", "mailto:"]
            .iter()
            .any(|scheme| url.strip_prefix(scheme) == Some(text))
}

fn docx_list(list: &List, level: usize) -> String {
    let mut out = String::new();
    let indent = INDENT_STEP_TWIPS * (level as u32 + 1);
    let mut counter: u64 = u64::from(list.start.unwrap_or(1));
    for item in &list.children {
        let Node::ListItem(li) = item else { continue };
        let marker = if list.ordered {
            let m = format!("{}. ", counter);
            counter += 1;
            m
        } else {
            "- ".to_string()
        };
        // The literal marker is prepended to the item's first paragraph; if
        // the item starts with some other block (code, nested list), the
        // marker gets its own paragraph so it is not lost.
        let mut marker_pending = true;
        if !matches!(li.children.first(), Some(Node::Paragraph(_))) {
            out.push_str(&styled_paragraph_indent(
                "BodyText",
                indent,
                &[run(false, false, marker.trim_end())],
            ));
            marker_pending = false;
        }
        for block in &li.children {
            match block {
                Node::Paragraph(p) => {
                    let mut runs: Vec<String> = Vec::new();
                    if marker_pending {
                        runs.push(run(false, false, &marker));
                        marker_pending = false;
                    }
                    runs.extend(inline_runs_xml(&inline_runs(&p.children)));
                    out.push_str(&styled_paragraph_indent("BodyText", indent, &runs));
                }
                Node::List(inner) => out.push_str(&docx_list(inner, level + 1)),
                other => out.push_str(&docx_block(other, level + 1)),
            }
        }
    }
    out
}

/// One monospace `BodyText` paragraph per code line; blank lines are
/// preserved as empty paragraphs.
fn docx_code(code: &Code, indent: u32) -> String {
    code.value
        .lines()
        .map(|line| {
            let runs: Vec<String> = run_props(false, false, true, line).into_iter().collect();
            styled_paragraph_indent("BodyText", indent, &runs)
        })
        .collect()
}

/// Usable text width in twips for the page geometry in `wrap_document_body`
/// (A4 11906 minus 2 × 1134 margins). Markdown table columns share it evenly
/// so the table never extends past the page edge.
const TABLE_TEXT_WIDTH_TWIPS: u32 = 9638;

fn docx_markdown_table(table: &Table) -> String {
    let ncols = table
        .children
        .iter()
        .filter_map(|row| match row {
            Node::TableRow(r) => Some(r.children.len()),
            _ => None,
        })
        .max()
        .unwrap_or(1)
        .max(1);
    let col_width = (TABLE_TEXT_WIDTH_TWIPS / ncols as u32).to_string();

    let mut rows = String::new();
    for (row_idx, row) in table.children.iter().enumerate() {
        let Node::TableRow(row) = row else { continue };
        let mut cells = String::new();
        for cell in &row.children {
            let runs = match cell {
                Node::TableCell(tc) => inline_runs(&tc.children),
                _ => Vec::new(),
            };
            let run_strs = if row_idx == 0 {
                inline_runs_xml_bold(&runs)
            } else {
                inline_runs_xml(&runs)
            };
            cells.push_str(&table_cell("BodyText", &col_width, "dxa", &run_strs));
        }
        rows.push_str(&format!("<w:tr>{}</w:tr>", cells));
    }

    let grid = format!(
        "<w:tblGrid>{}</w:tblGrid>",
        format!(r#"<w:gridCol w:w="{}"/>"#, col_width).repeat(ncols)
    );
    bordered_table_with(r#"<w:tblW w:w="5000" w:type="pct"/>"#, &grid, &rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn read_part(docx: &[u8], name: &str) -> String {
        let mut archive = ZipArchive::new(Cursor::new(docx)).unwrap();
        let mut file = archive.by_name(name).unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        content
    }

    fn sample_json() -> String {
        serde_json::json!({
            "text": "Evaṁ me sutaṁ — ekaṁ samayaṁ bhagavā sāvatthiyaṁ viharati\njetavane anāthapiṇḍikassa ārāme.",
            "paragraphs": [
                {
                    "text": "Evaṁ me sutaṁ — ekaṁ samayaṁ bhagavā sāvatthiyaṁ viharati jetavane anāthapiṇḍikassa ārāme.",
                    "vocabulary": [
                        {"uid": "ārāma-4/dpd", "word": "ārāme", "summary": "<b>park</b>; monastery <i>(loc sg)</i> & more"},
                        {"uid": "evaṁ/dpd", "word": "evaṁ", "summary": "thus; this <b>way</b>"}
                    ],
                    "ai_translations": [
                        {"model_name": "gemini-2.5-flash", "response": "Thus have I heard.\nAt one time…", "is_selected": true}
                    ]
                },
                {
                    "text": "Dutiyaṁ paragraph.",
                    "vocabulary": [],
                    "ai_translations": []
                }
            ]
        })
        .to_string()
    }

    #[test]
    fn test_generate_gloss_docx_structure() {
        let docx = generate_gloss_docx(&sample_json()).unwrap();

        // Output unzips and keeps the template's other parts intact.
        let styles = read_part(&docx, "word/styles.xml");
        for style_id in ["Title", "Heading1", "Heading2", "BodyText", "VocabEntry"] {
            assert!(styles.contains(&format!("w:styleId=\"{}\"", style_id)));
        }
        read_part(&docx, "[Content_Types].xml");
        read_part(&docx, "_rels/.rels");
        read_part(&docx, "word/_rels/document.xml.rels");

        let doc = read_part(&docx, "word/document.xml");
        // Well-formed XML (parses as a document).
        roxmltree_lite_check(&doc);

        assert!(doc.contains("Gloss Export"));
        assert!(doc.contains("Paragraph 1"));
        assert!(doc.contains("Paragraph 2"));
        assert!(doc.contains("anāthapiṇḍikassa ārāme."));
        assert!(doc.contains("AI Translations"));
        assert!(doc.contains("gemini-2.5-flash (selected)"));
        assert!(doc.contains("Thus have I heard."));
        // Vocabulary is rendered as a table, not a "Vocabulary" heading.
        assert!(!doc.contains("Vocabulary"));
        assert!(!doc.contains("Dictionary definitions from DPD"));
        assert!(doc.contains("<w:tbl>"));
        assert!(doc.contains("ārāme"));
        // Escaped ampersand from the summary text.
        assert!(doc.contains("&amp; more"));
        // Summary markup became runs, not literal tags.
        assert!(!doc.contains("&lt;b&gt;"));
        assert!(doc.contains(r#"<w:rPr><w:i/></w:rPr><w:t xml:space="preserve">(loc sg)</w:t>"#));
    }

    #[test]
    fn test_empty_export() {
        let docx = generate_gloss_docx(r#"{"text": "", "paragraphs": []}"#).unwrap();
        let doc = read_part(&docx, "word/document.xml");
        roxmltree_lite_check(&doc);
        assert!(doc.contains("Gloss Export"));
    }

    fn chat_sample_json() -> String {
        serde_json::json!({
            "messages": [
                {"role": "system", "content": "You are a Pāli tutor.", "responses": []},
                {"role": "user", "content": "Translate evaṁ.\nPlease.", "responses": []},
                {"role": "assistant", "content": "", "responses": [
                    {"model_name": "gemini-2.5-flash", "response": "thus\nso", "is_selected": true},
                    {"model_name": "gpt-4o", "response": "thus", "is_selected": false}
                ]}
            ]
        })
        .to_string()
    }

    #[test]
    fn test_generate_chat_docx_structure() {
        let docx = generate_chat_docx(&chat_sample_json()).unwrap();

        let styles = read_part(&docx, "word/styles.xml");
        for style_id in ["Title", "Heading1", "Heading2", "BodyText"] {
            assert!(styles.contains(&format!("w:styleId=\"{}\"", style_id)));
        }

        let doc = read_part(&docx, "word/document.xml");
        roxmltree_lite_check(&doc);

        assert!(doc.contains("Chat Export"));
        assert!(doc.contains("System"));
        assert!(doc.contains("User"));
        assert!(doc.contains("Assistant"));
        assert!(doc.contains("You are a Pāli tutor."));
        assert!(doc.contains("gemini-2.5-flash (selected)"));
        assert!(doc.contains("gpt-4o"));
    }

    #[test]
    fn test_empty_chat_export() {
        let docx = generate_chat_docx(r#"{"messages": []}"#).unwrap();
        let doc = read_part(&docx, "word/document.xml");
        roxmltree_lite_check(&doc);
        assert!(doc.contains("Chat Export"));
    }

    #[test]
    fn test_invalid_json_is_error() {
        assert!(generate_gloss_docx("not json").is_err());
    }

    #[test]
    fn test_summary_html_to_runs_plain() {
        let runs = summary_html_to_runs("plain text, no tags");
        assert_eq!(runs.len(), 1);
        assert!(runs[0].contains("plain text, no tags"));
        assert!(!runs[0].contains("<w:b/>"));
    }

    #[test]
    fn test_summary_html_to_runs_strips_unknown_tags() {
        let runs = summary_html_to_runs("a <span class=\"x\">b</span> c");
        let joined = runs.concat();
        assert!(!joined.contains("span"));
        assert!(joined.contains("a "));
        assert!(joined.contains("b"));
    }

    #[test]
    fn test_summary_html_to_runs_entities() {
        let runs = summary_html_to_runs("fish &amp; chips");
        assert_eq!(runs.len(), 1);
        assert!(runs[0].contains("fish &amp; chips"));
        assert!(!runs[0].contains("&amp;amp;"));
    }

    // --- Markdown → OOXML emitter -------------------------------------------

    #[test]
    fn test_docx_bold_italic_inline() {
        let body = markdown_to_docx_body("**bold** and *italic* text");
        roxmltree_lite_check(&wrap_document_body(&body));
        assert!(body.contains(r#"<w:rPr><w:b/></w:rPr><w:t xml:space="preserve">bold</w:t>"#));
        assert!(body.contains(r#"<w:rPr><w:i/></w:rPr><w:t xml:space="preserve">italic</w:t>"#));
        assert!(!body.contains("**"));
    }

    #[test]
    fn test_docx_heading_is_bold_bodytext() {
        let body = markdown_to_docx_body("## Section Title");
        assert!(body.contains(r#"<w:pStyle w:val="BodyText"/>"#));
        assert!(!body.contains("Heading"));
        assert!(body.contains(r#"<w:rPr><w:b/></w:rPr><w:t xml:space="preserve">Section Title</w:t>"#));
    }

    #[test]
    fn test_docx_nested_list_indent_and_prefixes() {
        let body = markdown_to_docx_body("- alpha\n- beta\n  - inner\n- gamma");
        roxmltree_lite_check(&wrap_document_body(&body));
        assert!(body.contains(r#"<w:ind w:left="360"/>"#));
        assert!(body.contains(r#"<w:ind w:left="720"/>"#));
        assert!(body.contains(r#"<w:t xml:space="preserve">- </w:t></w:r><w:r><w:t xml:space="preserve">alpha</w:t>"#));
        assert!(body.contains(r#"<w:t xml:space="preserve">- </w:t></w:r><w:r><w:t xml:space="preserve">inner</w:t>"#));
    }

    #[test]
    fn test_docx_ordered_list_honors_start() {
        let body = markdown_to_docx_body("3. third\n4. fourth");
        assert!(body.contains(r#"<w:t xml:space="preserve">3. </w:t>"#));
        assert!(body.contains(r#"<w:t xml:space="preserve">4. </w:t>"#));
    }

    #[test]
    fn test_docx_loose_list_item_second_paragraph_same_indent() {
        let body = markdown_to_docx_body("- first para\n\n  second para\n\n- next item");
        roxmltree_lite_check(&wrap_document_body(&body));
        // Both item paragraphs at the same level; only the first has a marker.
        assert_eq!(body.matches(r#"<w:ind w:left="360"/>"#).count(), 3);
        assert_eq!(body.matches(r#"<w:t xml:space="preserve">- </w:t>"#).count(), 2);
        assert!(body.contains("second para"));
    }

    #[test]
    fn test_docx_markdown_table_bold_header() {
        let body = markdown_to_docx_body("| A | B |\n|---|---|\n| **x** | y |");
        roxmltree_lite_check(&wrap_document_body(&body));
        assert!(body.contains("<w:tbl>"));
        assert!(body.contains(r#"<w:tblBorders>"#));
        assert!(body.contains(r#"<w:pStyle w:val="BodyText"/>"#));
        // Full-text-width layout: 100% pct table, explicit equal-column grid,
        // dxa cell widths (2 columns → 9638 / 2 = 4819 twips each).
        assert!(body.contains(r#"<w:tblW w:w="5000" w:type="pct"/>"#));
        assert!(body.contains(r#"<w:tblGrid><w:gridCol w:w="4819"/><w:gridCol w:w="4819"/></w:tblGrid>"#));
        assert!(body.contains(r#"<w:tcW w:w="4819" w:type="dxa"/>"#));
        // Header cells bold.
        assert!(body.contains(r#"<w:rPr><w:b/></w:rPr><w:t xml:space="preserve">A</w:t>"#));
        assert!(body.contains(r#"<w:rPr><w:b/></w:rPr><w:t xml:space="preserve">B</w:t>"#));
        // Body row keeps its own emphasis.
        assert!(body.contains(r#"<w:rPr><w:b/></w:rPr><w:t xml:space="preserve">x</w:t>"#));
        assert!(body.contains(r#"<w:r><w:t xml:space="preserve">y</w:t></w:r>"#));
    }

    #[test]
    fn test_docx_code_block_monospace_and_blank_lines() {
        let body = markdown_to_docx_body("```python\nprint('hi')\n\nprint('there')\n```");
        roxmltree_lite_check(&wrap_document_body(&body));
        assert_eq!(body.matches(r#"<w:rFonts w:ascii="Consolas""#).count(), 2);
        assert!(body.contains("print('hi')"));
        // The blank line is preserved as an empty paragraph.
        assert!(body.contains(r#"<w:p><w:pPr><w:pStyle w:val="BodyText"/></w:pPr></w:p>"#));
    }

    #[test]
    fn test_docx_inline_code_monospace() {
        let body = markdown_to_docx_body("run `cargo test` now");
        assert!(body.contains(r#"<w:rFonts w:ascii="Consolas" w:hAnsi="Consolas" w:cs="Courier New"/>"#));
        assert!(body.contains("cargo test"));
    }

    #[test]
    fn test_docx_link_text_and_url() {
        let body = markdown_to_docx_body("see [docs](https://x.y) here");
        assert!(body.contains("docs (https://x.y)"));
    }

    #[test]
    fn test_docx_autolink_bare_url() {
        let body = markdown_to_docx_body("See https://example.org here");
        assert!(body.contains(r#"<w:t xml:space="preserve">https://example.org</w:t>"#));
        assert!(!body.contains("https://example.org (https://example.org)"));
    }

    #[test]
    fn test_docx_autolink_email_and_www_no_url_repeat() {
        let body = markdown_to_docx_body("Mail <user@x.y> or visit www.example.org today");
        assert!(body.contains("user@x.y"));
        assert!(!body.contains("(mailto:user@x.y)"));
        assert!(body.contains("www.example.org"));
        assert!(!body.contains("(http://www.example.org)"));
    }

    #[test]
    fn test_docx_blockquote_indent_and_thematic_break() {
        let body = markdown_to_docx_body("> quoted text\n\n---");
        roxmltree_lite_check(&wrap_document_body(&body));
        assert!(body.contains(r#"<w:ind w:left="360"/>"#));
        assert!(body.contains("quoted text"));
        assert!(body.contains(r#"<w:pBdr><w:bottom w:val="single" w:sz="6" w:space="1" w:color="auto"/></w:pBdr>"#));
    }

    #[test]
    fn test_docx_xml_escaping_in_response() {
        let body = markdown_to_docx_body("a < b & c");
        roxmltree_lite_check(&wrap_document_body(&body));
        assert!(body.contains("a &lt; b &amp; c"));
    }

    #[test]
    fn test_docx_soft_break_becomes_w_br() {
        let body = markdown_to_docx_body("first line\nsecond line");
        assert!(body.contains(r#"<w:t xml:space="preserve">first line</w:t><w:br/><w:t xml:space="preserve">second line</w:t>"#));
    }

    #[test]
    fn test_docx_raw_fallback_plain_paragraphs() {
        // Exercised directly: `to_mdast` practically cannot fail on plain
        // markdown, so the fallback is tested at the function level.
        let body = docx_raw_fallback("line one\n\nline two");
        roxmltree_lite_check(&wrap_document_body(&body));
        assert_eq!(body.matches("<w:p>").count(), 2);
        assert!(body.contains("line one"));
        assert!(body.contains("line two"));
    }

    #[test]
    fn test_docx_response_markdown_rendered_in_gloss_export() {
        let json = serde_json::json!({
            "text": "Evaṁ me sutaṁ.",
            "paragraphs": [{
                "text": "Evaṁ me sutaṁ.",
                "vocabulary": [],
                "ai_translations": [
                    {"model_name": "m", "response": "**Thus** have I heard.", "is_selected": true}
                ]
            }]
        })
        .to_string();
        let docx = generate_gloss_docx(&json).unwrap();
        let doc = read_part(&docx, "word/document.xml");
        roxmltree_lite_check(&doc);
        assert!(doc.contains(r#"<w:rPr><w:b/></w:rPr><w:t xml:space="preserve">Thus</w:t>"#));
        assert!(!doc.contains("**Thus**"));
    }

    /// Minimal well-formedness check without adding an XML parser dependency:
    /// tags must balance and no stray `<`/`>` may remain in text content.
    fn roxmltree_lite_check(xml: &str) {
        let mut depth: i64 = 0;
        let mut rest = xml;
        // Skip the XML declaration.
        if let Some(pos) = rest.find("?>") {
            rest = &rest[pos + 2..];
        }
        while let Some(open) = rest.find('<') {
            let text = &rest[..open];
            assert!(!text.contains('>'), "stray '>' in text: {}", text);
            rest = &rest[open..];
            let close = rest.find('>').expect("unterminated tag");
            let tag = &rest[1..close];
            assert!(!tag.contains('<'), "stray '<' in tag: {}", tag);
            if tag.starts_with('/') {
                depth -= 1;
            } else if !tag.ends_with('/') {
                depth += 1;
            }
            assert!(depth >= 0, "unbalanced tags");
            rest = &rest[close + 1..];
        }
        assert_eq!(depth, 0, "unbalanced document");
        assert!(!rest.contains('>'), "stray '>' after last tag");
    }
}


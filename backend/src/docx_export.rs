//! DOCX export for the Gloss tab.
//!
//! The document is generated from an embedded template
//! (`assets/docx-template/gloss-template.docx`) analogous to pandoc's
//! `--reference-doc`: the template's parts (`word/styles.xml` etc.) are kept
//! intact and only `word/document.xml` is replaced with generated content.
//!
//! Named paragraph styles defined in the template (`w:styleId` / UI name):
//! - `Title` / "Title" — document title
//! - `Heading1` / "Heading 1" — per-paragraph header ("Paragraph N")
//! - `Heading2` / "Heading 2" — section headers ("AI Translations", "Vocabulary")
//! - `BodyText` / "Body Text" — Pāli text and AI translations
//! - `VocabEntry` / "Vocab Entry" — vocabulary list entries
//!
//! The input is the Gloss tab's `gloss_export_data()` JSON (the same data the
//! HTML / Markdown / Org-Mode exports derive from).

use std::io::{Cursor, Write};

use anyhow::{Context, Result};
use serde::Deserialize;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

static TEMPLATE_DOCX: &[u8] = include_bytes!("../../assets/docx-template/gloss-template.docx");

#[derive(Deserialize, Debug, Default)]
pub struct GlossExportData {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub paragraphs: Vec<GlossExportParagraph>,
}

#[derive(Deserialize, Debug, Default)]
pub struct GlossExportParagraph {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub vocabulary: Vec<GlossExportVocabItem>,
    #[serde(default)]
    pub ai_translations: Vec<GlossExportTranslation>,
}

#[derive(Deserialize, Debug, Default)]
pub struct GlossExportVocabItem {
    #[serde(default)]
    pub word: String,
    #[serde(default)]
    pub summary: String,
}

#[derive(Deserialize, Debug, Default)]
pub struct GlossExportTranslation {
    #[serde(default)]
    pub model_name: String,
    #[serde(default)]
    pub response: String,
    #[serde(default)]
    pub is_selected: bool,
}

/// Generate the DOCX bytes for a gloss export JSON (`gloss_export_data()` shape).
pub fn generate_gloss_docx(gloss_json: &str) -> Result<Vec<u8>> {
    let data: GlossExportData =
        serde_json::from_str(gloss_json).context("Failed to parse gloss export JSON")?;
    let document_xml = generate_document_xml(&data);
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

fn generate_document_xml(data: &GlossExportData) -> String {
    let mut body = String::new();

    body.push_str(&styled_paragraph("Title", &[run(false, false, "Gloss Export")]));

    for line in data.text.lines().filter(|l| !l.trim().is_empty()) {
        body.push_str(&styled_paragraph("BodyText", &[run(false, false, line.trim())]));
    }

    for (i, paragraph) in data.paragraphs.iter().enumerate() {
        body.push_str(&format_paragraph(paragraph, i + 1));
    }

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
            let selected = if trans.is_selected { " (selected)" } else { "" };
            out.push_str(&styled_paragraph(
                "BodyText",
                &[run(true, false, &format!("{}{}", trans.model_name, selected))],
            ));
            // AI translation responses are exported as plain text.
            for line in trans.response.lines().filter(|l| !l.trim().is_empty()) {
                out.push_str(&styled_paragraph("BodyText", &[run(false, false, line.trim())]));
            }
        }
    }

    out.push_str(&styled_paragraph("Heading2", &[run(false, false, "Vocabulary")]));
    out.push_str(&styled_paragraph(
        "BodyText",
        &[run(true, false, "Dictionary definitions from DPD:")],
    ));

    for vocab in &paragraph.vocabulary {
        let mut runs = vec![run(true, false, &vocab.word), run(false, false, " — ")];
        runs.extend(summary_html_to_runs(&vocab.summary));
        out.push_str(&styled_paragraph("VocabEntry", &runs));
    }

    out
}

fn styled_paragraph(style_id: &str, runs: &[String]) -> String {
    format!(
        r#"<w:p><w:pPr><w:pStyle w:val="{}"/></w:pPr>{}</w:p>"#,
        style_id,
        runs.concat()
    )
}

fn run(bold: bool, italic: bool, text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut rpr = String::new();
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
    format!(
        r#"<w:r>{}<w:t xml:space="preserve">{}</w:t></w:r>"#,
        rpr,
        escape_xml(text)
    )
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
        assert!(doc.contains("Vocabulary"));
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


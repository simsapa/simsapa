//! The defects found in the source, and the markup the extraction does not
//! map.

use std::collections::BTreeMap;

/// Lines, in the order found. Defects are reported, never repaired.
#[derive(Default)]
pub struct Report {
    pub defects: Vec<String>,
    pub unmapped: BTreeMap<String, usize>,
}

impl Report {
    pub fn defect(&mut self, line: String) {
        self.defects.push(line);
    }

    pub fn unmapped(&mut self, what: String) {
        *self.unmapped.entry(what).or_default() += 1;
    }

    /// The text of `extract-report.txt`: the title, the epub, the defects,
    /// the unmapped markup, and `summary` (the book's own counts) followed by
    /// the defect and unmapped counts. No timestamps.
    pub fn render(&self, title: &str, epub_file: &str, epub_sha256: &str, summary: &str) -> String {
        let mut text = String::new();
        text.push_str(&format!("{}: extraction report\n", title));
        text.push_str(&format!("epub: {} (SHA-256 {})\n\n", epub_file, epub_sha256));
        text.push_str("Defects in the source (reported, not repaired):\n");
        if self.defects.is_empty() {
            text.push_str("(none)\n");
        }
        for line in &self.defects {
            text.push_str(&format!("- {}\n", line));
        }
        text.push_str("\nMarkup the extraction does not map (rendered as its text):\n");
        if self.unmapped.is_empty() {
            text.push_str("(none)\n");
        }
        for (what, count) in &self.unmapped {
            text.push_str(&format!("- {} ({}×)\n", what, count));
        }
        text.push_str(&format!(
            "\nSummary: {}, {} defects, {} unmapped\n",
            summary,
            self.defects.len(),
            self.unmapped.len()
        ));
        text
    }
}

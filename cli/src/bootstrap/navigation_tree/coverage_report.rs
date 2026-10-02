//! The CST coverage report.
//!
//! One report, filled in two stages because the two halves are measured at
//! different points in the bootstrap:
//!
//! 1. **The CST import** ([`crate::bootstrap::tipitaka_xml`]) records each
//!    file's coverage class, the records it produced, the files it could not
//!    split, and the `romn/` ↔ toc pairing in both directions.
//! 2. **The CST spine** ([`super::cst_spine`]) records how many toc leaves
//!    ended up assigned to a record or a populated branch, and names the ones
//!    that did not.
//!
//! It is a report, never a repair: every number here is a measurement, and a
//! non-zero unassigned count is a mapping to fix, not something the bootstrap
//! works around.
//!
//! **Every figure is a count**, so a regression shows up as a number rather
//! than as a line to be spotted in a long log. The identifiers are logged as a
//! bounded sample beside each count, which is what makes one investigable
//! without re-running the build.

use std::collections::BTreeMap;

use simsapa_backend::logger;

#[derive(Debug, Default)]
pub struct CstCoverageReport {
    /// Coverage class name → how many files fell in it.
    pub class_counts: BTreeMap<&'static str, usize>,
    /// Partially mapped files with their mapped/total fragment counts.
    pub partial_files: Vec<(String, usize, usize)>,
    /// Files the split could not partition, with the reason.
    pub unsplittable: Vec<(String, String)>,
    /// `romn/` files with no `cscd/<name>.toc.xml`.
    pub sources_without_toc: Vec<String>,
    /// `cscd/*.toc.xml` files with no `romn/` source.
    pub tocs_without_source: Vec<String>,

    pub fragment_records: usize,
    pub division_records: usize,
    pub whole_file_records: usize,

    /// Toc leaves seen by the spine, across the three built roots.
    pub toc_leaves_total: usize,
    /// Toc leaves that reached neither a record nor a populated branch.
    pub toc_leaves_unassigned: Vec<String>,
    /// Toc leaves naming a preamble or postamble section, which open the
    /// adjacent record that carries their text. Expected, not a defect.
    pub toc_leaves_merged: Vec<String>,
}

impl CstCoverageReport {
    pub fn count_class(&mut self, class_name: &'static str) {
        *self.class_counts.entry(class_name).or_insert(0) += 1;
    }

    /// The headline number: a toc leaf that opens nothing is the defect this
    /// whole report exists to make visible.
    pub fn unassigned_count(&self) -> usize {
        self.toc_leaves_unassigned.len()
    }

    pub fn log_summary(&self) {
        logger::info("=== CST coverage report ===");

        for (class, count) in &self.class_counts {
            logger::info(&format!("cst_coverage: {}: {} file(s)", class, count));
        }

        logger::info(&format!(
            "cst_coverage: records: {} fragment, {} division, {} whole-file",
            self.fragment_records, self.division_records, self.whole_file_records
        ));

        log_list("partially mapped files (mapped/total fragments)", &self
            .partial_files
            .iter()
            .map(|(f, m, t)| format!("{} {}/{}", f, m, t))
            .collect::<Vec<_>>());

        log_list(
            "files imported whole because the split failed",
            &self
                .unsplittable
                .iter()
                .map(|(f, why)| format!("{} ({})", f, why))
                .collect::<Vec<_>>(),
        );

        log_list("romn/ files with no toc", &self.sources_without_toc);
        log_list("toc files with no romn/ source", &self.tocs_without_source);

        logger::info(&format!(
            "cst_coverage: toc leaves: {} total, {} unassigned, {} merged into an adjacent record",
            self.toc_leaves_total,
            self.unassigned_count(),
            self.toc_leaves_merged.len()
        ));
        log_list("unassigned toc leaves", &self.toc_leaves_unassigned);
        log_list("toc leaves merged into an adjacent record", &self.toc_leaves_merged);
    }
}

/// A count first, then a bounded sample. A count of zero still prints, because
/// "0 unassigned toc leaves" is the measurement the success metric asks for and
/// a silently absent line is not.
fn log_list(label: &str, items: &[String]) {
    if items.is_empty() {
        logger::info(&format!("cst_coverage: {}: 0", label));
        return;
    }

    let shown: Vec<&str> = items.iter().take(12).map(|s| s.as_str()).collect();
    let more = items.len() - shown.len();
    logger::warn(&format!(
        "cst_coverage: {}: {} -- {}{}",
        label,
        items.len(),
        shown.join(", "),
        if more > 0 { format!(" (+{} more)", more) } else { String::new() }
    ));
}

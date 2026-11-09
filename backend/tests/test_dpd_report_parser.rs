//! Tests for the DPD report and `distilled.json` readers used by the Word in
//! Sentence conformance test. Pure file parsing: no database.

#[path = "helpers/dpd_report.rs"]
mod dpd_report;

use dpd_report::*;

/// Lines of the table that are rows: every `|` line after the
/// `### Word-by-Word Analysis` heading except the header and separator.
fn table_row_line_count(markdown: &str) -> usize {
    let mut in_table = false;
    let mut count = 0;
    for line in markdown.lines() {
        if line.starts_with("### ") {
            in_table = line.trim() == "### Word-by-Word Analysis";
            continue;
        }
        if in_table && line.trim_start().starts_with('|') {
            count += 1;
        }
    }
    count - 2
}

#[test]
fn test_every_report_parses_with_all_rows() {
    for path in all_report_paths() {
        let text = std::fs::read_to_string(&path).unwrap();
        let report = parse_report(&text).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));

        assert_eq!(
            report.rows.len(),
            table_row_line_count(&text),
            "{}: row count",
            path.display()
        );
        assert!(!report.title.is_empty(), "{}: title", path.display());
        assert!(!report.passage.is_empty(), "{}: passage", path.display());
        assert!(!report.translation.is_empty(), "{}: translation", path.display());
        assert!(!report.literal_translation.is_empty(), "{}: literal translation", path.display());
        assert_eq!(report.rows[0].depth, 0, "{}: first row is a token", path.display());

        // A component is never more than one level below the row above it.
        for pair in report.rows.windows(2) {
            assert!(
                pair[1].depth <= pair[0].depth + 1,
                "{}: '{}' jumps from depth {} to {}",
                path.display(),
                pair[1].word,
                pair[0].depth,
                pair[1].depth
            );
        }
    }
}

#[test]
fn test_dhp23_rows() {
    let report = read_report(&example_report_path("DHP23"));
    assert_eq!(report.title, "DHP23");
    assert!(report.passage.starts_with("te jhāyino sātatikā,\nniccaṃ daḷhaparakkamā,"));
    assert!(report.translation.starts_with("Those wise ones, meditative"));
    assert!(report.variants.is_empty());
    assert_eq!(report.rows.len(), 14);
    assert_eq!(report.token_rows().len(), 10);

    assert_eq!(
        report.rows[1],
        ReportRow {
            id: Some(28829),
            depth: 0,
            word: "jhāyino".to_string(),
            grammar: "masc nom pl of jhāyī".to_string(),
            meaning: "meditators".to_string(),
            construction: "√jhā + ī".to_string(),
            root: "√jhā 3 ya (think, meditate)".to_string(),
        }
    );

    // `te` has neither construction nor root: two empty trailing cells.
    assert_eq!(report.rows[0].word, "te");
    assert_eq!(report.rows[0].construction, "");
    assert_eq!(report.rows[0].root, "");

    let daḷha = &report.rows[5];
    assert_eq!((daḷha.id, daḷha.depth, daḷha.word.as_str()), (Some(32201), 1, "daḷha"));
    assert_eq!(daḷha.grammar, "pp");

    let yogasmā = &report.rows[11];
    assert_eq!((yogasmā.depth, yogasmā.word.as_str()), (1, "yogasmā"));
    assert_eq!(yogasmā.grammar, "masc abl sg of yoga");
}

#[test]
fn test_nested_component_depths() {
    let report = read_report(&fixture_report_path("TH215"));
    let depths: Vec<usize> = report.rows.iter().map(|r| r.depth).collect();
    assert!(depths.contains(&2), "TH215 has depth-2 rows");
    assert!(depths.contains(&3), "TH215 has depth-3 rows");

    let report = read_report(&example_report_path("UD12"));
    let deep: Vec<&ReportRow> = report.rows.iter().filter(|r| r.depth >= 2).collect();
    let taṇhakkhayassa = deep.iter().find(|r| r.word == "taṇhakkhayassa").expect("taṇhakkhayassa row");
    assert_eq!((taṇhakkhayassa.id, taṇhakkhayassa.depth), (Some(29334), 2));
    let taṇhāya = deep.iter().find(|r| r.word == "taṇhāya").expect("taṇhāya row");
    assert_eq!(taṇhāya.depth, 3);
}

#[test]
fn test_deconstruction_and_variant_rows() {
    let report = read_report(&example_report_path("SN18.1_p2"));
    assert_eq!(report.variants.len(), report.variants.iter().filter(|v| v.len() >= 2).count());
    assert!(!report.variants.is_empty());

    // A sandhi headword row and a deconstruction row (no ID).
    let vāti = report.rows.iter().find(|r| r.word == "vā'ti").expect("vā'ti row");
    assert_eq!((vāti.id, vāti.grammar.as_str()), (Some(66767), "sandhi, ind + ind"));
    let attāti = report.rows.iter().find(|r| r.word == "attā'ti").expect("attā'ti row");
    assert_eq!((attāti.id, attāti.grammar.as_str()), (None, "sandhi"));
    assert_eq!(attāti.construction, "attā + iti");

    let report = read_report(&fixture_report_path("TH215"));
    assert_eq!(
        report.variants,
        vec![vec!["accharā-saṅghāta-mattam'pi".to_string(), "accharāsaṅghātamattam'pi".to_string()]]
    );
}

#[test]
fn test_escaped_pipe_in_cell() {
    let md = "# Analysis of: X\n\ntext\n\n### English Translation\n\n**Translation:** t\n\n\
              **Literal Translation:** l\n\n### Word-by-Word Analysis\n\n\
              | ID | Word in Sentence | Grammar | Meaning | Construction | Root |\n\
              | :--- | :--- | :--- | :--- | :--- | :--- |\n\
              | 1 | a | ind | this \\| that | x + y |  |\n\
              |  | - b | pp |  |  |  |\n";
    let report = parse_report(md).unwrap();
    assert_eq!(report.rows.len(), 2);
    assert_eq!(report.rows[0].meaning, "this | that");
    assert_eq!(report.rows[0].construction, "x + y");
    assert_eq!(report.rows[1], ReportRow {
        id: None,
        depth: 1,
        word: "b".to_string(),
        grammar: "pp".to_string(),
        meaning: String::new(),
        construction: String::new(),
        root: String::new(),
    });
}

#[test]
fn test_option_keys() {
    assert_eq!(parse_option_key("w0_30148_default").unwrap(), (0, Some(30148), Reading::Default));
    assert_eq!(parse_option_key("w8_29188_2").unwrap(), (8, Some(29188), Reading::Triple(2)));
    assert_eq!(parse_option_key("w57_decon_midhekacce_0").unwrap(), (57, None, Reading::Decon(0)));
    assert_eq!(parse_option_key("w85_decon_upapajjantīti_1").unwrap(), (85, None, Reading::Decon(1)));
    assert_eq!(parse_option_key("w3_54211_inflection").unwrap(), (3, Some(54211), Reading::Inflection));
    assert!(parse_option_key("30148_default").is_err());
}

/// The tokens where DPD's `distilled.json` and `study.md` name different
/// headwords (see `tests/data/dpd_analysis/SOURCE.md`): (fixture, token index,
/// distilled id, report id).
const KNOWN_DISTILLED_REPORT_DIFFERENCES: [(&str, usize, i32, i32); 6] = [
    ("MN41_p2", 84, 55973, 55972),
    ("SN15.1_p2", 21, 27734, 27732),
    ("SN15.1_p2", 30, 36804, 36803),
    ("SN15.1_p2", 37, 8709, 13779),
    ("SN15.1_p2", 55, 89413, 71433),
    ("AN3.33_p1", 15, 12242, 12241),
];

#[test]
fn test_distilled_matches_report_tokens() {
    let mut differences = Vec::new();
    for name in FIXTURES {
        let tokens = read_distilled(&fixture_distilled_path(name));
        let report = read_report(&fixture_report_path(name));
        let token_rows = report.token_rows();

        assert_eq!(tokens.len(), token_rows.len(), "{}: token count", name);
        for (i, (token, row)) in tokens.iter().zip(token_rows.iter()).enumerate() {
            assert_eq!(token.token_index, i, "{}: token index of '{}'", name, token.word);
            assert_eq!(token.word, row.word, "{}: word of token {}", name, i);
            if let Reading::Decon(_) = token.reading {
                assert!(token.id.is_none());
            }
            if token.id != row.id {
                differences.push((name, i, token.id.unwrap_or(0), row.id.unwrap_or(0)));
            }
        }
    }
    assert_eq!(differences, KNOWN_DISTILLED_REPORT_DIFFERENCES.to_vec());

    let tokens = read_distilled(&fixture_distilled_path("DHP211"));
    assert_eq!(tokens.len(), 14);
    assert_eq!((tokens[0].word.as_str(), tokens[0].id, tokens[0].reading), ("tasmā", Some(30148), Reading::Default));
    assert_eq!((tokens[8].word.as_str(), tokens[8].reading), ("tesaṃ", Reading::Triple(2)));
    assert_eq!((tokens[9].word.as_str(), tokens[9].occurrence), ("na", 2));
}

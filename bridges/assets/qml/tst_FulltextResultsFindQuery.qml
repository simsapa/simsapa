import QtQuick
import QtTest

// Unit tests for FulltextResults.derive_find_query() — the pure per-snippet
// find-bar query derivation (matched word + following words). See
// docs/search-snippet-highlight-pipeline.md §7.
TestCase {
    id: test_case
    name: "TestFulltextResultsFindQuery"

    FulltextResults {
        id: fulltext_results
        is_dark: false
        new_results_page_fn: function(_page) {}
    }

    function test_matched_word_plus_following() {
        var snippet = "… pajahati na upādiyati <span class='match'>pajahitvā</span> ṭhito …";
        compare(fulltext_results.derive_find_query(snippet), "pajahitvā ṭhito");
    }

    function test_no_match_span_returns_empty() {
        compare(fulltext_results.derive_find_query("… pajahati na upādiyati ṭhito …"), "");
    }

    function test_empty_snippet_returns_empty() {
        compare(fulltext_results.derive_find_query(""), "");
    }

    function test_trailing_punctuation_dropped() {
        var snippet = "<span class='match'>pajahati</span> na upādiyati.";
        // Matched word + up to two following, trailing '.' stripped.
        compare(fulltext_results.derive_find_query(snippet), "pajahati na upādiyati");
    }

    function test_strips_residual_tags() {
        var snippet = "<span class='match'>pajahati</span> <i>na</i> upādiyati";
        compare(fulltext_results.derive_find_query(snippet), "pajahati na upādiyati");
    }
}

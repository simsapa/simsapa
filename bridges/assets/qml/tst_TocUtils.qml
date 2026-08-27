import QtQuick
import QtTest

// Pure-function tests for TocUtils, the matching rules that decide which TOC
// entry corresponds to the book chapter open in the reader panel. Run with the
// QML test harness (`make qml-test`) — do not run automatically; the user runs
// QML tests.
TestCase {
    id: test_case
    name: "TestTocUtils"

    TocUtils { id: toc_utils }

    // A small two-level TOC in the shape the EPUB import writes: `label`,
    // `content` (chapter path, optionally with a fragment) and `children`.
    readonly property var sample_toc: [
        { label: "Cover", content: "cover.xhtml", children: [] },
        {
            label: "Part One",
            content: "part1.xhtml",
            children: [
                { label: "Chapter 1", content: "ch1.xhtml", children: [] },
                {
                    label: "Chapter 2",
                    content: "ch2.xhtml",
                    children: [
                        { label: "Section 2.1", content: "ch2.xhtml#s1", children: [] },
                        { label: "Section 2.2", content: "ch2.xhtml#s2", children: [] }
                    ]
                }
            ]
        },
        { label: "Colophon", content: "colophon.xhtml", children: [] }
    ]

    function test_item_key_is_the_tree_position() {
        compare(toc_utils.toc_item_key([0]), "toc_0");
        compare(toc_utils.toc_item_key([1, 1, 0]), "toc_1_1_0");
    }

    // The keys must distinguish entries that share a label and a depth under
    // different parents — the defect in keying on depth + label.
    function test_item_keys_of_same_labelled_siblings_differ() {
        verify(toc_utils.toc_item_key([1, 0]) !== toc_utils.toc_item_key([2, 0]));
    }

    function test_ancestor_keys_exclude_the_entry_itself() {
        compare(toc_utils.ancestor_keys([1, 1, 0]).join(","), "toc_1,toc_1_1");
        compare(toc_utils.ancestor_keys([0]).length, 0);
    }

    function test_split_content_path() {
        compare(toc_utils.split_content_path("ch2.xhtml").file_path, "ch2.xhtml");
        compare(toc_utils.split_content_path("ch2.xhtml").anchor, "");
        compare(toc_utils.split_content_path("ch2.xhtml#s1").file_path, "ch2.xhtml");
        // The anchor keeps its '#', which is the form the reader tab stores.
        compare(toc_utils.split_content_path("ch2.xhtml#s1").anchor, "#s1");
        compare(toc_utils.split_content_path("").file_path, "");
        compare(toc_utils.split_content_path(null).file_path, "");
    }

    function test_paths_match_exactly() {
        verify(toc_utils.paths_match("OEBPS/ch1.xhtml", "OEBPS/ch1.xhtml"));
        verify(!toc_utils.paths_match("OEBPS/ch1.xhtml", "OEBPS/ch2.xhtml"));
        verify(!toc_utils.paths_match("", "ch1.xhtml"));
        verify(!toc_utils.paths_match("ch1.xhtml", ""));
    }

    function test_paths_match_tolerates_encoding_and_different_roots() {
        verify(toc_utils.paths_match("text/chapter%20one.xhtml", "text/chapter one.xhtml"));
        verify(toc_utils.paths_match("OEBPS/text/ch1.xhtml", "text/ch1.xhtml"));
        // A shared basename is the loosest rule; different names still differ.
        verify(!toc_utils.paths_match("OEBPS/text/ch1.xhtml", "text/ch10.xhtml"));
    }

    function test_resolves_a_plain_chapter() {
        compare(toc_utils.resolve_toc_path(test_case.sample_toc, "ch1.xhtml", "").join(","), "1,0");
    }

    // Without an anchor, the entry for the whole file wins over its sub-entries.
    function test_no_anchor_prefers_the_whole_file_entry() {
        compare(toc_utils.resolve_toc_path(test_case.sample_toc, "ch2.xhtml", "").join(","), "1,1");
    }

    // With an anchor, the precise sub-entry is selected.
    function test_anchor_selects_the_sub_entry() {
        compare(toc_utils.resolve_toc_path(test_case.sample_toc, "ch2.xhtml", "#s2").join(","), "1,1,1");
    }

    // An anchor the TOC does not list still selects the chapter it is in,
    // rather than nothing.
    function test_unlisted_anchor_falls_back_to_the_file() {
        compare(toc_utils.resolve_toc_path(test_case.sample_toc, "ch2.xhtml", "#s9").join(","), "1,1");
    }

    function test_unlisted_file_resolves_to_null() {
        compare(toc_utils.resolve_toc_path(test_case.sample_toc, "appendix.xhtml", ""), null);
        compare(toc_utils.resolve_toc_path(test_case.sample_toc, "", ""), null);
        compare(toc_utils.resolve_toc_path([], "ch1.xhtml", ""), null);
    }

    // TOC entries that are headings with no href must not match anything.
    function test_entries_without_content_are_skipped() {
        const toc = [
            { label: "Heading only", content: "", children: [
                { label: "Real chapter", content: "ch1.xhtml", children: [] }
            ] }
        ];
        compare(toc_utils.resolve_toc_path(toc, "ch1.xhtml", "").join(","), "0,0");
    }
}

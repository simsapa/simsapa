import QtQuick
import QtTest

// Pure-function tests for TitleUtils.clean_title(). Run with the QML test
// harness (`make qml-test`) — do not run automatically; the user runs QML tests.
TestCase {
    id: test_case
    name: "TestTitleUtils"

    TitleUtils { id: title_utils }

    function test_empty_and_null() {
        compare(title_utils.clean_title(""), "");
        compare(title_utils.clean_title(null), "");
        compare(title_utils.clean_title(undefined), "");
    }

    function test_leaves_a_clean_title_alone() {
        compare(title_utils.clean_title("Saṅgīti Sutta"), "Saṅgīti Sutta");
        compare(title_utils.clean_title("Reciting in Concert"), "Reciting in Concert");
    }

    // The defect this exists for: 40 rows in appdata's `suttas` table begin
    // with the literal text `nbsp;`, the remnant of an `&nbsp;` whose
    // ampersand was stripped by an earlier import.
    function test_strips_the_ampersand_less_nbsp_remnant() {
        compare(title_utils.clean_title("nbsp;Saṅgīti Sutta"), "Saṅgīti Sutta");
        compare(title_utils.clean_title("nbsp;Ārakkha Sutta"), "Ārakkha Sutta");
    }

    function test_decodes_well_formed_entities() {
        compare(title_utils.clean_title("&nbsp;Sukha Sutta"), "Sukha Sutta");
        compare(title_utils.clean_title("Questions &amp; Answers"), "Questions & Answers");
        compare(title_utils.clean_title("&#160;Paññā Sutta"), "Paññā Sutta");
    }

    function test_collapses_whitespace_and_trims() {
        compare(title_utils.clean_title("  Sīha   Sutta  "), "Sīha Sutta");
        compare(title_utils.clean_title("Sīha Sutta"), "Sīha Sutta");
        compare(title_utils.clean_title("Sīha\nSutta"), "Sīha Sutta");
    }

    // Applied at more than one layer (tab construction and again at display),
    // so a second pass must not change an already-cleaned value.
    function test_is_idempotent() {
        var once = title_utils.clean_title("nbsp;Saṅgīti Sutta");
        compare(title_utils.clean_title(once), once);
    }
}

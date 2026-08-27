import QtQuick

// Shared display-side cleanup for sutta / document titles.
// Declare an instance where needed, conventionally `TitleUtils { id: title_utils }`.
//
// Some shipped titles carry HTML entity debris from their original source —
// 40 rows in appdata's `suttas` table begin with the literal text `nbsp;`
// (e.g. `nbsp;Saṅgīti Sutta`), the remnant of an `&nbsp;` whose ampersand was
// stripped during an earlier import. A Label renders that verbatim, so it
// shows up in tab labels and in the tab / window lists.
//
// This is a *display* fix only. The stored data is left as it is: repairing it
// belongs in the bootstrap import that produced it, and would need a
// re-bootstrap and a DB version bump.
QtObject {
    id: title_utils

    // Strip entity debris, normalise non-breaking spaces, collapse whitespace
    // runs and trim. Idempotent, so it is safe to apply at more than one layer.
    function clean_title(text) {
        if (!text) {
            return "";
        }
        var s = "" + text;
        // Well-formed entities that survived into the data.
        s = s.replace(/&nbsp;|&#160;|&#xa0;/gi, " ");
        // The ampersand-less remnant a partial strip leaves behind. No real
        // title contains this sequence.
        s = s.replace(/nbsp;/gi, " ");
        s = s.replace(/&amp;/gi, "&");
        // Literal U+00A0.
        s = s.replace(/ /g, " ");
        return s.replace(/\s+/g, " ").trim();
    }
}

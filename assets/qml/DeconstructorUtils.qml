import QtQuick

// Pure filtering / membership helpers for grouped DPD lookup results, shared by
// WordSummary, GlossTab and FulltextResults. See PRD FR-B2.
//
// This is a plain instantiated helper component (used as
// `DeconstructorUtils { id: dec_utils }`, like Logger), NOT a QML singleton —
// `assets/qml/` has no `qmldir` for its own files. The functions are pure (no
// side effects, no external state) so they can be unit-tested in isolation
// (tst_DeconstructorUtils.qml).
//
// `grouped` is the parsed GroupedDpdLookup object (see backend/src/types.rs):
//   { query, results: [{uid, ...}], deconstructions: [{words_joined,
//     components: [{word, result_uids: [uid, ...]}]}], direct_uids: [uid, ...] }
QtObject {
    id: dec_utils

    // The set of result uids to display, as an array.
    //   unlocked -> every result uid (today's full list),
    //   locked   -> direct_uids ∪ the selected break-down's component uids.
    // Order: direct uids first, then the selected break-down's component uids
    // in component order, deduplicated.
    function visible_uids(grouped, selected_index, locked) {
        if (!grouped) return [];

        if (!locked) {
            let all = [];
            let results = grouped.results || [];
            for (let i = 0; i < results.length; i++) {
                all.push(results[i].uid);
            }
            return all;
        }

        let uids = [];
        let seen = ({});
        let push = function(u) {
            if (!seen[u]) { seen[u] = true; uids.push(u); }
        };

        let direct = grouped.direct_uids || [];
        for (let i = 0; i < direct.length; i++) {
            push(direct[i]);
        }

        let decs = grouped.deconstructions || [];
        if (selected_index >= 0 && selected_index < decs.length) {
            let comps = decs[selected_index].components || [];
            for (let c = 0; c < comps.length; c++) {
                let ruids = comps[c].result_uids || [];
                for (let r = 0; r < ruids.length; r++) {
                    push(ruids[r]);
                }
            }
        }

        return uids;
    }

    // The indices of the break-downs whose components include the given result
    // uid (many-to-many membership). Returns an array of break-down indices.
    function breakdowns_of_uid(grouped, uid) {
        let out = [];
        if (!grouped) return out;

        let decs = grouped.deconstructions || [];
        for (let i = 0; i < decs.length; i++) {
            let comps = decs[i].components || [];
            let found = false;
            for (let c = 0; c < comps.length && !found; c++) {
                let ruids = comps[c].result_uids || [];
                if (ruids.indexOf(uid) !== -1) {
                    found = true;
                }
            }
            if (found) out.push(i);
        }
        return out;
    }

    // Convenience membership test: is `uid` in the array returned by
    // visible_uids()? Callers usually build the array once and reuse it, but
    // this keeps the intent readable at call sites.
    function uid_is_visible(visible, uid) {
        return (visible || []).indexOf(uid) !== -1;
    }
}

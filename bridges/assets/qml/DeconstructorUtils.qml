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

    // The component sub-rows to display for a deconstructor-resolved word
    // (GlossTab cases (c)/(d)), as an array of `{word, result_uids}` objects.
    //   locked   -> the selected break-down's components, in break-down order.
    //   unlocked -> the union of ALL break-downs' components, deduplicated by
    //               component word in first-appearance order (case (c)'s sole
    //               break-down trivially yields just its own components).
    // Per-component uid selections are keyed on the component word, so they
    // survive a break-down switch for components present in both.
    function visible_components(grouped, selected_index, locked) {
        if (!grouped) return [];
        let decs = grouped.deconstructions || [];

        if (locked && selected_index >= 0 && selected_index < decs.length) {
            return decs[selected_index].components || [];
        }

        let out = [];
        let seen = ({});
        for (let i = 0; i < decs.length; i++) {
            let comps = decs[i].components || [];
            for (let c = 0; c < comps.length; c++) {
                let w = comps[c].word;
                if (!seen[w]) { seen[w] = true; out.push(comps[c]); }
            }
        }
        return out;
    }

    // Map a component's `result_uids` to the corresponding result objects from
    // the flat `results` list (grouped.results), preserving result_uids order.
    function component_results(grouped, component) {
        let out = [];
        if (!grouped || !component) return out;
        let results = grouped.results || [];
        let by_uid = ({});
        for (let i = 0; i < results.length; i++) {
            by_uid[results[i].uid] = results[i];
        }
        let ruids = component.result_uids || [];
        for (let r = 0; r < ruids.length; r++) {
            let res = by_uid[ruids[r]];
            if (res !== undefined) out.push(res);
        }
        return out;
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

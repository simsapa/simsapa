import QtQuick

// Pure helpers for matching the chapter open in the reader panel to its entry
// in a book's table of contents, so BooksList can reveal and select it.
// Declare an instance where needed, conventionally `TocUtils { id: toc_utils }`.
//
// The matching is by resource path: an EPUB TOC entry's `content` is the
// chapter file path (optionally with a `#anchor`), and the import relies on
// that same fragment-stripped path equalling the spine item's `resource_path`
// (`backend/src/epub_import.rs` looks chapter titles up that way). The decoded
// and basename comparisons in paths_match() are tolerant fallbacks only.
QtObject {
    id: toc_utils

    // A TOC entry's key is its position in the tree, so the same key can be
    // computed while walking the raw TOC (revealing) and while flattening it
    // for display. Position, not label + depth: two entries under different
    // parents can share both of those.
    function toc_item_key(index_path) {
        return "toc_" + index_path.join("_");
    }

    // The keys of every ancestor of the entry at index_path, outermost first.
    // The entry itself is not included: whether its own children are shown
    // stays the user's choice.
    function ancestor_keys(index_path) {
        let keys = [];
        for (let n = 1; n < index_path.length; n++) {
            keys.push(toc_item_key(index_path.slice(0, n)));
        }
        return keys;
    }

    // Split a TOC "content" value into its file path and "#anchor" parts. The
    // anchor keeps its leading '#', which is the form the reader tab stores.
    function split_content_path(content_path) {
        const path = content_path || "";
        const hash_index = path.indexOf('#');
        return {
            file_path: hash_index >= 0 ? path.substring(0, hash_index) : path,
            anchor: hash_index >= 0 ? path.substring(hash_index) : ""
        };
    }

    function paths_match(a, b) {
        if (!a || !b) {
            return false;
        }
        if (a === b) {
            return true;
        }
        let da = a, db = b;
        try {
            da = decodeURIComponent(a);
            db = decodeURIComponent(b);
        } catch (e) {
            // Not valid percent-encoding; compare as given.
        }
        if (da === db) {
            return true;
        }
        const base_a = da.substring(da.lastIndexOf('/') + 1);
        const base_b = db.substring(db.lastIndexOf('/') + 1);
        return base_a.length > 0 && base_a === base_b;
    }

    // Depth-first search for the entry pointing at file_path, returning its
    // index path (e.g. [2, 0, 3]) or null. With require_anchor, only an entry
    // whose own anchor equals `anchor` matches.
    function find_toc_path(toc_items, index_path, file_path, anchor, require_anchor) {
        if (!toc_items) {
            return null;
        }
        for (let i = 0; i < toc_items.length; i++) {
            const item = toc_items[i];
            const child_path = index_path.concat([i]);
            const parts = split_content_path(item.content);

            if (paths_match(parts.file_path, file_path)
                && (!require_anchor || parts.anchor === anchor)) {
                return child_path;
            }

            if (item.children && item.children.length > 0) {
                const found = find_toc_path(item.children, child_path, file_path, anchor, require_anchor);
                if (found !== null) {
                    return found;
                }
            }
        }
        return null;
    }

    // The entry for a chapter opened at `anchor` (which may be ""), or null.
    //
    // The exact-anchor pass runs first for two reasons: with an anchor it
    // picks the precise sub-entry rather than the first entry in the same
    // file, and without one it prefers the entry for the whole file over a
    // sub-entry of it. Only then is the file path matched on its own, so a
    // chapter opened at an anchor the TOC does not list still selects its
    // chapter.
    function resolve_toc_path(toc_items, file_path, anchor) {
        if (!file_path) {
            return null;
        }
        const wanted_anchor = anchor || "";
        let index_path = find_toc_path(toc_items, [], file_path, wanted_anchor, true);
        if (index_path === null) {
            index_path = find_toc_path(toc_items, [], file_path, wanted_anchor, false);
        }
        return index_path;
    }
}

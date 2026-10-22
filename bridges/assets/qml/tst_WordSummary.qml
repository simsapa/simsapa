import QtQuick
import QtTest

Item {
    width: 800; height: 600

    WordSummary {
        id: word_summary
        is_dark: false
        handle_summary_close_fn: function() {}
        handle_open_dict_tab_fn: function(uid) {}
        window_height: 600
        search_as_you_type_checked: true
        anchors.fill: parent
    }

    TestCase {
        name: "TestWordSummary"
        when: windowShown

        function grouped_json(query, words) {
            let results = [];
            for (let i = 0; i < words.length; i++) {
                results.push({ uid: words[i] + "/dpd", title: words[i], snippet: "<b>" + words[i] + "</b>" });
            }
            return JSON.stringify({ query: query, results: results, deconstructions: [], direct_uids: [] });
        }

        // Run a lookup and accept a response with the given words, as the
        // bridge would. The stub bridge answers every lookup with an empty
        // result on a later tick, so wait for that first, then answer again
        // under the same id.
        function look_up_with_results(query, words) {
            verify(word_summary.lookup_from_outside(query));
            tryCompare(word_summary, "is_loading", false);
            word_summary.handle_grouped_ready(word_summary.current_query_id, grouped_json(query, words));
        }

        function test_lookup_starts_while_loading() {
            look_up_with_results("dhammo", ["dhamma 1.01"]);
            word_summary.is_loading = true;
            const old_id = word_summary.current_query_id;

            verify(word_summary.lookup_from_outside("samādhi"));
            verify(word_summary.current_query_id !== old_id);
            // The search-as-you-type timer, restarted by the text change, is
            // stopped, so the tap runs one lookup.
            verify(!findChild(word_summary, "search_timer").running);
            tryCompare(word_summary, "is_loading", false);
        }

        function test_short_word_starts_no_lookup() {
            look_up_with_results("dhammo", ["dhamma 1.01"]);
            const old_id = word_summary.current_query_id;

            verify(!word_summary.lookup_from_outside("kho"));
            compare(word_summary.current_query_id, old_id);
            compare(findChild(word_summary, "summaries_list").count, 1);
        }

        function test_stale_response_ignored() {
            look_up_with_results("dhammo", ["dhamma 1.01", "dhamma 1.02"]);
            compare(findChild(word_summary, "summaries_list").count, 2);

            word_summary.handle_grouped_ready("stale_id", grouped_json("other", []));
            compare(findChild(word_summary, "summaries_list").count, 2);
            compare(word_summary.empty_result_query, "");
        }

        function test_empty_result_line() {
            verify(word_summary.lookup_from_outside("xyzabc"));
            // The stub's empty response is accepted.
            tryCompare(word_summary, "is_loading", false);
            compare(word_summary.empty_result_query, "xyzabc");

            look_up_with_results("dhammo", ["dhamma 1.01"]);
            compare(word_summary.empty_result_query, "");
            compare(findChild(word_summary, "summaries_list").count, 1);
        }
    }
}

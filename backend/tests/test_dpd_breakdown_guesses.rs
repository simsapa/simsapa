//! The "starts with" break-down guesses of `dpd_deconstructor_query()`
//! (attempts 2 and 4): candidate keys from the primary-key index, rows checked
//! one at a time until one has a break-down. See
//! docs/search-snippet-highlight-pipeline.md §9.
//!
//! These tests only read the dist database.

use std::collections::HashMap;

use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{Bool, Text};
use serial_test::serial;
use simsapa_backend::db::dpd::{DECONSTRUCTOR_GUESS_KEY_CAP, DECONSTRUCTOR_GUESS_KEYS_SQL};
use simsapa_backend::get_app_data;

mod helpers;
use helpers as h;

/// Prefixes for the plan and equivalence checks: the largest 3-letter ranges
/// (`sam`, `par`), the one needing the most rows (`saṁ`), and attempt-4
/// prefixes of the test words that hit (`pupphas`, `aggaphalasukha`).
const PREFIXES: [&str; 6] = ["sam", "par", "saṁ", "pupphas", "aggaphalasukha", "kilesapariruddh"];

/// Form C, one statement: the shortest key in the range whose row has a
/// break-down. Reads a table row per key, so the app does not use it; here it
/// is the oracle for the guess helper.
const ORACLE_SQL: &str = "SELECT lookup_key FROM lookup \
     WHERE lookup_key >= ?1 AND lookup_key < ?2 AND deconstructor != '' \
     ORDER BY length(lookup_key), lookup_key LIMIT 1";

#[derive(QueryableByName)]
struct KeyRow {
    #[diesel(sql_type = Text)]
    lookup_key: String,
}

#[derive(QueryableByName)]
struct PlanRow {
    #[diesel(sql_type = Text)]
    detail: String,
}

#[derive(QueryableByName)]
struct KeyHasBreakdownRow {
    #[diesel(sql_type = Text)]
    lookup_key: String,
    #[diesel(sql_type = Bool)]
    has_bd: bool,
}

/// Same as the crate's `prefix_upper_bound()`.
fn upper_bound(prefix: &str) -> String {
    format!("{}{}", prefix, char::MAX)
}

fn sql_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn oracle_key(prefix: &str) -> Option<String> {
    let app_data = get_app_data();
    let conn = &mut app_data.dbm.dpd.get_conn().expect("dpd conn");
    let rows: Vec<KeyRow> = sql_query(ORACLE_SQL)
        .bind::<Text, _>(prefix)
        .bind::<Text, _>(upper_bound(prefix))
        .load(conn)
        .expect("oracle query");
    rows.into_iter().next().map(|r| r.lookup_key)
}

fn count_keys(prefix: &str) -> i64 {
    use simsapa_backend::db::dpd_schema::lookup::dsl::*;
    let app_data = get_app_data();
    let conn = &mut app_data.dbm.dpd.get_conn().expect("dpd conn");
    lookup
        .filter(lookup_key.ge(prefix))
        .filter(lookup_key.lt(upper_bound(prefix)))
        .count()
        .get_result(conn)
        .expect("count")
}

/// The key query reads only the primary-key index. The plan is checked with
/// real values in place of the placeholders: the bundled SQLite has STAT4, so
/// it plans a range query for its bound values, and a plan taken with unbound
/// `?` placeholders is not the plan that runs.
#[test]
#[serial]
fn test_guess_key_query_is_covered_by_the_key_index() {
    h::app_data_setup();
    let app_data = get_app_data();
    let conn = &mut app_data.dbm.dpd.get_conn().expect("dpd conn");

    for prefix in PREFIXES {
        let sql = DECONSTRUCTOR_GUESS_KEYS_SQL
            .replace("?1", &sql_literal(prefix))
            .replace("?2", &sql_literal(&upper_bound(prefix)))
            .replace("?3", &DECONSTRUCTOR_GUESS_KEY_CAP.to_string());
        let plan: Vec<PlanRow> = sql_query(format!("EXPLAIN QUERY PLAN {}", sql))
            .load(conn)
            .expect("explain");
        let details: Vec<String> = plan.into_iter().map(|r| r.detail).collect();
        let lookup_steps: Vec<&String> = details.iter().filter(|d| d.contains("lookup")).collect();
        assert_eq!(lookup_steps.len(), 1, "{}: {:?}", prefix, details);
        assert!(
            lookup_steps[0].starts_with("SEARCH lookup USING COVERING INDEX sqlite_autoindex_lookup_1"),
            "{}: the key query must read only the key index, got {:?}",
            prefix,
            details,
        );
    }
}

/// The guess is the shortest key in the range whose row has a break-down: the
/// same key form C's single statement returns.
#[test]
#[serial]
fn test_guess_equals_shortest_key_with_breakdown() {
    h::app_data_setup();
    let app_data = get_app_data();

    for prefix in PREFIXES {
        let (row, checked) = app_data.dbm.dpd.dpd_deconstructor_prefix_guess(prefix).expect("guess");
        let key = row.as_ref().map(|r| r.lookup_key.clone());
        assert_eq!(key, oracle_key(prefix), "{}", prefix);
        assert!((checked as i32) <= DECONSTRUCTOR_GUESS_KEY_CAP, "{}: {}", prefix, checked);
        if let Some(row) = row {
            assert!(row.lookup_key.starts_with(prefix), "{}: {}", prefix, row.lookup_key);
            assert!(!row.deconstructor.is_empty(), "{}: {} has no break-down", prefix, row.lookup_key);
        }
    }
}

/// A range with more keys than the cap and no break-down among them gives no
/// guess, after checking exactly the cap. `sho` is an English-entry range.
#[test]
#[serial]
fn test_guess_stops_at_the_cap() {
    h::app_data_setup();
    let app_data = get_app_data();

    assert!(
        count_keys("sho") > DECONSTRUCTOR_GUESS_KEY_CAP as i64,
        "the test needs a range larger than the cap",
    );
    assert_eq!(oracle_key("sho"), None, "the test needs a range with no break-down");
    let (row, checked) = app_data.dbm.dpd.dpd_deconstructor_prefix_guess("sho").expect("guess");
    assert!(row.is_none());
    assert_eq!(checked as i32, DECONSTRUCTOR_GUESS_KEY_CAP);
}

/// `pupphasu` is not in the DPD; attempt 2 guesses a key starting with it.
/// With `exact_only` there is no guess.
#[test]
#[serial]
fn test_exact_only_never_guesses() {
    h::app_data_setup();
    let app_data = get_app_data();

    let guessed = app_data.dbm.dpd.dpd_deconstructor_query("pupphasu", false).expect("query");
    assert!(guessed.is_some(), "pupphasu should get a guess with exact_only = false");
    let exact = app_data.dbm.dpd.dpd_deconstructor_query("pupphasu", true).expect("query");
    assert!(exact.is_none(), "exact_only must not guess");
}

/// Cap headroom: for every 3- and 4-letter prefix of Pāli letters that has a
/// key with a break-down, the rows checked before the first one stay within
/// the cap. Attempt 4 gets such prefixes from 4- and 5-letter words. Run it
/// after a DPD data update:
///
/// `cargo test --test test_dpd_breakdown_guesses -- --ignored --nocapture`
#[test]
#[ignore]
#[serial]
fn test_guess_cap_headroom() {
    h::app_data_setup();
    let app_data = get_app_data();
    let conn = &mut app_data.dbm.dpd.get_conn().expect("dpd conn");

    let pali: Vec<char> = "aāiīuūeokgṅcjñṭḍṇtdnpbmyrlvshḷṁ".chars().collect();
    let rows: Vec<KeyHasBreakdownRow> =
        sql_query("SELECT lookup_key, deconstructor != '' AS has_bd FROM lookup")
            .load(conn)
            .expect("lookup keys");

    for n in [3usize, 4] {
        // prefix -> (char length, key, has break-down)
        let mut groups: HashMap<String, Vec<(usize, &str, bool)>> = HashMap::new();
        for r in &rows {
            let len = r.lookup_key.chars().count();
            if len < n {
                continue;
            }
            let prefix: String = r.lookup_key.chars().take(n).collect();
            if !prefix.chars().all(|c| pali.contains(&c)) {
                continue;
            }
            groups.entry(prefix).or_default().push((len, &r.lookup_key, r.has_bd));
        }

        // (rows checked before the first key with a break-down, prefix)
        let mut needed: Vec<(usize, String)> = groups
            .into_iter()
            .filter_map(|(prefix, mut keys)| {
                // The key query's order: length() in characters, then the key
                // (BINARY, which is UTF-8 byte order, as `str`'s `Ord`).
                keys.sort();
                keys.iter().position(|k| k.2).map(|i| (i + 1, prefix))
            })
            .collect();
        needed.sort_by(|a, b| b.cmp(a));

        println!("{}-letter prefixes with a break-down: {}; most rows needed: {:?}",
                 n, needed.len(), &needed[..needed.len().min(8)]);
        let over: Vec<&(usize, String)> =
            needed.iter().filter(|x| x.0 as i32 > DECONSTRUCTOR_GUESS_KEY_CAP).collect();
        assert!(over.is_empty(), "prefixes needing more than the cap: {:?}", over);
    }
}

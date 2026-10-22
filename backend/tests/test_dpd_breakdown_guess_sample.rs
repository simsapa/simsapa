//! Usefulness sample for the deconstructor's "starts with" guesses (attempts 2
//! and 4 of `dpd_deconstructor_query()`).
//!
//! Draws a reproducible sample of word tokens from the Pāli `content_plain` of
//! the `ms` and CST editions, keeps the ones whose exact deconstructor lookup
//! misses (the words that reach the guesses), and records what each guess form
//! returns. The output is a TSV for a human to mark as useful or not; nothing
//! here judges usefulness.
//!
//! Run by hand:
//!
//! ``` sh
//! DPD_GUESS_SAMPLE_OUT=/path/to/sample.tsv [DPD_GUESS_SAMPLE_SIZE=1000] \
//!   cargo test --test test_dpd_breakdown_guess_sample -- --ignored --nocapture
//! ```

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;

use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{BigInt, Text};
use serial_test::serial;

use simsapa_backend::get_app_data;
use simsapa_backend::helpers::{normalize_dpd_query_split, normalize_dpd_query_text};
use simsapa_backend::pali_stemmer::pali_stem;

mod helpers;
use helpers as h;

const DEFAULT_SAMPLE_SIZE: usize = 1000;
const MAX_DRAWS: usize = 500_000;
const SEED: u64 = 20261022;
const CONTEXT_WORDS: usize = 5;

#[derive(QueryableByName)]
struct SuttaLen {
    #[diesel(sql_type = Text)]
    uid: String,
    #[diesel(sql_type = BigInt)]
    len: i64,
}

#[derive(QueryableByName)]
struct SuttaContent {
    #[diesel(sql_type = Text)]
    content_plain: String,
}

#[derive(QueryableByName)]
struct KeyRow {
    #[diesel(sql_type = Text)]
    lookup_key: String,
}

#[derive(QueryableByName)]
struct LookupRow {
    #[diesel(sql_type = Text)]
    lookup_key: String,
    #[diesel(sql_type = Text)]
    deconstructor: String,
}

#[derive(QueryableByName)]
struct UidRow {
    #[diesel(sql_type = Text)]
    uid: String,
}

/// A guess: the matched key and its break-downs (empty when the row has none).
struct Guess {
    key: String,
    breakdowns: Vec<String>,
}

/// Attempt 2 and attempt 4 of one guess form. Attempt 4 runs only when the
/// form's attempt 2 found nothing, as in `dpd_deconstructor_query()`.
struct FormResult {
    a2: Option<Guess>,
    a4: Option<Guess>,
    a4_ran: bool,
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

fn upper_bound(prefix: &str) -> String {
    format!("{}{}", prefix, char::MAX)
}

fn parse_breakdowns(deconstructor: &str) -> Vec<String> {
    if deconstructor.is_empty() {
        return Vec::new();
    }
    serde_json::from_str::<Vec<String>>(deconstructor).unwrap_or_else(|_| vec![deconstructor.to_string()])
}

fn load_row(conn: &mut SqliteConnection, key: &str) -> Guess {
    let row: LookupRow = sql_query("SELECT lookup_key, deconstructor FROM lookup WHERE lookup_key = ?1")
        .bind::<Text, _>(key)
        .get_result(conn)
        .expect("lookup row by key");
    Guess { key: row.lookup_key, breakdowns: parse_breakdowns(&row.deconstructor) }
}

/// Today's form: `LIKE 'prefix%'`, first row in table order.
fn guess_like(conn: &mut SqliteConnection, prefix: &str) -> Option<Guess> {
    let row: Option<LookupRow> = sql_query("SELECT lookup_key, deconstructor FROM lookup WHERE lookup_key LIKE ?1 LIMIT 1")
        .bind::<Text, _>(format!("{}%", prefix))
        .get_result(conn)
        .optional()
        .expect("like guess");
    row.map(|r| Guess { key: r.lookup_key, breakdowns: parse_breakdowns(&r.deconstructor) })
}

/// Index range, shortest key, then key order; optionally only rows with a
/// break-down.
fn guess_range(conn: &mut SqliteConnection, prefix: &str, with_breakdown_only: bool) -> Option<Guess> {
    let sql = if with_breakdown_only {
        "SELECT lookup_key FROM lookup WHERE lookup_key >= ?1 AND lookup_key < ?2 AND deconstructor != '' \
         ORDER BY length(lookup_key), lookup_key LIMIT 1"
    } else {
        "SELECT lookup_key FROM lookup WHERE lookup_key >= ?1 AND lookup_key < ?2 \
         ORDER BY length(lookup_key), lookup_key LIMIT 1"
    };
    let key: Option<KeyRow> = sql_query(sql)
        .bind::<Text, _>(prefix)
        .bind::<Text, _>(upper_bound(prefix))
        .get_result(conn)
        .optional()
        .expect("range guess");
    key.map(|k| load_row(conn, &k.lookup_key))
}

fn run_form<F>(word: &str, mut guess: F) -> FormResult
where
    F: FnMut(&str) -> Option<Guess>,
{
    let a2 = guess(word);
    if a2.is_some() {
        return FormResult { a2, a4: None, a4_ran: false };
    }
    let n = word.chars().count();
    let all_but_last: String = word.chars().take(n - 1).collect();
    FormResult { a2: None, a4: guess(&all_but_last), a4_ran: true }
}

/// The uids Phase 8 of the grouped lookup returns for `word` when it runs.
fn phase8_uids(conn: &mut SqliteConnection, word: &str) -> HashSet<String> {
    let rows: Vec<UidRow> = sql_query(
        "SELECT uid FROM dpd_headwords WHERE (lemma_clean >= ?1 AND lemma_clean < ?2) OR (word_ascii >= ?1 AND word_ascii < ?2)",
    )
    .bind::<Text, _>(word)
    .bind::<Text, _>(upper_bound(word))
    .load(conn)
    .expect("phase 8 headwords");
    if !rows.is_empty() {
        return rows.into_iter().map(|r| r.uid).collect();
    }
    let stem = pali_stem(word, false);
    let rows: Vec<UidRow> = sql_query("SELECT uid FROM dpd_headwords WHERE stem >= ?1 AND stem < ?2")
        .bind::<Text, _>(&stem)
        .bind::<Text, _>(upper_bound(&stem))
        .load(conn)
        .expect("phase 8 stem");
    rows.into_iter().map(|r| r.uid).collect()
}

fn yn(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

/// Columns for one attempt: key, has_bd, break-downs. `-` when the attempt did
/// not run, empty key when it ran and found nothing.
fn guess_cols(g: &Option<Guess>, ran: bool, with_has_bd: bool) -> Vec<String> {
    let mut cols = Vec::new();
    match (ran, g) {
        (false, _) => {
            cols.push("-".to_string());
            if with_has_bd {
                cols.push("-".to_string());
            }
            cols.push("-".to_string());
        }
        (true, None) => {
            cols.push(String::new());
            if with_has_bd {
                cols.push(String::new());
            }
            cols.push(String::new());
        }
        (true, Some(g)) => {
            cols.push(g.key.clone());
            if with_has_bd {
                cols.push(yn(!g.breakdowns.is_empty()).to_string());
            }
            cols.push(g.breakdowns.join(" | "));
        }
    }
    cols
}

#[derive(Default)]
struct FormCounts {
    a2_rows: usize,
    a2_empty_bd: usize,
    a4_rows: usize,
    a4_empty_bd: usize,
    any_bd: usize,
}

impl FormCounts {
    fn add(&mut self, r: &FormResult) {
        if let Some(g) = &r.a2 {
            self.a2_rows += 1;
            if g.breakdowns.is_empty() {
                self.a2_empty_bd += 1;
            }
        }
        if let Some(g) = &r.a4 {
            self.a4_rows += 1;
            if g.breakdowns.is_empty() {
                self.a4_empty_bd += 1;
            }
        }
        let contributes = r.a2.as_ref().or(r.a4.as_ref()).is_some_and(|g| !g.breakdowns.is_empty());
        if contributes {
            self.any_bd += 1;
        }
    }
}

enum Skip {
    Short,
    Iti,
    Other,
}

/// `ti` / `nti`, alone or with a joined `ādi…` / `pi`, as the bootstrap's
/// iti-sandhi split leaves it in `content_plain`. Not `tipi…`: `tipiṭaka`.
fn is_iti_tail(token: &str) -> bool {
    let rest = token.strip_prefix("nti").or_else(|| token.strip_prefix("ti"));
    rest.is_some_and(|r| r.is_empty() || r == "pi" || r.starts_with("ādi") || r.starts_with("ādī"))
}

/// The sample token at `char_offset` of `text`, with a few words of context:
/// the token covering the offset, or the next one when it falls on whitespace.
fn resolve_draw(text: &str, char_offset: usize) -> Result<(String, String), Skip> {
    let byte_offset = text.char_indices().nth(char_offset).map(|(b, _)| b).unwrap_or(text.len());
    let tokens: Vec<(usize, &str)> = text
        .split_whitespace()
        .map(|t| (t.as_ptr() as usize - text.as_ptr() as usize, t))
        .collect();
    let ti = tokens.iter().position(|(start, t)| byte_offset < start + t.len()).ok_or(Skip::Other)?;

    let normalized = normalize_dpd_query_split(tokens[ti].1);
    let words: Vec<&str> = normalized.split_whitespace().collect();
    if words.len() != 1 || words[0].chars().any(|c| c.is_ascii_digit()) {
        return Err(Skip::Other);
    }
    let word = words[0].to_string();
    if word.chars().count() < 4 {
        return Err(Skip::Short);
    }
    // Iti sandhi never guesses: either the token carries it, or the bootstrap
    // normalization split it off into a following `ti` / `nti`, possibly with
    // a joined `ādi` / `pi` (`parijānātī tiādinā`, `santo ntipi`). Such a
    // tail is not a word of its own either: in the page it is part of the
    // word the user taps.
    let next_is_ti = tokens.get(ti + 1).is_some_and(|(_, t)| is_iti_tail(t));
    if next_is_ti || is_iti_tail(&word) || normalize_dpd_query_text(&word).1.is_some() {
        return Err(Skip::Iti);
    }

    let from = ti.saturating_sub(CONTEXT_WORDS);
    let to = (ti + CONTEXT_WORDS + 1).min(tokens.len());
    let context = tokens[from..to].iter().map(|(_, t)| *t).collect::<Vec<_>>().join(" ");
    Ok((word, context))
}

#[test]
#[ignore]
#[serial]
fn dpd_breakdown_guess_sample() {
    let out_path = std::env::var("DPD_GUESS_SAMPLE_OUT")
        .expect("set DPD_GUESS_SAMPLE_OUT to the TSV output path");

    h::app_data_setup();
    let app_data = get_app_data();
    let appdata_conn = &mut app_data.dbm.appdata.get_conn().expect("appdata conn");
    let dpd_conn = &mut app_data.dbm.dpd.get_conn().expect("dpd conn");

    let suttas: Vec<SuttaLen> = sql_query(
        "SELECT uid, COALESCE(length(content_plain), 0) AS len FROM suttas \
         WHERE language = 'pli' AND (uid LIKE '%/pli/ms' OR uid LIKE '%/pli/cst') \
         ORDER BY uid",
    )
    .load(appdata_conn)
    .expect("sutta lengths");

    let mut cumulative: Vec<i64> = Vec::with_capacity(suttas.len());
    let mut total: i64 = 0;
    for s in &suttas {
        total += s.len;
        cumulative.push(total);
    }
    assert!(total > 0, "no Pāli content_plain found");

    let sample_size: usize = std::env::var("DPD_GUESS_SAMPLE_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SAMPLE_SIZE);

    let mut rng = Lcg(SEED);
    let mut draws = 0;
    let mut sampled: Vec<(String, String, String)> = Vec::new(); // (word, sutta uid, context)
    let mut dropped_short = 0;
    let mut dropped_iti = 0;
    let mut dropped_other = 0;

    // Draws are taken in LCG order, resolved in chunks grouped by sutta so
    // that each text is loaded once per chunk.
    while sampled.len() < sample_size && draws < MAX_DRAWS {
        let chunk = ((sample_size - sampled.len()) * 5 / 4 + 100).min(MAX_DRAWS - draws);
        let offsets: Vec<(usize, usize)> = (0..chunk)
            .map(|_| {
                let offset = (rng.next() % total as u64) as i64;
                let idx = cumulative.partition_point(|&c| c <= offset);
                (idx, (offset - (cumulative[idx] - suttas[idx].len)) as usize)
            })
            .collect();
        let mut order: Vec<usize> = (0..chunk).collect();
        order.sort_by_key(|&i| offsets[i]);

        let mut resolved: Vec<Option<Result<(String, String), Skip>>> = (0..chunk).map(|_| None).collect();
        let mut loaded: Option<(usize, String)> = None;
        for i in order {
            let (idx, char_offset) = offsets[i];
            if loaded.as_ref().is_none_or(|(j, _)| *j != idx) {
                let content: SuttaContent = sql_query("SELECT content_plain FROM suttas WHERE uid = ?1")
                    .bind::<Text, _>(&suttas[idx].uid)
                    .get_result(appdata_conn)
                    .expect("sutta content");
                loaded = Some((idx, content.content_plain));
            }
            let text = &loaded.as_ref().unwrap().1;
            resolved[i] = Some(resolve_draw(text, char_offset));
        }

        for (i, r) in resolved.into_iter().enumerate() {
            if sampled.len() >= sample_size {
                break;
            }
            draws += 1;
            match r.unwrap() {
                Ok((word, context)) => sampled.push((word, suttas[offsets[i].0].uid.clone(), context)),
                Err(Skip::Short) => dropped_short += 1,
                Err(Skip::Iti) => dropped_iti += 1,
                Err(Skip::Other) => dropped_other += 1,
            }
        }
    }

    let mut tsv = String::new();
    let mut header = vec!["query".to_string(), "other_results".to_string(), "p8_results".to_string()];
    for a in ["a2", "a4"] {
        for (form, has_bd) in [("like", true), ("range", true), ("range_bd", false)] {
            header.push(format!("{}_{}", a, form));
            if has_bd {
                header.push(format!("{}_{}_has_bd", a, form));
            }
            header.push(format!("{}_{}_breakdown", a, form));
        }
    }
    header.extend(["sutta_uid", "context", "useful"].map(String::from));
    writeln!(tsv, "{}", header.join("\t")).unwrap();

    let mut reaching = 0;
    let mut reaching_no_other = 0;
    let mut counts_like = FormCounts::default();
    let mut counts_range = FormCounts::default();
    let mut counts_range_bd = FormCounts::default();
    let mut range_bd_differs = 0; // range_bd picked a different key than plain range

    for (word, sutta_uid, context) in &sampled {
        // Attempt 1 (attempt 3 needs a space and never applies to one token).
        let exact: Option<KeyRow> = sql_query("SELECT lookup_key FROM lookup WHERE lookup_key = ?1")
            .bind::<Text, _>(word)
            .get_result(dpd_conn)
            .optional()
            .expect("exact lookup");
        if exact.is_some() {
            continue;
        }
        reaching += 1;

        // Phases 1–6 without guesses; Phase 7 adds nothing because attempt 1
        // missed, and Phase 8 runs only when 1–6 found nothing.
        let grouped = app_data.dbm.dpd
            .dpd_lookup_grouped(word, false, true, true, None, None)
            .expect("grouped lookup");
        let result_uids: HashSet<String> = grouped.results.iter().map(|r| r.uid.clone()).collect();
        let p8 = phase8_uids(dpd_conn, word);
        let other_results = !result_uids.is_empty() && (p8.is_empty() || result_uids != p8);
        let p8_results = if other_results { 0 } else { result_uids.len() };
        if !other_results {
            reaching_no_other += 1;
        }

        let like = run_form(word, |p| guess_like(dpd_conn, p));
        let range = run_form(word, |p| guess_range(dpd_conn, p, false));
        let range_bd = run_form(word, |p| guess_range(dpd_conn, p, true));
        counts_like.add(&like);
        counts_range.add(&range);
        counts_range_bd.add(&range_bd);
        let range_key = range.a2.as_ref().or(range.a4.as_ref()).map(|g| g.key.as_str());
        let range_bd_key = range_bd.a2.as_ref().or(range_bd.a4.as_ref()).map(|g| g.key.as_str());
        if range_key != range_bd_key {
            range_bd_differs += 1;
        }

        let mut cols = vec![word.clone(), yn(other_results).to_string(), p8_results.to_string()];
        cols.extend(guess_cols(&like.a2, true, true));
        cols.extend(guess_cols(&range.a2, true, true));
        cols.extend(guess_cols(&range_bd.a2, true, false));
        cols.extend(guess_cols(&like.a4, like.a4_ran, true));
        cols.extend(guess_cols(&range.a4, range.a4_ran, true));
        cols.extend(guess_cols(&range_bd.a4, range_bd.a4_ran, false));
        cols.push(sutta_uid.clone());
        cols.push(context.replace('\t', " "));
        cols.push(String::new());
        writeln!(tsv, "{}", cols.join("\t")).unwrap();
    }

    fs::write(&out_path, tsv).expect("write TSV");

    println!("DPD-GUESS-SAMPLE: seed={} draws={} sampled={}", SEED, draws, sampled.len());
    println!("  dropped: short={} iti={} other={}", dropped_short, dropped_iti, dropped_other);
    println!("  reaching the guesses: {} (with no other results: {})", reaching, reaching_no_other);
    for (name, c) in [("like", &counts_like), ("range", &counts_range), ("range_bd", &counts_range_bd)] {
        println!(
            "  {:<8} a2 rows={} (empty bd {})  a4 rows={} (empty bd {})  tokens with a break-down={}",
            name, c.a2_rows, c.a2_empty_bd, c.a4_rows, c.a4_empty_bd, c.any_bd,
        );
    }
    println!("  range_bd picked a different key than range: {}", range_bd_differs);
    println!("  TSV: {}", out_path);
}

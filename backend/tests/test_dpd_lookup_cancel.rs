use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::lookup_cancel::{LookupCancel, is_lookup_cancelled};

mod helpers;
use helpers as h;

/// A token whose counter has moved on since it was taken.
fn cancelled_token() -> LookupCancel {
    let counter = Arc::new(AtomicU64::new(0));
    let token = LookupCancel::new(&counter);
    counter.fetch_add(1, Ordering::Relaxed);
    token
}

/// A cancelled lookup returns the `LookupCancelled` error, not partial
/// results. `pañcaggadāyakaṁ` is not in the DPD, so uncancelled it would run
/// every phase including the deconstructor guesses.
#[test]
#[serial]
fn test_cancelled_grouped_lookup_returns_cancelled_error() {
    h::app_data_setup();
    let app_data = get_app_data();

    let res = app_data.dbm.dpd.dpd_lookup_grouped_cancellable(
        "pañcaggadāyakaṁ", false, true, false, None, None, &cancelled_token(),
    );
    let err = res.expect_err("a cancelled lookup must return an error");
    assert!(is_lookup_cancelled(&err), "expected LookupCancelled, got: {}", err);
}

/// A cancelled lookup leaves the memo as it was. The memo is a process global,
/// so this file holds only one memo test: each test file is its own process.
#[test]
#[serial]
fn test_cancelled_lookup_is_not_memoized() {
    h::app_data_setup();
    let app_data = get_app_data();
    let dpd = &app_data.dbm.dpd;

    let first = dpd
        .dpd_lookup_grouped_memo("dhamma", false, true, false, None, None)
        .unwrap();

    let res = dpd.dpd_lookup_grouped_memo_cancellable(
        "pañcaggadāyakaṁ", false, true, false, None, None, &cancelled_token(),
    );
    assert!(res.is_err_and(|e| is_lookup_cancelled(&e)));

    let third = dpd
        .dpd_lookup_grouped_memo("dhamma", false, true, false, None, None)
        .unwrap();
    assert!(
        Arc::ptr_eq(&first, &third),
        "the cancelled lookup replaced the memo entry"
    );
}

/// An uncancelled token gives the same lookup as the non-cancellable wrapper.
#[test]
#[serial]
fn test_none_token_matches_wrapper() {
    h::app_data_setup();
    let app_data = get_app_data();
    let dpd = &app_data.dbm.dpd;

    for query in ["pañcaggadāyakaṁ", "sādhūti", "dhamma"] {
        let wrapped = dpd
            .dpd_lookup_grouped(query, false, true, false, None, None)
            .unwrap();
        let cancellable = dpd
            .dpd_lookup_grouped_cancellable(query, false, true, false, None, None, &LookupCancel::none())
            .unwrap();
        assert_eq!(
            serde_json::to_string(&wrapped).unwrap(),
            serde_json::to_string(&cancellable).unwrap(),
            "results differ for {}",
            query
        );
    }
}

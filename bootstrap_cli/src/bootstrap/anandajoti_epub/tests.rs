//! Tests of the shared epub, page and import code.

#[test]
fn a_page_marker_span_keeps_the_text_around_the_marker_outside_it() {
    use super::page::split_page_marker;
    assert_eq!(split_page_marker("{1.3}"), ("", "{1.3}", ""));
    assert_eq!(split_page_marker(" {1.3} [28.146]"), (" ", "{1.3} [28.146]", ""));
    assert_eq!(split_page_marker("{1.423} "), ("", "{1.423}", " "));
    // The source's grey span also covers a dialogue dash.
    assert_eq!(split_page_marker("[28.167] –"), ("", "[28.167]", " –"));
    assert_eq!(split_page_marker("{M. ii. 97}"), ("", "{M. ii. 97}", ""));
    // A marker split over two spans.
    assert_eq!(split_page_marker("{3."), ("", "{3.", ""));
    assert_eq!(split_page_marker("465}"), ("", "465}", ""));
    assert_eq!(split_page_marker(" "), (" ", "", ""));
}


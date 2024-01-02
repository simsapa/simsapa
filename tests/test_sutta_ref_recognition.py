"""Test Sutta Reference Recognition
"""

from simsapa.app.helpers import is_book_sutta_ref, is_pts_sutta_ref, query_text_to_uid_field_query

# sutta_range_from_ref
# normalize_sutta_ref
# normalize_sutta_uid
#
# dhp_verse_to_chapter
# dhp_chapter_ref_for_verse_num
# thag_verse_to_uid
# thig_verse_to_uid
# snp_verse_to_uid

BOOK_REF_TEST_CASES = [
    # test input, expected uid
    ["MN 1", "mn1"],
    ["MN1", "mn1"],
    ["MN44", "mn44"],
    ["MN 118", "mn118"],
    ["AN 4.10", "an4.10"],
    ["Sn 4:2", "sn4.2"],
    ["Dhp 182", "dhp179-196"],
    ["Thag 1207", "thag20.1"],
    ["Vism 152", "FIXME"],
]

PTS_REF_TEST_CASES = [
    # test input, expected uid
    ["Vin.ii.40", "FIXME"],
    ["AN.i.78", "FIXME"],
    ["D iii 264", "FIXME"],
    ["SN i 190", "FIXME"],
    ["M. III. 203.", "FIXME"],
]

def test_is_book_sutta_ref():
    for case, _ in BOOK_REF_TEST_CASES:
        is_ref = is_book_sutta_ref(case)
        print(f"{case}: {is_ref}")
        assert is_ref is True

def test_is_pts_sutta_ref():
    for case, _ in PTS_REF_TEST_CASES:
        is_ref = is_pts_sutta_ref(case)
        print(f"{case}: {is_ref}")
        assert is_ref is True

def test_query_text_to_uid():
    query_text = "SN 44.22"
    uid = query_text_to_uid_field_query(query_text)
    assert uid == "uid:sn44.22"

def test_not_matching_url_path_sep():
    # Regex must not match part of the path sep (/) in a url, only mn44
    # <a class="link" href="ssp://suttas/mn44/en/sujato">
    text = "/mn44/en/sujato"
    is_ref = (is_book_sutta_ref(text) or is_pts_sutta_ref(text))
    assert is_ref is False

def test_does_match_complete_uid():
    # But is should match without the leading "/"
    text = "mn44/en/sujato"
    is_ref = (is_book_sutta_ref(text) or is_pts_sutta_ref(text))
    assert is_ref is True

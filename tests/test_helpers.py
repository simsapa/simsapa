"""Test Helpers
"""

import simsapa.app.helpers as h

def test_pali_to_ascii():
    assert (h.pali_to_ascii("dhammāya") == "dhammaya")
    assert (h.pali_to_ascii("saṁsāra") == "samsara")
    assert (h.pali_to_ascii("Ñāṇa") == "Nana")
    assert (h.pali_to_ascii("  √muc  ") == "muc")
    assert (h.pali_to_ascii(None) == "")

def test_word_uid_sanitize():
    assert(h.word_uid_sanitize("word.with,punct;") == "word-with-punct-")
    assert(h.word_uid_sanitize("word (bracket)") == "word-bracket-")
    assert(h.word_uid_sanitize("word's quote\"") == "words-quote")
    assert(h.word_uid_sanitize("word--with---dashes") == "word-with-dashes")
    assert(h.word_uid_sanitize("  leading space  ") == "-leading-space-")

def test_word_uid():
    assert(h.word_uid("kammavācā", "PTS") == "kammavācā/pts")
    assert(h.word_uid("paṭisallāna", "dpd") == "paṭisallāna/dpd")

def test_remove_punct():
    assert(h.remove_punct("Hello, world! How are you? …") == "Hello world How are you ")
    assert(h.remove_punct("Line1.\nLine2;") == "Line1 Line2 ")
    assert(h.remove_punct("nibbāpethā'ti") == "nibbāpethā ti")
    assert(h.remove_punct("  Multiple   spaces.  ") == " Multiple spaces ")
    assert(h.remove_punct(None) == "")

def test_compact_plain_text():
    assert(h.compact_plain_text("  HELLO, World! ṃ {test}  ") == "hello world ṁ test")
    assert(h.compact_plain_text("Saṃsāra.") == "saṁsāra")

def test_strip_html():
    assert(h.strip_html("<p>Hello <b>world</b></p>") == "Hello world")
    assert(h.strip_html("Text with &amp; entity.") == "Text with & entity.")
    assert(h.strip_html("<head><title>T</title></head><body>Text</body>") == "Text")
    assert(h.strip_html("👍 Text 👎") == "Text")

def test_compact_rich_text():
    assert(h.compact_rich_text("<p>Hello, <b>W</b>orld! ṃ</p>\n<a class=\"ref\">ref</a>") == "hello world ṁ")
    assert(h.compact_rich_text("dhamm<b>āya</b>") == "dhammāya")
    assert(h.compact_rich_text("<i>italic</i> test") == "italic test")
    assert(h.compact_rich_text("<td>dhammassa</td><td>dhammāya</td>") == "dhammassa dhammāya")

def test_root_info_clean_plaintext():
    html = "<div>Pāḷi Root: √gam ･ Bases: gacchati etc.</div>"
    assert(h.root_info_clean_plaintext(html) == "√gam")

def test_latinize():
    assert(h.latinize("dhammāya") == "dhammaya")
    assert(h.latinize("saṁsāra") == "samsara")
    assert(h.latinize("Ñāṇa") == "nana")

def test_consistent_niggahita():
    assert(h.consistent_niggahita("saṃsāra") == "saṁsāra")
    assert(h.consistent_niggahita("dhammaṁ") == "dhammaṁ")

use simsapa_backend::snowball::{Algorithm, Stemmer, lang_to_algorithm};
use simsapa_backend::helpers::pali_to_ascii;

// NOTE: These are very basic tests to verify minimal stemmer funcionality.
// A more extensive stemmer test suite is found in the pali-stemmer-in-snowball/ project.

// The Pali stemmer operates on ASCII-folded input, mirroring the tokenizer pipeline
// which runs AsciiFoldingFilter before PaliStemmerFilter.
fn pali_stems_to(input: &str, expected: &str) {
    let folded = pali_to_ascii(Some(input));
    let stemmer = Stemmer::create(Algorithm::Pali);
    let result = stemmer.stem(&folded);
    assert_eq!(result, expected, "stemming '{}' (folded: '{}') with Pali: got '{}', expected '{}'", input, folded, result, expected);
}

fn stems_to(input: &str, expected: &str, algo: Algorithm) {
    let stemmer = Stemmer::create(algo);
    let result = stemmer.stem(input);
    assert_eq!(result, expected, "stemming '{}' with {:?}: got '{}', expected '{}'", input, algo, result, expected);
}

#[test]
fn test_pali_a_stem_basic() {
    pali_stems_to("dhammo", "dhamma");
    pali_stems_to("dhammassa", "dhamma");
}

#[test]
fn test_pali_u_stem() {
    pali_stems_to("bhikkhūnaṁ", "bhikkhu");
}

#[test]
fn test_pali_exception_list() {
    pali_stems_to("nibbānaṁ", "nibbana");
}

#[test]
fn test_pali_consonantal_stem() {
    // accept 'bhagavanta' token instead of 'bhagavant'
    pali_stems_to("bhagavantaṁ", "bhagavanta");
}

#[test]
fn test_pali_verb_forms() {
    // a-conjugation verb
    pali_stems_to("vadeyya", "vadati");
}

#[test]
fn test_english_stemmer() {
    stems_to("suffering", "suffer", Algorithm::English);
    stems_to("running", "run", Algorithm::English);
}

#[test]
fn test_hungarian_stemmer() {
    stems_to("kutyák", "kutya", Algorithm::Hungarian);
    stems_to("látják", "látja", Algorithm::Hungarian);
}

#[test]
fn test_lang_to_algorithm_mappings() {
    assert_eq!(lang_to_algorithm("pli"), Algorithm::Pali);
    assert_eq!(lang_to_algorithm("san"), Algorithm::Pali);
    assert_eq!(lang_to_algorithm("en"), Algorithm::English);
    assert_eq!(lang_to_algorithm("de"), Algorithm::German);
}

#[test]
fn test_lang_to_algorithm_fallback() {
    // Unknown language codes should fall back to English
    assert_eq!(lang_to_algorithm("xx"), Algorithm::English);
    assert_eq!(lang_to_algorithm("zz"), Algorithm::English);
    assert_eq!(lang_to_algorithm(""), Algorithm::English);
}

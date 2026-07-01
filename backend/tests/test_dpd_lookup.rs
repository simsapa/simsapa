use std::fs;
use std::path::PathBuf;
use std::collections::{HashMap, BTreeMap};

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::helpers::{extract_words, normalize_query_text};
use simsapa_backend::db::dpd::LookupResult;

mod helpers;
use helpers as h;

#[test]
#[serial]
fn test_dpd_lookup_list() {
    h::app_data_setup();
    let app_data = get_app_data();

    let query = "olokitasaññāṇeneva";
    let result = app_data.dbm.dpd.dpd_lookup_list(query);

    // println!("{}", result.join("\n"));

    let expected: Vec<String> = r#"
<b>eva 1</b> <i>(ind)</i> only; just; merely; exclusively   <i>ind, emph</i>
<b>eva 2</b> <i>(ind)</i> still   <i>ind</i>
<b>eva 3</b> <i>(ind)</i> even; too; as well   <i>ind, adv</i>
<b>eva 4</b> <i>(ind)</i> indeed; really; certainly; absolutely  <b>[eva]</b>  <i>ind, emph</i>
<b>eva 5</b> <i>(ind)</i> as soon as   <i>ind, emph</i>
<b>iva</b> <i>(ind)</i> like; as   <i>ind</i>
<b>olokita</b> <i>(pp)</i> looked at; observed; viewed (by)  <b>[ava + √lok + ita]</b>  <i>pp of oloketi</i>
<b>saññāṇa 1</b> <i>(nt)</i> marking; signing  <b>[saṁ + √ñā + aṇa]</b>  <i>nt, act, from sañjānāti</i>
<b>saññāṇa 2</b> <i>(nt)</i> mental noting  <b>[saṁ + √ñā + aṇa]</b>  <i>nt, act, from sañjānāti</i>
"#.trim().split("\n").map(|i| i.to_string()).collect();

    assert_eq!(result.len(), expected.len());

    for (idx, result_i) in result.iter().enumerate() {
        assert_eq!(result_i.to_string(), expected[idx].to_string());
    }
}

#[test]
#[serial]
fn test_dpd_lookup_generate_json() {
    h::app_data_setup();
    let app_data = get_app_data();

    let mut texts: HashMap<&str, &str> = HashMap::new();
    texts.insert("dpd_lookup"                         , "Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ. So vivicceva kāmehi vivicca akusalehi dhammehi savitakkaṁ savicāraṁ vivekajaṁ pītisukhaṁ paṭhamaṁ jhānaṁ upasampajja viharati. / Saddhassa hi, sāriputta, ariyasāvakassa āraddhavīriyassa upaṭṭhitassatino etaṁ pāṭikaṅkhaṁ yaṁ vossaggārammaṇaṁ karitvā labhissati samādhiṁ, labhissati cittassa ekaggataṁ. Yo hissa, sāriputta, samādhi tadassa samādhindriyaṁ.");
    texts.insert("yam-janna"                          , "yaṁ jaññā — ‘sakkomi ajjeva gantun’ti.");
    texts.insert("anumattesu-vajjesu"                 , "aṇumattesu vajjesu bhayadassāvino, samādāya sikkhatha sikkhāpadesū’ti");
    texts.insert("anasavanca-vo"                      , "“Anāsavañca vo, bhikkhave, desessāmi anāsavagāmiñca maggaṁ. Taṁ suṇātha. Katamañca, bhikkhave, anāsavaṁ …pe….");
    texts.insert("suriyassa-bhikkhave"                , "“Sūriyassa, bhikkhave, udayato");
    texts.insert("yatha-asankhatam"                   , "(Yathā asaṅkhataṁ tathā vitthāretabbaṁ.)");
    texts.insert("parens-48.10-katamanca-bhikkhave"   , "(SN 48.10) Katamañca, bhikkhave, samādhindriyaṁ?");
    texts.insert("brackets-48.10-katamanca-bhikkhave" , "[SN 48:10] Katamañca, bhikkhave, samādhindriyaṁ?");
    texts.insert("te-jananti"                         , "Te jānanti atthaññe āvāsikā bhikkhū");
    texts.insert("evametam-dharayami"                 , "evametaṁ dhārayāmī’”ti.");
    texts.insert("dassanaya"                          , "dassanāyā’ti");
    texts.insert("kilamittha"                         , "kilamitthā”ti?");
    texts.insert("addhanam-agata"                     , "addhānaṁ āgatā”ti.");
    texts.insert("migabandhake"                       , "migabandhake”ti");
    texts.insert("tanhadaso"                          , "taṇhādāso’ti");
    texts.insert("seyyo"                              , "seyyo”ti.");
    texts.insert("sikkhapadesu"                       , "sikkhāpadesū’ti");

    texts.insert("idha-nandati", r#"
18.

idha nandati pecca nandati, katapuñño ubhayattha nandati.

‘‘puññaṁ me kata’’nti nandati, bhiyyo nandati suggatiṁ gato..
"#);

    texts.insert("gataddhino", r#"
Gataddhino visokassa,
vippamuttassa sabbadhi;
Sabbaganthappahīnassa,
pariḷāho na vijjati.
"#);

    for (file_name, quote) in texts.into_iter() {
        // Use a BTreeMap for consistent key sorting across test runs.
        let mut lookup_data: BTreeMap<String, Vec<LookupResult>> = BTreeMap::new();

        for word in extract_words(quote) {
            if word.len() <= 1 {
                continue;
            }
            let word = normalize_query_text(Some(word.to_string()));
            let res = app_data.dbm.dpd.dpd_lookup(&word, false, true, None, None).unwrap();
            lookup_data.insert(word, LookupResult::from_search_results(&res));
        }

        let json = serde_json::to_string_pretty(&lookup_data).expect("Can't encode JSON");

        let path = PathBuf::from(format!("tests/data/{}.json", file_name));
        // fs::write(&path, json.clone()).expect("Unable to write file!");

        let expected_json = fs::read_to_string(&path).expect("Failed to read file");

        assert_eq!(json, expected_json);
    }
}

/// A dict_words-style word uid (sanitized lemma + "/dpd", e.g. "ko/dpd") should
/// resolve to its DPD headword in DpdLookup, the same word Combined mode reaches
/// via UidMatch against dict_words. This is what lets WordSummary append "/dpd"
/// to a short query (e.g. "ko" -> "ko/dpd") and still find the word.
#[test]
#[serial]
fn test_dpd_lookup_word_uid_form() {
    h::app_data_setup();
    let app_data = get_app_data();

    // Simple single-token lemmas: "ko/dpd" -> "ko", "i/dpd" -> "i".
    let res = app_data.dbm.dpd.dpd_lookup("ko/dpd", false, true, None, None).unwrap();
    assert!(res.iter().any(|r| r.title == "ko"),
            "ko/dpd should resolve to the 'ko' headword, got: {:?}",
            res.iter().map(|r| &r.title).collect::<Vec<_>>());

    let res = app_data.dbm.dpd.dpd_lookup("i/dpd", false, true, None, None).unwrap();
    assert!(!res.is_empty(), "i/dpd should resolve to a headword");

    // Numbered/sanitized lemma: "dhamma-1-01/dpd" -> lemma_1 "dhamma 1.01".
    let res = app_data.dbm.dpd.dpd_lookup("dhamma-1-01/dpd", false, true, None, None).unwrap();
    assert!(res.iter().any(|r| r.title == "dhamma 1.01"),
            "dhamma-1-01/dpd should resolve to the 'dhamma 1.01' headword, got: {:?}",
            res.iter().map(|r| &r.title).collect::<Vec<_>>());
}

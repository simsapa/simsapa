//! Sutta references resolved to gloss passages (`gloss_source`), against the
//! dist database, read-only.

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::gloss_source::{resolve_gloss_source, GlossSource, GlossSourceError, PaliEdition};

mod helpers;
use helpers as h;

fn resolve(reference: &str, edition: PaliEdition, paragraphs: Option<&str>) -> Result<GlossSource, GlossSourceError> {
    h::app_data_setup();
    resolve_gloss_source(&get_app_data().dbm.appdata, reference, edition, paragraphs)
}

#[test]
#[serial]
fn test_dhp_verse_inside_its_range_record() {
    // The CST Dhammapada record `dhp21-32/pli/cst` is keyed by the vagga, so it
    // cannot be narrowed to verse 23: the default edition reads /pli/ms.
    let s = resolve("dhp23", PaliEdition::Default, None).unwrap();
    assert_eq!(s.uid, "dhp21-32/pli/ms");
    assert_eq!(s.paragraphs.len(), 1);
    let p = &s.paragraphs[0];
    assert!(p.starts_with("Te jhāyino sātatikā,\n"), "{p}");
    assert!(p.contains("yogakkhemaṁ anuttaraṁ"), "{p}");
    assert!(!p.contains("Appamādo"), "{p}");
    assert_eq!(s.title(), "dhp23 (dhp21-32/pli/ms)");

    // An explicit CST edition is not substituted: it reads the whole vagga.
    let cst = resolve("dhp23", PaliEdition::Cst, None).unwrap();
    assert_eq!(cst.uid, "dhp21-32/pli/cst");
    assert!(cst.paragraph_count > 1);
}

#[test]
#[serial]
fn test_sn_default_is_cst_without_paragraph_numbers() {
    let s = resolve("sn1.71", PaliEdition::Default, None).unwrap();
    assert_eq!(s.uid, "sn1.71/pli/cst");
    assert!(s.paragraphs[0].starts_with("Sāvatthinidānaṁ."), "{}", s.paragraphs[0]);
    assert!(s.paragraphs.iter().all(|p| !p.contains('<') && !p.contains("Chetvāvaggo")));

    let ms = resolve("SN 1.71", PaliEdition::Ms, None).unwrap();
    assert_eq!(ms.uid, "sn1.71/pli/ms");
    assert_eq!(ms.paragraphs[0], "Sāvatthinidānaṁ. Ekamantaṁ ṭhitā kho sā devatā bhagavantaṁ gāthāya ajjhabhāsi:");
    assert!(ms.paragraphs[1].starts_with("“Kiṁsu chetvā sukhaṁ seti,\nkiṁsu chetvā na socati;"));
}

#[test]
#[serial]
fn test_sn18_1_second_paragraph() {
    let s = resolve("SN18.1", PaliEdition::Default, Some("2")).unwrap();
    assert_eq!(s.uid, "sn18.1/pli/cst");
    assert_eq!(s.paragraph_numbers, vec![2]);
    assert_eq!(s.paragraphs.len(), 1);
    assert!(s.paragraphs[0].starts_with("‘‘Taṁ kiṁ maññasi, rāhula, cakkhuṁ niccaṁ vā aniccaṁ vā’’ti?"), "{}", s.paragraphs[0]);
    assert_eq!(s.title(), "SN18.1 ¶2 (sn18.1/pli/cst)");

    assert!(matches!(resolve("sn18.1", PaliEdition::Default, Some("99")), Err(GlossSourceError::BadRequest(_))));
}

#[test]
#[serial]
fn test_dpd_codes_ud_and_iti() {
    // The code is read the same in either edition; MS keeps the text fixed.
    let ud = resolve("UD1", PaliEdition::Ms, None).unwrap();
    assert_eq!(ud.uid, "ud1.1/pli/ms");
    assert!(ud.text().contains("Yadā have pātubhavanti dhammā"), "{}", ud.text());

    let iti = resolve("ITI1", PaliEdition::Ms, None).unwrap();
    assert_eq!(iti.uid, "iti1/pli/ms");
    assert!(iti.text().contains("Yena lobhena luddhāse"), "{}", iti.text());

    // The default edition reads CST where it has the record.
    assert_eq!(resolve("UD1", PaliEdition::Default, None).unwrap().uid, "ud1.1/pli/cst");
    assert_eq!(resolve("ITI1", PaliEdition::Default, None).unwrap().uid, "iti1/pli/cst");
}

#[test]
#[serial]
fn test_theragatha_verse_number() {
    // SuttaCentral's verse 215, the first of thag2.48 (DPD numbers TH
    // differently: its TH215 is in thag6.6).
    let s = resolve("Th 215", PaliEdition::Default, None).unwrap();
    assert_eq!(s.uid, "thag2.48/pli/ms");
    assert_eq!(s.paragraphs.len(), 1);
    assert!(s.paragraphs[0].starts_with("“Saṁsaraṁ dīghamaddhānaṁ,\n"), "{}", s.paragraphs[0]);
    assert_eq!(s.paragraphs[0].lines().count(), 4, "{}", s.paragraphs[0]);
}

#[test]
#[serial]
fn test_bad_references() {
    assert!(matches!(resolve("evaṁ me sutaṁ", PaliEdition::Default, None), Err(GlossSourceError::BadRequest(_))));
    assert!(matches!(resolve("zz999", PaliEdition::Default, None), Err(GlossSourceError::NotFound(_))));
}

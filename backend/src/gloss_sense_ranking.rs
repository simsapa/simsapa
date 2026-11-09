//! The default sense of a gloss token when nothing else decides: no cache
//! row, no set phrase and no AI answer. DPD's ranking without an AI score
//! (docs/dpd-analysis-procedure.md §1.6 ranks 3–10, and the headword and
//! particle filters of §1.2 steps 6–7 as a demotion), applied by setting
//! `ProcessedWord::selected_index`. `results` is never reordered: the Gloss
//! tab's ComboBox index, the AI item options and saved sessions refer to its
//! order.
//!
//! It runs on every paragraph of the shared pipeline (`process_all_paragraphs`,
//! `process_single_paragraph`), so the Headwords and Word in Sentence formats
//! default to the same sense. A cached or AI-selected sense is never changed,
//! nor any `context_hash`.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use lazy_static::lazy_static;
use regex::Regex;

use crate::db::dpd::{headword_row_fields_by_ids, DpdDbHandle, HeadwordRowFields, LookupResult};
use crate::gloss_rows::{grammar_case_rank, inflected_readings, Reading, ReadingSource};
use crate::types::{ProcessedWord, WordProcessingResult};

/// DPD's `exceptions_list`: grammarian's entries left out of the options.
pub const DPD_EXCLUDED_POS: [&str; 6] = ["cs", "letter", "prefix", "root", "suffix", "ve"];

/// DPD's `sandhi_particles`: a lookup row with a split ending in one of these
/// also lists the headwords of the split's parts.
const SANDHI_PARTICLES: [&str; 6] = [" + api", " + ca", " + eva", " + iti", " + iva", " + hi"];

/// DPD's headword filters (`get_word_details`): a stem with `!`, a `pos` in
/// `DPD_EXCLUDED_POS`, `(gram)` in `meaning_1`, and inside a construction a
/// `pos` of `abbrev`.
pub fn dpd_filtered_headword(hw: &HeadwordRowFields, in_construction: bool) -> bool {
    hw.stem.contains('!')
        || DPD_EXCLUDED_POS.contains(&hw.pos.as_str())
        || hw.meaning_1.contains("(gram)")
        || (in_construction && hw.pos == "abbrev")
}

/// DPD's `_has_particle_deconstructor`, over the break-downs of the word.
fn has_particle_split(pw: &ProcessedWord) -> bool {
    lazy_static! {
        static ref RE_NOTE: Regex = Regex::new(r" \[.+").unwrap();
    }
    pw.deconstructions.iter().any(|d| {
        let clean = RE_NOTE.replace(&d.words_joined, "");
        SANDHI_PARTICLES.iter().any(|p| clean.ends_with(p))
    })
}

/// DPD's particle filter keeps a sense of a particle-split row only when it
/// is a sandhi headword or has a reading from the row's grammar triples.
fn kept_by_particle_filter(hw: &HeadwordRowFields, readings: &[Reading]) -> bool {
    hw.pos == "sandhi"
        || hw.grammar.contains("sandhi")
        || readings.iter().any(|r| matches!(r.source, ReadingSource::Triple(_)))
}

/// DPD's `_normalize_example_text`, with `<b>` tags removed (DPD's
/// `_strip_bold`) and `ṃ` as `ṁ`: lower case, no apostrophes, every run of
/// non-word characters one space.
pub fn example_overlap_text(text: &str) -> String {
    lazy_static! {
        static ref RE_NON_WORD: Regex = Regex::new(r"[^\w]+").unwrap();
    }
    let text = text
        .replace("<b>", "")
        .replace("</b>", "")
        .to_lowercase()
        .replace('ṃ', "ṁ")
        .replace(['\'', '’', '‘', 'ʼ'], "");
    RE_NON_WORD.replace_all(&text, " ").trim().to_string()
}

/// DPD's `_texts_overlap` over texts already passed through
/// `example_overlap_text`: either contains the other.
fn texts_overlap(first: &str, second: &str) -> bool {
    !first.is_empty() && !second.is_empty() && (first.contains(second) || second.contains(first))
}

/// One sense of a token as the ranking sees it.
struct RankedSense {
    /// 3: a headword of the word's `lookup` row (DPD's options); 2: a
    /// headword only Simsapa's other lookup phases found (a `lemma_clean`
    /// or stem match, docs/dpd-analysis-procedure.md §3 gap 18); 1: a result
    /// that is not a headword (a root); 0: a headword DPD's filters leave
    /// out (demoted).
    tier: u8,
    /// DPD's `_db_example_rank`: 1 when an example overlaps the paragraph
    /// (`text_overlap`). DPD's 2, `source_text_overlap`, needs the passage's
    /// source code and is not given.
    db_example: u8,
    /// DPD's `_direct_key_rank`: 1 for a reading from the lookup triples.
    direct_key: u8,
    /// DPD's `_dictionary_quality_rank`.
    quality: u8,
    /// DPD's `_grammar_case_rank` of the best reading.
    case: u8,
    /// DPD's completeness score.
    completeness: u8,
    /// DPD's `stable_id_rank`: the lower id wins.
    stable_id: i64,
}

impl RankedSense {
    fn key(&self) -> (u8, u8, u8, u8, u8, u8, i64) {
        (self.tier, self.db_example, self.direct_key, self.quality, self.case, self.completeness, self.stable_id)
    }
}

fn result_headword_id(r: &LookupResult) -> Option<i32> {
    r.uid.strip_suffix("/dpd").and_then(|id| id.parse::<i32>().ok())
}

fn rank_sense(hw: Option<&HeadwordRowFields>, pw: &ProcessedWord, particle_split: bool, paragraph: &str) -> RankedSense {
    let Some(hw) = hw else {
        return RankedSense { tier: 1, db_example: 0, direct_key: 0, quality: 0, case: 0, completeness: 0, stable_id: i64::MIN };
    };
    let readings = inflected_readings(hw, &pw.lookup_grammar);
    let demoted = dpd_filtered_headword(hw, false) || (particle_split && !kept_by_particle_filter(hw, &readings));
    let db_example = [&hw.example_1, &hw.example_2]
        .iter()
        .any(|e| texts_overlap(&example_overlap_text(e), paragraph)) as u8;
    let quality = match (hw.meaning_1.is_empty(), hw.example_1.is_empty() && hw.example_2.is_empty()) {
        (true, _) => 0,
        (false, true) => 1,
        (false, false) => 2,
    };
    let completeness = match (hw.meaning_1.is_empty(), hw.source_1.is_empty()) {
        (true, _) => 0,
        (false, true) => 1,
        (false, false) => 2,
    };
    RankedSense {
        tier: if demoted {
            0
        } else if pw.lookup_headword_ids.contains(&hw.id) {
            3
        } else {
            2
        },
        db_example,
        direct_key: readings.iter().any(|r| matches!(r.source, ReadingSource::Triple(_))) as u8,
        quality,
        case: readings.iter().map(|r| grammar_case_rank(&r.text)).max().unwrap_or(0),
        completeness,
        stable_id: -(hw.id as i64),
    }
}

/// The senses of a direct or mixed word: its direct results, which occupy the
/// front of `results` (all of them for a word without `direct_uids`).
fn sense_count(pw: &ProcessedWord) -> usize {
    if pw.direct_uids.is_empty() {
        pw.results.len()
    } else {
        pw.results.iter().filter(|r| pw.direct_uids.contains(&r.uid)).count()
    }
}

/// Ranks the senses of every ambiguous direct or mixed word of a paragraph:
/// sets `demoted_uids` (the senses DPD's filters leave out, which the AI is
/// not offered) and, when no cache row or set phrase resolved the word,
/// `selected_index` to the best sense (the first among equals). One batched
/// headword query per paragraph. Deconstructor-resolved words are left as
/// they are: their components are chosen per component.
pub fn rank_paragraph_default_senses(
    dpd: &DpdDbHandle,
    words: &mut [Option<WordProcessingResult>],
    paragraph_text: &str,
) -> Result<()> {
    let ambiguous = |pw: &ProcessedWord| {
        let is_deconstructor_resolved = pw.direct_uids.is_empty() && !pw.deconstructions.is_empty();
        !is_deconstructor_resolved && sense_count(pw) > 1
    };

    let ids: Vec<i32> = words
        .iter()
        .filter_map(|w| match w {
            Some(WordProcessingResult::Recognized(pw)) if ambiguous(pw) => Some(pw),
            _ => None,
        })
        .flat_map(|pw| pw.results[..sense_count(pw)].iter().filter_map(result_headword_id))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let headwords: HashMap<i32, HeadwordRowFields> = {
        let mut conn = dpd.get_conn().context("rank_paragraph_default_senses: no dpd connection")?;
        headword_row_fields_by_ids(&mut conn, &ids)?
    };
    let paragraph = example_overlap_text(paragraph_text);

    for w in words.iter_mut() {
        let Some(WordProcessingResult::Recognized(pw)) = w else { continue };
        if !ambiguous(pw) {
            continue;
        }
        let particle_split = has_particle_split(pw);
        let n = sense_count(pw);
        let ranked: Vec<RankedSense> = pw.results[..n]
            .iter()
            .map(|r| rank_sense(result_headword_id(r).and_then(|id| headwords.get(&id)), pw, particle_split, &paragraph))
            .collect();

        pw.demoted_uids = pw.results[..n]
            .iter()
            .zip(&ranked)
            .filter(|(_, s)| s.tier == 0)
            .map(|(r, _)| r.uid.clone())
            .collect();

        if pw.resolution.is_none() {
            let mut best = 0;
            for (i, s) in ranked.iter().enumerate() {
                if s.key() > ranked[best].key() {
                    best = i;
                }
            }
            pw.selected_index = best as i32;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_text_normalisation() {
        assert_eq!(
            example_overlap_text("sīlavantassa <b>jhāyino</b>.\nek'āhaṃ jīvitaṃ, mahinda-ghosa"),
            "sīlavantassa jhāyino ekāhaṁ jīvitaṁ mahinda ghosa"
        );
    }

    #[test]
    fn overlap_either_way() {
        let example = example_overlap_text("yo ca vassasataṁ jīve, dussīlo asamāhito, ek'āhaṁ jīvitaṁ seyyo, sīlavantassa <b>jhāyino</b>.");
        let passage = example_overlap_text("Yo ca vassasataṃ jīve, dussīlo asamāhito; ekāhaṃ jīvitaṃ seyyo, sīlavantassa jhāyino.");
        assert!(texts_overlap(&example, &passage));
        assert!(texts_overlap(&example_overlap_text("ekāhaṁ jīvitaṁ seyyo"), &example));
        assert!(!texts_overlap(&example_overlap_text("jīvitaṁ seyyo ekāhaṁ"), &example));
        assert!(!texts_overlap("", &example));
    }

    #[test]
    fn headword_filters() {
        let hw = |stem: &str, pos: &str, meaning_1: &str| HeadwordRowFields {
            stem: stem.to_string(),
            pos: pos.to_string(),
            meaning_1: meaning_1.to_string(),
            ..Default::default()
        };
        assert!(dpd_filtered_headword(&hw("!*", "ind", "therefore"), false));
        assert!(dpd_filtered_headword(&hw("t", "letter", "letter t"), false));
        assert!(dpd_filtered_headword(&hw("x", "ind", "particle (gram)"), false));
        assert!(!dpd_filtered_headword(&hw("x", "abbrev", "abbreviation"), false));
        assert!(dpd_filtered_headword(&hw("x", "abbrev", "abbreviation"), true));
        assert!(!dpd_filtered_headword(&hw("dhamm", "masc", "nature"), false));
    }
}

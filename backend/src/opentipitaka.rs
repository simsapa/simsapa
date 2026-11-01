//! The OpenTipitaka English records (`mn1/en/opentipitaka`).
//!
//! The English is AI-translated and CC BY-NC-SA, so each text must say so where
//! it is read. Both the bootstrap import and the reader add these footers, which
//! is why they live here: `backend` does not link `tipitaka_xml_parser`. A
//! `bootstrap_cli` test asserts that the shared constants equal the parser's.

/// Uid author of all OT English, as in `mn1/en/opentipitaka`.
pub const OT_AUTHOR_UID: &str = "opentipitaka";

/// Footer text of every OT English text.
pub const OT_ENGLISH_FOOTER: &str =
    "This English translation was produced by AI (www.opentipitaka.org)";

/// Licence of the OT English.
pub const OT_LICENSE: &str = "CC-BY-NC-SA 4.0";

/// Source of the OT data.
pub const OT_SOURCE_URL: &str = "https://www.opentipitaka.org/";

/// Text of the licence. CC BY-NC-SA 4.0 §3(a)(1) requires a link to it.
pub const OT_LICENSE_URL: &str = "https://creativecommons.org/licenses/by-nc-sa/4.0/";

/// The licence, the source, and the statement that the material was modified,
/// which §3(a)(1) also requires. The import adds this after the exported AI
/// footer. The `license` column is not displayed, so it does not take its place.
pub fn ot_license_footer_html() -> String {
    format!(
        "<footer class='noindex'>Licence: <a href='{OT_LICENSE_URL}'>{OT_LICENSE}</a>. \
         Source: <a href='{OT_SOURCE_URL}'>opentipitaka.org</a>. \
         Arranged in lines for Simsapa, with the niggahīta spelled ṁ.</footer>"
    )
}

/// The AI footer followed by the licence footer. The reader adds this when the
/// English is a column but the opened text is not, because the aligned view uses
/// the opened text's template.
pub fn ot_footer_html() -> String {
    format!(
        "<footer class='noindex'>{OT_ENGLISH_FOOTER}</footer>{}",
        ot_license_footer_html()
    )
}

/// Whether `uid` is an OpenTipitaka English record.
pub fn is_ot_english_uid(uid: &str) -> bool {
    uid.ends_with(&format!("/en/{OT_AUTHOR_UID}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_license_footer_links_the_licence_and_the_source_and_states_the_change() {
        let footer = ot_license_footer_html();

        assert!(footer.starts_with("<footer class='noindex'>"), "{}", footer);
        assert!(footer.ends_with("</footer>"), "{}", footer);
        assert!(
            footer.contains(
                "<a href='https://creativecommons.org/licenses/by-nc-sa/4.0/'>CC-BY-NC-SA 4.0</a>"
            ),
            "{}",
            footer
        );
        assert!(
            footer.contains("<a href='https://www.opentipitaka.org/'>opentipitaka.org</a>"),
            "{}",
            footer
        );
        assert!(
            footer.contains("Arranged in lines for Simsapa, with the niggahīta spelled ṁ."),
            "{}",
            footer
        );
    }

    #[test]
    fn the_full_footer_says_it_is_ai_translated_before_the_licence() {
        let footer = ot_footer_html();

        assert!(
            footer.starts_with(&format!("<footer class='noindex'>{}</footer>", OT_ENGLISH_FOOTER)),
            "{}",
            footer
        );
        assert!(footer.ends_with(&ot_license_footer_html()), "{}", footer);
    }

    #[test]
    fn only_a_translation_by_the_ot_author_is_an_ot_english_uid() {
        assert!(is_ot_english_uid("mn1/en/opentipitaka"));
        assert!(is_ot_english_uid("mn1.att/en/opentipitaka"));
        assert!(is_ot_english_uid("vin02m4.mul0.xml/en/opentipitaka"));

        assert!(!is_ot_english_uid("mn1/en/sujato"));
        assert!(!is_ot_english_uid("mn1/pli/cst"));
        assert!(!is_ot_english_uid("mn1/de/opentipitaka"));
        assert!(!is_ot_english_uid("opentipitaka"));
    }
}

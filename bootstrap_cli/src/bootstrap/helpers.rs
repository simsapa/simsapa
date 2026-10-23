use regex::Regex;
use simsapa_backend::db::appdata_models::NewSutta;
use simsapa_backend::helpers::sutta_range_from_ref;

/// Owned version of sutta data for building during parsing.
/// This allows us to build sutta data with owned strings during parsing,
/// then convert to borrowed NewSutta for database insertion.
#[derive(Debug, Clone)]
pub struct SuttaData {
    pub uid: String,
    pub sutta_ref: String,
    pub nikaya: String,
    pub language: String,
    pub title: String,
    pub title_ascii: String,
    pub title_pali: Option<String>,
    pub content_plain: String,
    pub content_html: String,
    pub source_uid: String,
    pub sutta_range_group: Option<String>,
    pub sutta_range_start: Option<i32>,
    pub sutta_range_end: Option<i32>,
}

impl SuttaData {
    /// Parse the uid and populate range fields
    pub fn parse_range_from_uid(uid: &str) -> (Option<String>, Option<i32>, Option<i32>) {
        if let Some(range) = sutta_range_from_ref(uid) {
            let start = range.start.map(|s| s as i32);
            let end = range.end.map(|e| e as i32);
            (Some(range.group), start, end)
        } else {
            (None, None, None)
        }
    }

    /// Convert to NewSutta for database insertion
    pub fn to_new_sutta(&self) -> NewSutta<'_> {
        NewSutta {
            uid: &self.uid,
            sutta_ref: &self.sutta_ref,
            nikaya: &self.nikaya,
            language: &self.language,
            group_path: None,
            group_index: None,
            order_index: None,
            sutta_range_group: self.sutta_range_group.as_deref(),
            sutta_range_start: self.sutta_range_start,
            sutta_range_end: self.sutta_range_end,
            title: Some(&self.title),
            title_ascii: Some(&self.title_ascii),
            title_pali: self.title_pali.as_deref(),
            title_trans: None,
            description: None,
            content_plain: Some(&self.content_plain),
            content_html: Some(&self.content_html),
            content_json: None,
            content_json_tmpl: None,
            source_uid: Some(&self.source_uid),
            source_info: None,
            source_language: None,
            message: None,
            copyright: None,
            license: None,
        }
    }
}

/// sn12.23 to SN 12.23
pub fn uid_to_ref(uid: &str) -> String {
    // Add a space after the letters, i.e. the collection abbrev
    let re = Regex::new(r"^([a-z]+)([0-9])").unwrap();
    let mut ref_str = re.replace(uid, "$1 $2").to_string();

    // handle all-upcase collections
    let replacements = [
        ("dn ", "DN "),
        ("mn ", "MN "),
        ("sn ", "SN "),
        ("an ", "AN "),
    ];

    for (from, to) in &replacements {
        ref_str = ref_str.replace(from, to);
    }

    // titlecase the rest, upcase the first letter
    if !ref_str.is_empty() {
        let first_char = ref_str.chars().next().unwrap().to_uppercase().to_string();
        ref_str = first_char + &ref_str[1..];
    }

    ref_str
}

/// sn12.23 to sn
pub fn uid_to_nikaya(uid: &str) -> String {
    let re = Regex::new(r"^([a-z]+).*").unwrap();
    if let Some(caps) = re.captures(uid) {
        caps[1].to_string()
    } else {
        "unknown".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uid_to_ref() {
        assert_eq!(uid_to_ref("dn1"), "DN 1");
        assert_eq!(uid_to_ref("mn2"), "MN 2");
        assert_eq!(uid_to_ref("sn12.23"), "SN 12.23");
        assert_eq!(uid_to_ref("an4.10"), "AN 4.10");
    }

    #[test]
    fn test_uid_to_nikaya() {
        assert_eq!(uid_to_nikaya("dn1"), "dn");
        assert_eq!(uid_to_nikaya("mn2"), "mn");
        assert_eq!(uid_to_nikaya("sn12.23"), "sn");
        assert_eq!(uid_to_nikaya("an4.10"), "an");
    }
}

//! Finding CST records by the name of the source file they were imported from.
//!
//! A CST file that is not split into mapped fragments is imported under its
//! own file name: whole as `<name>.xml/pli/cst` (`vin02m4.mul.xml`), or as
//! the divisions its toc names, `<name><n>.xml/pli/cst` (`vin02m4.mul0.xml`
//! … `vin02m4.mul17.xml`). Such a name fits none of the sutta reference
//! patterns, so a typed `vin02` or `e01` was searched as text and found
//! nothing.
//!
//! The source file names are a fixed list, the `romn/` folder of the
//! tipitaka.org CST edition, so the query is matched against that list rather
//! than against a pattern: a query that contains a letter and a digit and is a
//! substring of a file name names that file. Which records a file has depends on how the
//! bootstrap imported it, so the records are looked up in the database, by the
//! file's name as a uid prefix.
//!
//! A file whose text is imported as mapped fragments (`mn11.att/pli/cst`) has
//! no records under its file name, and a query naming only such files finds
//! nothing.

/// The 217 source files of the CST edition, `romn/*.xml`.
pub static CST_SOURCE_FILES: [&str; 217] = [
    "abh01a.att.xml",
    "abh01m.mul.xml",
    "abh01t.tik.xml",
    "abh02a.att.xml",
    "abh02m.mul.xml",
    "abh02t.tik.xml",
    "abh03a.att.xml",
    "abh03m1.mul.xml",
    "abh03m10.mul.xml",
    "abh03m11.mul.xml",
    "abh03m2.mul.xml",
    "abh03m3.mul.xml",
    "abh03m4.mul.xml",
    "abh03m5.mul.xml",
    "abh03m6.mul.xml",
    "abh03m7.mul.xml",
    "abh03m8.mul.xml",
    "abh03m9.mul.xml",
    "abh03t.tik.xml",
    "abh04t.nrf.xml",
    "abh05t.nrf.xml",
    "abh06t.nrf.xml",
    "abh07t.nrf.xml",
    "abh08t.nrf.xml",
    "abh09t.nrf.xml",
    "e0101n.mul.xml",
    "e0102n.mul.xml",
    "e0103n.att.xml",
    "e0104n.att.xml",
    "e0105n.nrf.xml",
    "e0201n.nrf.xml",
    "e0301n.nrf.xml",
    "e0401n.nrf.xml",
    "e0501n.nrf.xml",
    "e0601n.nrf.xml",
    "e0602n.nrf.xml",
    "e0603n.nrf.xml",
    "e0604n.nrf.xml",
    "e0605n.nrf.xml",
    "e0606n.nrf.xml",
    "e0607n.nrf.xml",
    "e0608n.nrf.xml",
    "e0701n.nrf.xml",
    "e0702n.nrf.xml",
    "e0703n.nrf.xml",
    "e0801n.nrf.xml",
    "e0802n.nrf.xml",
    "e0803n.nrf.xml",
    "e0804n.nrf.xml",
    "e0805n.nrf.xml",
    "e0806n.nrf.xml",
    "e0807n.nrf.xml",
    "e0808n.nrf.xml",
    "e0809n.nrf.xml",
    "e0810n.nrf.xml",
    "e0811n.nrf.xml",
    "e0812n.nrf.xml",
    "e0813n.nrf.xml",
    "e0901n.nrf.xml",
    "e0902n.nrf.xml",
    "e0903n.nrf.xml",
    "e0904n.nrf.xml",
    "e0905n.nrf.xml",
    "e0906n.nrf.xml",
    "e0907n.nrf.xml",
    "e1001n.nrf.xml",
    "e1002n.nrf.xml",
    "e1003n.nrf.xml",
    "e1004n.nrf.xml",
    "e1005n.nrf.xml",
    "e1006n.nrf.xml",
    "e1007n.nrf.xml",
    "e1008n.nrf.xml",
    "e1009n.nrf.xml",
    "e1010n.nrf.xml",
    "e1101n.nrf.xml",
    "e1102n.nrf.xml",
    "e1103n.nrf.xml",
    "e1201n.nrf.xml",
    "e1202n.nrf.xml",
    "e1203n.nrf.xml",
    "e1204n.nrf.xml",
    "e1205n.nrf.xml",
    "e1206n.nrf.xml",
    "e1207n.nrf.xml",
    "e1208n.nrf.xml",
    "e1209n.nrf.xml",
    "e1210n.nrf.xml",
    "e1211n.nrf.xml",
    "e1212n.nrf.xml",
    "e1213n.nrf.xml",
    "e1214n.nrf.xml",
    "e1215n.nrf.xml",
    "s0101a.att.xml",
    "s0101m.mul.xml",
    "s0101t.tik.xml",
    "s0102a.att.xml",
    "s0102m.mul.xml",
    "s0102t.tik.xml",
    "s0103a.att.xml",
    "s0103m.mul.xml",
    "s0103t.tik.xml",
    "s0104t.nrf.xml",
    "s0105t.nrf.xml",
    "s0201a.att.xml",
    "s0201m.mul.xml",
    "s0201t.tik.xml",
    "s0202a.att.xml",
    "s0202m.mul.xml",
    "s0202t.tik.xml",
    "s0203a.att.xml",
    "s0203m.mul.xml",
    "s0203t.tik.xml",
    "s0301a.att.xml",
    "s0301m.mul.xml",
    "s0301t.tik.xml",
    "s0302a.att.xml",
    "s0302m.mul.xml",
    "s0302t.tik.xml",
    "s0303a.att.xml",
    "s0303m.mul.xml",
    "s0303t.tik.xml",
    "s0304a.att.xml",
    "s0304m.mul.xml",
    "s0304t.tik.xml",
    "s0305a.att.xml",
    "s0305m.mul.xml",
    "s0305t.tik.xml",
    "s0401a.att.xml",
    "s0401m.mul.xml",
    "s0401t.tik.xml",
    "s0402a.att.xml",
    "s0402m1.mul.xml",
    "s0402m2.mul.xml",
    "s0402m3.mul.xml",
    "s0402t.tik.xml",
    "s0403a.att.xml",
    "s0403m1.mul.xml",
    "s0403m2.mul.xml",
    "s0403m3.mul.xml",
    "s0403t.tik.xml",
    "s0404a.att.xml",
    "s0404m1.mul.xml",
    "s0404m2.mul.xml",
    "s0404m3.mul.xml",
    "s0404m4.mul.xml",
    "s0404t.tik.xml",
    "s0501a.att.xml",
    "s0501m.mul.xml",
    "s0501t.nrf.xml",
    "s0502a.att.xml",
    "s0502m.mul.xml",
    "s0503a.att.xml",
    "s0503m.mul.xml",
    "s0504a.att.xml",
    "s0504m.mul.xml",
    "s0505a.att.xml",
    "s0505m.mul.xml",
    "s0506a.att.xml",
    "s0506m.mul.xml",
    "s0507a.att.xml",
    "s0507m.mul.xml",
    "s0508a1.att.xml",
    "s0508a2.att.xml",
    "s0508m.mul.xml",
    "s0509a.att.xml",
    "s0509m.mul.xml",
    "s0510a.att.xml",
    "s0510m1.mul.xml",
    "s0510m2.mul.xml",
    "s0511a.att.xml",
    "s0511m.mul.xml",
    "s0512a.att.xml",
    "s0512m.mul.xml",
    "s0513a1.att.xml",
    "s0513a2.att.xml",
    "s0513a3.att.xml",
    "s0513a4.att.xml",
    "s0513m.mul.xml",
    "s0514a1.att.xml",
    "s0514a2.att.xml",
    "s0514a3.att.xml",
    "s0514m.mul.xml",
    "s0515a.att.xml",
    "s0515m.mul.xml",
    "s0516a.att.xml",
    "s0516m.mul.xml",
    "s0517a.att.xml",
    "s0517m.mul.xml",
    "s0518m.nrf.xml",
    "s0519a.att.xml",
    "s0519m.mul.xml",
    "s0519t.tik.xml",
    "s0520m.nrf.xml",
    "vin01a.att.xml",
    "vin01m.mul.xml",
    "vin01t1.tik.xml",
    "vin01t2.tik.xml",
    "vin02a1.att.xml",
    "vin02a2.att.xml",
    "vin02a3.att.xml",
    "vin02a4.att.xml",
    "vin02m1.mul.xml",
    "vin02m2.mul.xml",
    "vin02m3.mul.xml",
    "vin02m4.mul.xml",
    "vin02t.tik.xml",
    "vin04t.nrf.xml",
    "vin05t.nrf.xml",
    "vin06t.nrf.xml",
    "vin07t.nrf.xml",
    "vin08t.nrf.xml",
    "vin09t.nrf.xml",
    "vin10t.nrf.xml",
    "vin11t.nrf.xml",
    "vin12t.nrf.xml",
    "vin13t.nrf.xml",
];

/// The file extensions in [`CST_SOURCE_FILES`], before `.xml`.
const CST_FILE_KINDS: [&str; 4] = ["mul", "att", "tik", "nrf"];

/// What a query names among the CST source files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CstFileQuery {
    /// One division of a split file, by its full uid:
    /// `vin02m4.mul0` → `vin02m4.mul0.xml/pli/cst`.
    Division(String),
    /// Every record of these files, given without `.xml`: `e01` →
    /// `e0101n.mul`, `e0102n.mul`, …
    Files(Vec<&'static str>),
}

/// The CST source files a query names, or `None` if it names none.
///
/// The query must contain a letter and a digit, so that a word which happens to
/// be part of a file name (`mul`, `att`) or a number (`101`) is still searched
/// as text. A uid with a source
/// suffix (`vin02m4.mul0.xml/pli/cst`) is left to the exact uid match.
pub fn cst_file_query(query: &str) -> Option<CstFileQuery> {
    let query = query.trim().to_lowercase();
    if query.contains('/')
        || !query.chars().any(|c| c.is_ascii_digit())
        || !query.chars().any(|c| c.is_ascii_lowercase())
    {
        return None;
    }

    if let Some(division_uid) = division_uid(&query) {
        return Some(CstFileQuery::Division(division_uid));
    }

    let files: Vec<&'static str> = CST_SOURCE_FILES
        .iter()
        .filter(|name| name.contains(query.as_str()))
        .map(|name| name.trim_end_matches(".xml"))
        .collect();

    if files.is_empty() {
        None
    } else {
        Some(CstFileQuery::Files(files))
    }
}

/// `vin02m4.mul0` or `vin02m4.mul0.xml` → `vin02m4.mul0.xml/pli/cst`, when
/// `vin02m4.mul.xml` is a source file. The division number follows the file
/// kind, so it cannot be confused with the digits of the file name itself
/// (`abh03m10.mul`).
fn division_uid(query: &str) -> Option<String> {
    let query = query.strip_suffix(".xml").unwrap_or(query);
    let (base, tail) = query.rsplit_once('.')?;
    let kind = CST_FILE_KINDS.iter().find(|kind| tail.starts_with(**kind))?;
    let number = &tail[kind.len()..];
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let file_name = format!("{}.{}.xml", base, kind);
    if !CST_SOURCE_FILES.contains(&file_name.as_str()) {
        return None;
    }
    Some(format!("{}.{}{}.xml/pli/cst", base, kind, number))
}

/// Whether `uid` is one of the records of `file` (given without `.xml`): the
/// whole file, `<file>.xml/pli/cst`, or a division, `<file><n>.xml/pli/cst`.
pub fn uid_is_record_of_file(uid: &str, file: &str) -> bool {
    let Some(rest) = uid.strip_prefix(file) else {
        return false;
    };
    let Some(number) = rest.strip_suffix(".xml/pli/cst") else {
        return false;
    };
    number.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(query: &str) -> Vec<&'static str> {
        match cst_file_query(query) {
            Some(CstFileQuery::Files(files)) => files,
            other => panic!("expected Files for '{}', got {:?}", query, other),
        }
    }

    #[test]
    fn test_source_file_list_is_sorted_and_unique() {
        let mut sorted = CST_SOURCE_FILES.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted, CST_SOURCE_FILES.to_vec());
    }

    #[test]
    fn test_every_source_file_has_a_known_kind() {
        for name in CST_SOURCE_FILES {
            let stem = name.strip_suffix(".xml").unwrap();
            let (_, kind) = stem.rsplit_once('.').unwrap();
            assert!(CST_FILE_KINDS.contains(&kind), "{}", name);
        }
    }

    #[test]
    fn test_substring_of_a_file_name() {
        assert_eq!(files("e01"), vec!["e0101n.mul", "e0102n.mul", "e0103n.att", "e0104n.att", "e0105n.nrf"]);
        assert_eq!(files("vin02"), vec![
            "vin02a1.att", "vin02a2.att", "vin02a3.att", "vin02a4.att",
            "vin02m1.mul", "vin02m2.mul", "vin02m3.mul", "vin02m4.mul", "vin02t.tik",
        ]);
        assert_eq!(files("vin02m4.mul"), vec!["vin02m4.mul"]);
        assert_eq!(files("VIN02M4.MUL.XML"), vec!["vin02m4.mul"]);
        assert_eq!(files("0101n"), vec!["e0101n.mul"]);
    }

    #[test]
    fn test_division() {
        assert_eq!(
            cst_file_query("vin02m4.mul0"),
            Some(CstFileQuery::Division("vin02m4.mul0.xml/pli/cst".to_string()))
        );
        assert_eq!(
            cst_file_query("vin02m4.mul17.xml"),
            Some(CstFileQuery::Division("vin02m4.mul17.xml/pli/cst".to_string()))
        );
        assert_eq!(
            cst_file_query("abh03m10.mul2"),
            Some(CstFileQuery::Division("abh03m10.mul2.xml/pli/cst".to_string()))
        );
        // Not a source file, so not a division of one.
        assert_eq!(cst_file_query("vin99m4.mul0"), None);
    }

    #[test]
    fn test_not_a_file_query() {
        // No digit: words that are parts of file names stay text queries.
        assert_eq!(cst_file_query("mul"), None);
        assert_eq!(cst_file_query("vin"), None);
        assert_eq!(cst_file_query("att"), None);
        // No letter: numbers stay text queries.
        assert_eq!(cst_file_query("101"), None);
        // Ordinary queries and sutta references.
        assert_eq!(cst_file_query("vinaya"), None);
        assert_eq!(cst_file_query("mn1"), None);
        assert_eq!(cst_file_query("sn56.11"), None);
        // A full uid goes to the exact match.
        assert_eq!(cst_file_query("vin02m4.mul0.xml/pli/cst"), None);
    }

    #[test]
    fn test_uid_is_record_of_file() {
        assert!(uid_is_record_of_file("vin02m4.mul.xml/pli/cst", "vin02m4.mul"));
        assert!(uid_is_record_of_file("vin02m4.mul0.xml/pli/cst", "vin02m4.mul"));
        assert!(uid_is_record_of_file("vin02m4.mul17.xml/pli/cst", "vin02m4.mul"));
        assert!(!uid_is_record_of_file("abh03m10.mul0.xml/pli/cst", "abh03m1.mul"));
        assert!(!uid_is_record_of_file("vin02m4.mulx.xml/pli/cst", "vin02m4.mul"));
        assert!(!uid_is_record_of_file("mn11.att/pli/cst", "s0201a.att"));
    }
}

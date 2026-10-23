use anyhow::{Context, Result};
use diesel::prelude::*;
use std::path::PathBuf;
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;

use simsapa_backend::db::appdata_schema::suttas;
use simsapa_backend::helpers::{pali_to_ascii, consistent_niggahita, sutta_html_to_plain_text, dhp_chapter_ref_for_verse_num, thig_verse_to_uid};
use simsapa_backend::logger;

use crate::bootstrap::SuttaImporter;
use crate::bootstrap::helpers::{SuttaData, uid_to_ref, uid_to_nikaya};

/// Represents a sutta record from the Buddha Ujja legacy database
#[derive(Debug, Clone, QueryableByName)]
#[allow(dead_code)]
struct BuddhaUjjaSutta {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    id: i32,
    #[diesel(sql_type = diesel::sql_types::Text)]
    sutta_ref_code: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    title: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    sutta_title_pali: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    sutta_title_trans: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Text)]
    markdown_content: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    sutta_pts: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    copyright: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    license: Option<String>,
}

/// Represents an author from the Buddha Ujja legacy database
#[derive(Debug, Clone, QueryableByName)]
#[allow(dead_code)]
struct BuddhaUjjaAuthor {
    #[diesel(sql_type = diesel::sql_types::Text)]
    author_ref_code: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    full_name: Option<String>,
}

pub struct BuddhaUjjaImporter {
    bu_db_path: PathBuf,
}

impl BuddhaUjjaImporter {
    pub fn new(bu_db_path: PathBuf) -> Self {
        Self { bu_db_path }
    }

    /// Convert Buddha Ujja reference code to standard UID format
    fn code_to_uid(&self, code: &str) -> String {
        let s = code.to_lowercase();

        if s.contains("dhp-") {
            // dhp-11-vagga
            let re = Regex::new(r"dhp-([0-9]+)-vagga").unwrap();
            if let Some(caps) = re.captures(&s) {
                let n: u32 = caps[1].parse().unwrap_or(0);
                // Convert to dhp chapter reference
                return dhp_chapter_ref_for_verse_num(n).expect("Can't find dhp chapter");
            }
        } else if s.contains("thig-") {
            // thig-5.67-121
            // Convert verse numbers to SC chapter range.
            let re = Regex::new(r"thig-[0-9]+\.([0-9]+)-.*").unwrap();
            if let Some(caps) = re.captures(&s) {
                let n: u32 = caps[1].parse().unwrap_or(0);
                return thig_verse_to_uid(n).expect("Can't find thig verse");
            }
        } else if s.starts_with("uda-") {
            // Udāna: uda-5.5 -> ud5.5
            // The rest of the suttas use the 'ud' nikaya code, not 'uda'.
            let re = Regex::new(r"^uda-([0-9\.]+)(.*)").unwrap();
            return re.replace(&s, "ud$1$2").to_string();
        } else if s == "mv-10.2.3-20" {
            // Dīghāvu Vatthu
            // pli-tv-kd10 Contains the Kosambiya Jataka about Dighavu
            return "pli-tv-kd10".to_string();
        } else if s == "mv-1.1.5-8" {
            // Mahāvagga
            // pli-tv-kd1 Contains the story with Upaka.
            return "pli-tv-kd1".to_string();
        }

        // Default:
        // sn56.11 Dhammacakka, double ref?
        //
        // thag-8.1
        // 8.1 is identical to SC number.
        //
        // an-10.13 -> an10.13
        // an-1.296-305 -> an1.296-305
        let re = Regex::new(r"^([a-z]+)-([0-9\.]+)(.*)").unwrap();
        re.replace(&s, "$1$2$3").to_string()
    }

    /// Query suttas from Buddha Ujja database
    fn get_suttas(&self, source_conn: &mut SqliteConnection) -> Result<Vec<BuddhaUjjaSutta>> {
        let query = "SELECT id, sutta_ref_code, title, sutta_title_pali, sutta_title_trans, \
                     markdown_content, sutta_pts, copyright, license \
                     FROM suttas \
                     WHERE language_code = 'hu' \
                     AND (is_published = 1 OR translator_name = 'A páli fordító csoport')";

        let suttas = diesel::sql_query(query)
            .load::<BuddhaUjjaSutta>(source_conn)
            .context("Failed to load suttas from Buddha Ujja database")?;

        Ok(suttas)
    }

    /// Get author reference code for a sutta
    fn get_author_ref(&self, source_conn: &mut SqliteConnection, sutta_id: i32) -> Result<String> {
        // Get author_id from sutta_author junction table
        let author_id_query = format!("SELECT author_id FROM sutta_author WHERE sutta_id = {}", sutta_id);
        
        #[derive(QueryableByName)]
        struct AuthorIdResult {
            #[diesel(sql_type = diesel::sql_types::Integer)]
            author_id: i32,
        }
        
        let result = diesel::sql_query(author_id_query)
            .get_result::<AuthorIdResult>(source_conn)
            .context("Failed to get author_id")?;

        // Get author_ref_code from authors table
        let author_query = format!("SELECT author_ref_code, full_name FROM authors WHERE id = {}", result.author_id);
        let author = diesel::sql_query(author_query)
            .get_result::<BuddhaUjjaAuthor>(source_conn)
            .context("Failed to get author")?;

        Ok(author.author_ref_code)
    }

    /// Convert BuddhaUjjaSutta to SuttaData
    fn convert_to_sutta_data(&self, source_conn: &mut SqliteConnection, bu_sutta: &BuddhaUjjaSutta) -> Result<SuttaData> {
        // Get author reference
        let author_ref = self.get_author_ref(source_conn, bu_sutta.id)?;

        // Convert reference code to UID
        let sutta_uid = self.code_to_uid(&bu_sutta.sutta_ref_code);
        
        // Calculate sutta_ref and nikaya from sutta_uid before creating full uid
        let sutta_ref = uid_to_ref(&sutta_uid);
        let nikaya = uid_to_nikaya(&sutta_uid);
        
        // Now create the full uid
        let uid = format!("{}/hu/{}", sutta_uid, author_ref).to_lowercase();

        // Extract title from "AN 2.31-32 Kataññu Sutta" -> "Kataññu Sutta"
        let title_re = Regex::new(r"^[a-zA-Z]+ [0-9\.-]+ (.*)").unwrap();
        let title = if let Some(caps) = title_re.captures(&bu_sutta.title) {
            caps[1].to_string()
        } else {
            bu_sutta.title.clone()
        };

        // Process markdown content
        let mut content_text = consistent_niggahita(Some(bu_sutta.markdown_content.clone()));
        // line break is two trailing spaces, not trailing backslash
        let backslash_newline_re = Regex::new(r"\\\n").unwrap();
        content_text = backslash_newline_re.replace_all(&content_text, "  \n").to_string();

        // Convert markdown to HTML
        let content_main = mdbook::utils::render_markdown(&content_text, true);

        // Add license HTML
        let license_html = if bu_sutta.license.as_deref() == Some("cc-by-nc-sa") {
            r#"
            <p>Ez a Mű a <a rel="license" href="http://creativecommons.org/licenses/by-nc-sa/4.0/">Creative Commons Nevezd meg! - Ne add el! - Így add tovább! 4.0 Nemzetközi Licenc</a> feltételeinek megfelelően felhasználható.</p>
            "#.to_string()
        } else if let Some(license) = &bu_sutta.license {
            format!("<p>License: {}</p>", license)
        } else {
            String::new()
        };

        let copyright_html = bu_sutta.copyright.as_ref()
            .map(|c| format!("<p>Copyright &copy; {}</p>", c))
            .unwrap_or_default();

        // Build full HTML content
        let content_html = format!(
            r#"
        <h1>{} {}</h1>
        {}
        <p>&nbsp;</p>
        <footer class="noindex">
            {}
            {}
        </footer>
        "#,
            sutta_ref, title, content_main, copyright_html, license_html
        );

        let content_plain = sutta_html_to_plain_text(&content_html);

        // Apply consistent niggahita to titles
        let title = consistent_niggahita(Some(title));
        let title_ascii = pali_to_ascii(Some(&title));
        let title_pali = consistent_niggahita(Some(bu_sutta.sutta_title_pali.clone()));

        // Parse range information from uid
        let (sutta_range_group, sutta_range_start, sutta_range_end) =
            SuttaData::parse_range_from_uid(&uid);

        Ok(SuttaData {
            source_uid: author_ref,
            title,
            title_ascii,
            title_pali: Some(title_pali),
            uid,
            sutta_ref,
            nikaya,
            language: "hu".to_string(),
            content_html,
            content_plain,
            sutta_range_group,
            sutta_range_start,
            sutta_range_end,
        })
    }

    fn import_suttas(&mut self, target_conn: &mut SqliteConnection) -> Result<()> {
        logger::info("Importing Hungarian translations from Buddha Ujja");

        // Check if Buddha Ujja database exists
        if !self.bu_db_path.exists() {
            anyhow::bail!("Buddha Ujja database not found: {:?}", self.bu_db_path);
        }

        // Connect to Buddha Ujja database
        let mut source_conn = SqliteConnection::establish(
            self.bu_db_path.to_str()
                .context("Invalid path")?
        ).context("Failed to connect to Buddha Ujja database")?;

        // Query suttas
        let bu_suttas = self.get_suttas(&mut source_conn)?;
        let sutta_count = bu_suttas.len();
        logger::info(&format!("Found {} published Hungarian suttas", sutta_count));

        // Create progress bar
        let pb = ProgressBar::new(sutta_count as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=>-"),
        );

        // Process each sutta
        for bu_sutta in &bu_suttas {
            let sutta_data = self.convert_to_sutta_data(&mut source_conn, bu_sutta)
                .with_context(|| format!("Failed to convert sutta: {}", bu_sutta.sutta_ref_code))?;

            // Insert into database
            diesel::insert_into(suttas::table)
                .values((
                    suttas::uid.eq(&sutta_data.uid),
                    suttas::sutta_ref.eq(&sutta_data.sutta_ref),
                    suttas::nikaya.eq(&sutta_data.nikaya),
                    suttas::language.eq(&sutta_data.language),
                    suttas::title.eq(&sutta_data.title),
                    suttas::title_ascii.eq(&sutta_data.title_ascii),
                    suttas::title_pali.eq(&sutta_data.title_pali),
                    suttas::content_html.eq(&sutta_data.content_html),
                    suttas::content_plain.eq(&sutta_data.content_plain),
                    suttas::source_uid.eq(&sutta_data.source_uid),
                ))
                .execute(target_conn)
                .with_context(|| format!("Failed to insert sutta: {}", sutta_data.uid))?;

            pb.set_message(sutta_data.uid.clone());
            pb.inc(1);
        }

        pb.finish_with_message("Done");
        logger::info(&format!("Successfully imported {} Hungarian translations from Buddha Ujja", sutta_count));

        Ok(())
    }
}

impl SuttaImporter for BuddhaUjjaImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        self.import_suttas(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_to_uid() {
        let importer = BuddhaUjjaImporter::new(PathBuf::from("test.db"));
        
        assert_eq!(importer.code_to_uid("an-10.13"), "an10.13");
        assert_eq!(importer.code_to_uid("an-1.296-305"), "an1.296-305");
        assert_eq!(importer.code_to_uid("uda-5.5"), "ud5.5");
        assert_eq!(importer.code_to_uid("mv-10.2.3-20"), "pli-tv-kd10");
        assert_eq!(importer.code_to_uid("mv-1.1.5-8"), "pli-tv-kd1");
    }

    #[test]
    fn test_title_extraction() {
        let title_re = Regex::new(r"^[a-zA-Z]+ [0-9\.-]+ (.*)").unwrap();
        
        let test_cases = vec![
            ("AN 2.31-32 Kataññu Sutta", "Kataññu Sutta"),
            ("SN 12.23 Test Sutta", "Test Sutta"),
            ("MN 1 Mūlapariyāya Sutta", "Mūlapariyāya Sutta"),
        ];

        for (input, expected) in test_cases {
            if let Some(caps) = title_re.captures(input) {
                assert_eq!(&caps[1], expected);
            }
        }
    }
}

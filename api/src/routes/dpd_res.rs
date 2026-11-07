//! `GET /dpd_res/<path..>`: the static files of DPD's pages (CSS, fonts,
//! scripts), served from the `dpd_res` table of `dpd.sqlite3`. The DPD word
//! page links each of them as `{api_url}/dpd_res/<file>`. See
//! docs/dpd-upgrades.md.

use std::path::PathBuf;
use std::sync::Arc;

use rocket::http::{ContentType, Status};
use rocket::{get, State};

use simsapa_backend::db::DbManager;
use simsapa_backend::logger::{error, warn};

use crate::helpers::pathbuf_to_forward_slash_string;

/// The response for one stored-file lookup: the stored bytes with the stored
/// mime type, `404` when the path is not stored (or the table is missing),
/// `500` on a database error.
pub fn dpd_res_response(path: &str, lookup: anyhow::Result<Option<(String, Vec<u8>)>>) -> (Status, (ContentType, Vec<u8>)) {
    match lookup {
        Ok(Some((mime, content))) => {
            let content_type = ContentType::parse_flexible(&mime).unwrap_or(ContentType::Binary);
            (Status::Ok, (content_type, content))
        }
        Ok(None) => {
            let msg = format!("404 Not Found: /dpd_res/{}", path);
            warn(&msg);
            (Status::NotFound, (ContentType::Plain, msg.into_bytes()))
        }
        Err(e) => {
            let msg = format!("500 Internal Server Error: /dpd_res/{}: {:#}", path, e);
            error(&msg);
            (Status::InternalServerError, (ContentType::Plain, msg.into_bytes()))
        }
    }
}

#[get("/dpd_res/<path..>")]
pub fn get_dpd_res(path: PathBuf, dbm: &State<Arc<DbManager>>) -> (Status, (ContentType, Vec<u8>)) {
    let path_str = pathbuf_to_forward_slash_string(&path);
    let lookup = dbm.dpd.get_dpd_res(&path_str);
    dpd_res_response(&path_str, lookup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_file_keeps_its_mime_type() {
        let (status, (ct, body)) = dpd_res_response("main.js", Ok(Some(("application/javascript".to_string(), b"x()".to_vec()))));
        assert_eq!(status, Status::Ok);
        assert_eq!(ct.to_string(), "application/javascript");
        assert_eq!(body, b"x()");

        let (_, (ct, _)) = dpd_res_response("a.css", Ok(Some(("text/css".to_string(), vec![]))));
        assert_eq!(ct.to_string(), "text/css");

        let (_, (ct, _)) = dpd_res_response("Inter.ttf", Ok(Some(("font/ttf".to_string(), vec![0, 1]))));
        assert_eq!(ct.to_string(), "font/ttf");
    }

    #[test]
    fn absent_path_or_table_is_404() {
        let (status, (ct, body)) = dpd_res_response("nope.js", Ok(None));
        assert_eq!(status, Status::NotFound);
        assert_eq!(ct, ContentType::Plain);
        assert!(String::from_utf8(body).unwrap().contains("/dpd_res/nope.js"));
    }

    #[test]
    fn database_error_is_500() {
        let (status, _) = dpd_res_response("main.js", Err(anyhow::anyhow!("disk I/O error")));
        assert_eq!(status, Status::InternalServerError);
    }
}

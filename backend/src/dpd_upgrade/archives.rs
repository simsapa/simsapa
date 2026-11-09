//! Getting a DPD release onto the disk: downloading the two assets, unpacking
//! `dpd.db` from `dpd.db.tar.xz` and the `dpd/` folder from
//! `dpd-goldendict.zip`, or taking the user's local files.
//!
//! Everything is written into the staging folder in 1 MB chunks with the
//! cancel flag checked between chunks. A user's local file is never modified,
//! moved or deleted: an archive is read in place, an extracted `dpd.db` is
//! copied (the migration rewrites and renames its input), and an extracted
//! `dpd/` folder is read in place.

use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::releases::{ReleaseCandidate, DPD_DB_ASSET, GOLDENDICT_ASSET};
use super::{DpdSource, LocalDir, LocalFile, SourcePart, Step, UpgradeError};
use crate::import_staging::{copy_stream_to_file, human_bytes, CHUNK_BYTES};
use crate::logger::{error, info};

/// The `dpd/` folder's share of `dpd-goldendict.zip`'s size, for estimating the
/// unpacked folder before the zip is downloaded (154.5 MB of 271.2 MB in
/// v0.4.20261026; the zip also holds the other DPD dictionaries).
const DPD_DIR_SHARE_OF_ZIP: f64 = 0.6;

/// The member of `dpd-goldendict.zip` that is imported.
pub const DPD_ZIP_MEMBER: &str = "dpd";

/// The file inside `dpd.db.tar.xz`.
pub const DPD_DB_FILE: &str = "dpd.db";

/// Connection attempts per download, waiting 2, 4, 8, 16 s between them, as
/// the database downloads do.
const MAX_ATTEMPTS: u32 = 5;

/// The estimated size of the unpacked `dpd/` folder of a zip of `zip_bytes`.
pub fn estimated_dpd_dir_bytes(zip_bytes: u64) -> u64 {
    (zip_bytes as f64 * DPD_DIR_SHARE_OF_ZIP) as u64
}

/// The unpacked size of the `dpd/` member of a local zip, from its central
/// directory.
pub fn zip_dpd_dir_bytes(zip_path: &Path) -> anyhow::Result<u64> {
    let file = File::open(zip_path)?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file))?;
    let mut total = 0u64;
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i)?;
        if is_dpd_member(entry.name()) && !entry.is_dir() {
            total += entry.size();
        }
    }
    Ok(total)
}

fn is_dpd_member(name: &str) -> bool {
    name.strip_prefix(DPD_ZIP_MEMBER).is_some_and(|rest| rest.starts_with('/'))
}

/// The two downloaded archives, in the staging folder.
#[derive(Debug, Clone)]
pub struct Downloads {
    pub dpd_db_archive: PathBuf,
    pub goldendict_zip: PathBuf,
}

/// The release's files, ready for the import.
#[derive(Debug, Clone)]
pub struct StagedInputs {
    /// A `dpd.db` in the staging folder, which the migration may rewrite.
    pub dpd_db: PathBuf,
    /// The `dpd/` folder holding `dpd.ifo` and `res/`. In the staging folder,
    /// or the user's own folder (read only).
    pub dpd_dir: PathBuf,
}

/// Download both assets of `candidate` into `staging`. `progress` gets the
/// asset name and the bytes done and expected.
pub fn download_release(
    candidate: &ReleaseCandidate,
    staging: &Path,
    cancel: &AtomicBool,
    progress: &dyn Fn(&str, u64, u64),
) -> Result<Downloads, UpgradeError> {
    let dpd_db_archive = staging.join(DPD_DB_ASSET);
    let goldendict_zip = staging.join(GOLDENDICT_ASSET);
    download_one(&candidate.dpd_db.url, &dpd_db_archive, candidate.dpd_db.size, cancel, progress)?;
    download_one(&candidate.goldendict.url, &goldendict_zip, candidate.goldendict.size, cancel, progress)?;
    Ok(Downloads { dpd_db_archive, goldendict_zip })
}

fn download_one(
    url: &str,
    dest: &Path,
    expected: u64,
    cancel: &AtomicBool,
    progress: &dyn Fn(&str, u64, u64),
) -> Result<(), UpgradeError> {
    let name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let fail = |m: String| UpgradeError::failed(Step::Download, m);

    // Bound the connection phase; no overall timeout, so a slow line can
    // finish a 270 MB body.
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("simsapa-app/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| fail(format!("Could not create the HTTP client: {}", e)))?;

    let mut attempt = 0;
    let mut response = loop {
        attempt += 1;
        if cancel.load(Ordering::Relaxed) {
            return Err(UpgradeError::cancelled(Step::Download));
        }
        match client.get(url).send() {
            Ok(r) => break r,
            Err(e) if attempt < MAX_ATTEMPTS => {
                let wait = 2u64.pow(attempt);
                error(&format!("dpd_upgrade: download of {} failed (attempt {}/{}): {}; retrying in {} s",
                    name, attempt, MAX_ATTEMPTS, e, wait));
                for _ in 0..wait * 10 {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(UpgradeError::cancelled(Step::Download));
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            Err(e) => {
                return Err(fail(format!(
                    "Could not download {} after {} attempts: {}. Check the internet connection and try again.",
                    name, MAX_ATTEMPTS, e
                )));
            }
        }
    };

    let status = response.status();
    if !status.is_success() {
        return Err(fail(format!("The download server returned HTTP {} for {} ({}).", status.as_u16(), name, url)));
    }

    let declared = response.content_length();
    let total = declared.unwrap_or(expected);
    let bytes = copy_stream_to_file(&mut response, dest, Some(total), cancel, &mut |done, total| {
        progress(&name, done, total)
    })
    .map_err(|e| {
        if e.code == "cancelled" {
            UpgradeError::cancelled(Step::Download)
        } else {
            fail(format!("Downloading {} failed: {}", name, e.message))
        }
    })?;

    if let Some(d) = declared {
        if bytes < d {
            let _ = std::fs::remove_file(dest);
            return Err(fail(format!("The download of {} ended early: {} of {} bytes.", name, bytes, d)));
        }
    }
    if bytes == 0 {
        let _ = std::fs::remove_file(dest);
        return Err(fail(format!("The download of {} was empty.", name)));
    }
    info(&format!("dpd_upgrade: downloaded {} ({})", name, human_bytes(bytes)));
    Ok(())
}

/// Put the release's `dpd.db` and `dpd/` folder in place for the import.
/// Downloaded archives are deleted once unpacked; the user's never are.
pub fn stage_inputs(
    source: &DpdSource,
    downloads: Option<&Downloads>,
    staging: &Path,
    cancel: &AtomicBool,
    progress: &dyn Fn(&str, u64, u64),
) -> Result<StagedInputs, UpgradeError> {
    let staged_db = staging.join(DPD_DB_FILE);
    let unzip_root = staging.join("goldendict");

    let (dpd_db, goldendict, owned) = match (source, downloads) {
        (DpdSource::Release(_), Some(d)) => (
            LocalFile::Archive(d.dpd_db_archive.clone()),
            LocalDir::Zip(d.goldendict_zip.clone()),
            true,
        ),
        (DpdSource::Local { dpd_db, goldendict }, _) => (dpd_db.clone(), goldendict.clone(), false),
        (DpdSource::Release(_), None) => {
            return Err(UpgradeError::failed(Step::Extract, "The release was not downloaded."));
        }
    };

    let db_err = |m: String| UpgradeError::failed(Step::Extract, m).with_part(SourcePart::DpdDb);
    match &dpd_db {
        LocalFile::Archive(p) => {
            require_file(p).map_err(|m| UpgradeError::new(Step::Extract, "missing_input", m).with_part(SourcePart::DpdDb))?;
            extract_dpd_db(p, &staged_db, cancel, &|done, total| progress("Unpacking the word data", done, total))
                .map_err(|e| if e.is_cancelled() { e } else { db_err(e.message) })?;
            if owned {
                let _ = std::fs::remove_file(p);
            }
        }
        LocalFile::Extracted(p) => {
            require_file(p).map_err(|m| UpgradeError::new(Step::Extract, "missing_input", m).with_part(SourcePart::DpdDb))?;
            copy_file(p, &staged_db, cancel, &|done, total| progress("Copying the word data", done, total))?;
        }
    }

    let dpd_dir = match &goldendict {
        LocalDir::Zip(p) => {
            require_file(p).map_err(|m| UpgradeError::new(Step::Extract, "missing_input", m).with_part(SourcePart::Goldendict))?;
            let dir = extract_dpd_dir(p, &unzip_root, cancel, &|done, total| progress("Unpacking the dictionary pages", done, total))
                .map_err(|e| if e.is_cancelled() { e } else { e.with_part(SourcePart::Goldendict) })?;
            if owned {
                let _ = std::fs::remove_file(p);
            }
            dir
        }
        LocalDir::Extracted(p) => p.clone(),
    };

    Ok(StagedInputs { dpd_db: staged_db, dpd_dir })
}

/// Check the user's own files before anything is staged: each file exists,
/// and an extracted `dpd/` folder has `dpd.ifo` and a `res/` folder with
/// files. An archive's contents are checked once unpacked. The error names the
/// input at fault. Nothing to check for a release.
pub fn check_local_inputs(source: &DpdSource) -> Result<(), UpgradeError> {
    let DpdSource::Local { dpd_db, goldendict } = source else {
        return Ok(());
    };
    let missing = |m: String, part: SourcePart| UpgradeError::new(Step::Check, "missing_input", m).with_part(part);
    match dpd_db {
        LocalFile::Archive(p) | LocalFile::Extracted(p) => require_file(p).map_err(|m| missing(m, SourcePart::DpdDb))?,
    }
    match goldendict {
        LocalDir::Zip(p) => require_file(p).map_err(|m| missing(m, SourcePart::Goldendict))?,
        LocalDir::Extracted(p) => crate::dpd_upgrade::staged::check_dpd_dir(p).map_err(|m| missing(m, SourcePart::Goldendict))?,
    }
    Ok(())
}

fn require_file(path: &Path) -> Result<(), String> {
    match path.try_exists() {
        Ok(true) if path.is_file() => Ok(()),
        Ok(true) => Err(format!("{} is not a file.", path.display())),
        Ok(false) => Err(format!("{} does not exist.", path.display())),
        Err(e) => Err(format!("Cannot access {}: {}", path.display(), e)),
    }
}

/// Copy a file in chunks, with progress and cancel.
fn copy_file(from: &Path, to: &Path, cancel: &AtomicBool, progress: &dyn Fn(u64, u64)) -> Result<(), UpgradeError> {
    let total = std::fs::metadata(from).map(|m| m.len()).unwrap_or(0);
    let mut src = File::open(from)
        .map_err(|e| UpgradeError::failed(Step::Extract, format!("Cannot open {}: {}", from.display(), e)).with_part(SourcePart::DpdDb))?;
    copy_stream_to_file(&mut src, to, Some(total), cancel, &mut |done, total| progress(done, total)).map_err(|e| {
        if e.code == "cancelled" {
            UpgradeError::cancelled(Step::Extract)
        } else {
            UpgradeError::failed(Step::Extract, format!("Copying {} failed: {}", from.display(), e.message)).with_part(SourcePart::DpdDb)
        }
    })?;
    Ok(())
}

/// Unpack the `dpd.db` entry of `dpd.db.tar.xz` to `dest`. The xz stream may
/// hold several blocks; the decoder checks each block's CRC64 when it reaches
/// the block's end, so the stream is read to its end after the entry: the
/// last block (the tail of `dpd.db` and the tar trailer) and the stream's
/// index and footer are checked too. On a failed check `dest` is removed.
pub fn extract_dpd_db(archive: &Path, dest: &Path, cancel: &AtomicBool, progress: &dyn Fn(u64, u64)) -> Result<(), UpgradeError> {
    let fail = |m: String| UpgradeError::failed(Step::Extract, m);
    let file = File::open(archive).map_err(|e| fail(format!("Cannot open {}: {}", archive.display(), e)))?;
    let xz = lzma_rust2::XzReader::new(BufReader::with_capacity(CHUNK_BYTES, file), true);
    let mut tar = tar::Archive::new(xz);

    let mut unpacked: Option<u64> = None;
    {
        let entries = tar.entries().map_err(|e| fail(format!("{} is not a tar archive: {}", archive.display(), e)))?;
        for entry in entries {
            let mut entry = entry.map_err(|e| fail(format!("Reading {} failed: {}", archive.display(), e)))?;
            let is_dpd_db = entry.header().entry_type().is_file()
                && entry.path().ok().and_then(|p| p.file_name().map(|n| n == DPD_DB_FILE)).unwrap_or(false);
            if !is_dpd_db {
                continue;
            }
            let total = entry.header().size().unwrap_or(0);
            copy_stream_to_file(&mut entry, dest, Some(total), cancel, &mut |done, total| progress(done, total)).map_err(|e| {
                if e.code == "cancelled" {
                    UpgradeError::cancelled(Step::Extract)
                } else {
                    fail(format!("Unpacking {} from {} failed: {}", DPD_DB_FILE, archive.display(), e.message))
                }
            })?;
            unpacked = Some(total);
            break;
        }
    }
    let Some(total) = unpacked else {
        return Err(fail(format!("{} holds no {}.", archive.display(), DPD_DB_FILE)));
    };

    if let Err(e) = std::io::copy(&mut tar.into_inner(), &mut std::io::sink()) {
        let _ = std::fs::remove_file(dest);
        return Err(fail(format!(
            "{} is damaged or incomplete (the end of the archive failed its check: {}). Download it again.",
            archive.display(),
            e
        )));
    }
    info(&format!("dpd_upgrade: unpacked {} ({})", dest.display(), human_bytes(total)));
    Ok(())
}

/// Unpack only the `dpd/` member of `dpd-goldendict.zip` under `dest_root`, and
/// return the unpacked `dpd/` folder. The zip's other folders are not read.
pub fn extract_dpd_dir(zip_path: &Path, dest_root: &Path, cancel: &AtomicBool, progress: &dyn Fn(u64, u64)) -> Result<PathBuf, UpgradeError> {
    let fail = |m: String| UpgradeError::failed(Step::Extract, m);
    let file = File::open(zip_path).map_err(|e| fail(format!("Cannot open {}: {}", zip_path.display(), e)))?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file))
        .map_err(|e| fail(format!("{} is not a zip archive: {}", zip_path.display(), e)))?;

    let mut wanted: Vec<usize> = Vec::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i).map_err(|e| fail(format!("Reading {} failed: {}", zip_path.display(), e)))?;
        if is_dpd_member(entry.name()) {
            total += entry.size();
            wanted.push(i);
        }
    }
    if wanted.is_empty() {
        return Err(fail(format!("{} holds no {}/ folder.", zip_path.display(), DPD_ZIP_MEMBER)));
    }

    let mut done = 0u64;
    let mut buf = vec![0u8; CHUNK_BYTES];
    for i in wanted {
        let mut entry = zip.by_index(i).map_err(|e| fail(format!("Reading {} failed: {}", zip_path.display(), e)))?;
        let Some(rel) = entry.enclosed_name() else {
            return Err(fail(format!("{} has an entry with an unsafe path: {}", zip_path.display(), entry.name())));
        };
        let out_path = dest_root.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| fail(format!("Cannot create {}: {}", out_path.display(), e)))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| fail(format!("Cannot create {}: {}", parent.display(), e)))?;
        }
        let mut out = File::create(&out_path).map_err(|e| fail(format!("Cannot write {}: {}", out_path.display(), e)))?;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(UpgradeError::cancelled(Step::Extract));
            }
            let n = entry.read(&mut buf).map_err(|e| fail(format!("Unpacking {} failed: {}", out_path.display(), e)))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(|e| fail(format!("Writing {} failed: {}", out_path.display(), e)))?;
            done += n as u64;
            progress(done, total);
        }
    }

    let dpd_dir = dest_root.join(DPD_ZIP_MEMBER);
    info(&format!("dpd_upgrade: unpacked {} ({})", dpd_dir.display(), human_bytes(done)));
    Ok(dpd_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
        let f = File::create(path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in files {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }

    #[test]
    fn only_the_dpd_member_of_the_zip_is_unpacked() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("dpd-goldendict.zip");
        write_zip(&zip_path, &[
            ("dpd/dpd.ifo", b"ifo"),
            ("dpd/res/main.js", b"js"),
            ("dpd-grammar/dpd-grammar.ifo", b"other"),
            ("dpdx/a.txt", b"not dpd"),
        ]);
        assert_eq!(zip_dpd_dir_bytes(&zip_path).unwrap(), 5);

        let cancel = AtomicBool::new(false);
        let dir = extract_dpd_dir(&zip_path, &tmp.path().join("out"), &cancel, &|_, _| {}).unwrap();
        assert_eq!(std::fs::read(dir.join("dpd.ifo")).unwrap(), b"ifo");
        assert_eq!(std::fs::read(dir.join("res/main.js")).unwrap(), b"js");
        assert!(!tmp.path().join("out/dpd-grammar").try_exists().unwrap());
        assert!(!tmp.path().join("out/dpdx").try_exists().unwrap());
        // The user's zip is untouched.
        assert!(zip_path.try_exists().unwrap());
    }

    #[test]
    fn a_zip_without_dpd_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("x.zip");
        write_zip(&zip_path, &[("other/a.ifo", b"x")]);
        let cancel = AtomicBool::new(false);
        let e = extract_dpd_dir(&zip_path, &tmp.path().join("out"), &cancel, &|_, _| {}).unwrap_err();
        assert!(e.message.contains("no dpd/ folder"), "{}", e.message);
    }

    #[test]
    fn a_tar_xz_archive_gives_its_dpd_db() {
        let tmp = tempfile::tempdir().unwrap();
        let tar_path = tmp.path().join("dpd.db.tar");
        {
            let mut b = tar::Builder::new(File::create(&tar_path).unwrap());
            let data = b"SQLite format 3\0 fixture";
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_cksum();
            b.append_data(&mut h, "dpd.db", &data[..]).unwrap();
            b.finish().unwrap();
        }
        let xz_path = tmp.path().join("dpd.db.tar.xz");
        {
            let mut w = lzma_rust2::XzWriter::new(File::create(&xz_path).unwrap(), lzma_rust2::XzOptions::default()).unwrap();
            w.write_all(&std::fs::read(&tar_path).unwrap()).unwrap();
            w.finish().unwrap();
        }
        let cancel = AtomicBool::new(false);
        let dest = tmp.path().join("out.db");
        extract_dpd_db(&xz_path, &dest, &cancel, &|_, _| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"SQLite format 3\0 fixture");

        // The entry decodes in full, but the stream's footer is missing: only
        // reading the stream to its end finds it.
        let bytes = std::fs::read(&xz_path).unwrap();
        let cut_path = tmp.path().join("cut.tar.xz");
        std::fs::write(&cut_path, &bytes[..bytes.len() - 12]).unwrap();
        let cut_dest = tmp.path().join("cut.db");
        let e = extract_dpd_db(&cut_path, &cut_dest, &cancel, &|_, _| {}).unwrap_err();
        assert!(!e.is_cancelled() && e.message.contains("damaged or incomplete"), "{}", e.message);
        assert!(!cut_dest.try_exists().unwrap(), "a failed check leaves no file");

        cancel.store(true, Ordering::Relaxed);
        let e = extract_dpd_db(&xz_path, &tmp.path().join("out2.db"), &cancel, &|_, _| {}).unwrap_err();
        assert!(e.is_cancelled());
        assert!(!tmp.path().join("out2.db").try_exists().unwrap(), "a cancelled unpack leaves no file");
    }
}

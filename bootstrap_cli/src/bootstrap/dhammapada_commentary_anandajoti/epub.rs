//! Reading the epub: its text files, the spine order from the OPF, and the
//! outline from `toc.ncx`.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use sha2::{Digest, Sha256};

/// The text files of an epub, keyed by their path relative to the folder of
/// the OPF file (`007.html`, `toc.ncx`), as the OPF and the links name them.
pub struct EpubFiles {
    /// The epub's file name, recorded in the manifest.
    pub file_name: String,
    /// SHA-256 of the epub file, lower-case hex.
    pub sha256: String,
    /// The OPF path relative to the OPF folder (its file name).
    pub opf_name: String,
    pub files: BTreeMap<String, String>,
}

const TEXT_EXTENSIONS: [&str; 5] = ["html", "xhtml", "htm", "ncx", "opf"];

fn is_text_file(path: &str) -> bool {
    path.rsplit_once('.')
        .map(|(_, ext)| TEXT_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// The OPF path from `META-INF/container.xml`, e.g. `Ops/content.opf`.
fn rootfile_path(container_xml: &str) -> Result<String> {
    let mut reader = Reader::from_str(container_xml);
    reader.trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == b"rootfile" => {
                for attr in e.attributes() {
                    let attr = attr.context("Malformed attribute in container.xml")?;
                    if attr.key.local_name().as_ref() == b"full-path" {
                        return Ok(attr.unescape_value()?.to_string());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(anyhow!("container.xml: XML error: {}", e)),
        }
    }
    Err(anyhow!("container.xml names no rootfile"))
}

/// Keep the text files under the OPF's folder, keyed relative to it.
fn files_under_opf(entries: BTreeMap<String, String>, file_name: String, sha256: String) -> Result<EpubFiles> {
    let container = entries
        .get("META-INF/container.xml")
        .ok_or_else(|| anyhow!("{}: no META-INF/container.xml", file_name))?;
    let opf_path = rootfile_path(container)?;
    let (base, opf_name) = match opf_path.rsplit_once('/') {
        Some((base, name)) => (format!("{}/", base), name.to_string()),
        None => (String::new(), opf_path.clone()),
    };
    let files: BTreeMap<String, String> = entries
        .into_iter()
        .filter_map(|(path, text)| path.strip_prefix(&base).map(|rel| (rel.to_string(), text)))
        .collect();
    if !files.contains_key(&opf_name) {
        return Err(anyhow!("{}: the OPF {} is missing", file_name, opf_path));
    }
    Ok(EpubFiles { file_name, sha256, opf_name, files })
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{:02x}", b)).collect()
}

impl EpubFiles {
    /// Read an `.epub` file.
    pub fn from_zip(epub_path: &Path) -> Result<Self> {
        let bytes = std::fs::read(epub_path).with_context(|| format!("Cannot read {}", epub_path.display()))?;
        let file_name = epub_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let sha256 = sha256_hex(&bytes);

        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .with_context(|| format!("{} is not a zip file", epub_path.display()))?;
        let mut entries = BTreeMap::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_string();
            if !entry.is_file() || !(is_text_file(&name) || name == "META-INF/container.xml") {
                continue;
            }
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .with_context(|| format!("{}: {} is not UTF-8", file_name, name))?;
            entries.insert(name, text);
        }
        files_under_opf(entries, file_name, sha256)
    }

    /// Read an unpacked epub folder (the test fixture). The folder's name
    /// stands for the epub's file name, and the "SHA-256" is that of the
    /// sorted `path NUL contents NUL` sequence of its files, so that it is
    /// stable and changes with any file.
    pub fn from_dir(dir: &Path) -> Result<Self> {
        let file_name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let mut entries = BTreeMap::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(folder) = stack.pop() {
            for entry in std::fs::read_dir(&folder).with_context(|| format!("Cannot read {}", folder.display()))? {
                let path = entry?.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let rel = path
                    .strip_prefix(dir)
                    .map_err(|_| anyhow!("{} is outside {}", path.display(), dir.display()))?
                    .to_string_lossy()
                    .replace('\\', "/");
                if is_text_file(&rel) || rel == "META-INF/container.xml" {
                    let text = std::fs::read_to_string(&path).with_context(|| format!("Cannot read {}", path.display()))?;
                    entries.insert(rel, text);
                }
            }
        }
        let mut hasher = Sha256::new();
        for (path, text) in &entries {
            hasher.update(path.as_bytes());
            hasher.update([0]);
            hasher.update(text.as_bytes());
            hasher.update([0]);
        }
        let sha256 = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect();
        files_under_opf(entries, file_name, sha256)
    }

    pub fn get(&self, name: &str) -> Result<&str> {
        self.files
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| anyhow!("{}: {} is missing", self.file_name, name))
    }

    /// The spine's files in reading order (`linear="no"` items included).
    pub fn spine(&self) -> Result<Vec<String>> {
        let opf = self.get(&self.opf_name)?;
        let mut reader = Reader::from_str(opf);
        reader.trim_text(true);
        let mut hrefs: BTreeMap<String, String> = BTreeMap::new();
        let mut order: Vec<String> = Vec::new();
        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                    let name = e.local_name().as_ref().to_vec();
                    let attr = |key: &[u8]| -> Result<Option<String>> {
                        for a in e.attributes() {
                            let a = a.context("Malformed OPF attribute")?;
                            if a.key.local_name().as_ref() == key {
                                return Ok(Some(a.unescape_value()?.to_string()));
                            }
                        }
                        Ok(None)
                    };
                    match name.as_slice() {
                        b"item" => {
                            if let (Some(id), Some(href)) = (attr(b"id")?, attr(b"href")?) {
                                hrefs.insert(id, href);
                            }
                        }
                        b"itemref" => {
                            if let Some(idref) = attr(b"idref")? {
                                order.push(idref);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(e) => return Err(anyhow!("{}: XML error: {}", self.opf_name, e)),
            }
        }
        order
            .into_iter()
            .map(|idref| {
                hrefs
                    .get(&idref)
                    .cloned()
                    .ok_or_else(|| anyhow!("{}: spine item {} is not in the manifest", self.opf_name, idref))
            })
            .collect()
    }

    /// The epub's NCX file (the first `.ncx` file), or `toc.ncx`.
    pub fn ncx_name(&self) -> String {
        self.files
            .keys()
            .find(|name| name.ends_with(".ncx"))
            .cloned()
            .unwrap_or_else(|| "toc.ncx".to_string())
    }

    /// The outline from the NCX, flattened in document order.
    pub fn toc(&self) -> Result<Vec<TocEntry>> {
        let ncx_name = self.ncx_name();
        parse_ncx(self.get(&ncx_name)?).with_context(|| format!("{}: {}", self.file_name, ncx_name))
    }
}

/// One `navPoint` of the NCX.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    /// 1 for a top-level entry.
    pub depth: usize,
    pub label: String,
    /// The target file, `007.html`.
    pub file: String,
    /// The target anchor without `#`, if any.
    pub fragment: Option<String>,
}

fn parse_ncx(text: &str) -> Result<Vec<TocEntry>> {
    let mut reader = Reader::from_str(text);
    reader.trim_text(true);
    let mut entries: Vec<TocEntry> = Vec::new();
    // Open navPoints: the index of their entry in `entries`.
    let mut open: Vec<usize> = Vec::new();
    let mut in_label_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.local_name().as_ref() != b"content" => match e.local_name().as_ref() {
                b"navPoint" => {
                    entries.push(TocEntry { depth: open.len() + 1, label: String::new(), file: String::new(), fragment: None });
                    open.push(entries.len() - 1);
                }
                b"text" => in_label_text = !open.is_empty(),
                _ => {}
            },
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) if e.local_name().as_ref() == b"content" => {
                if let Some(&i) = open.last() {
                    for a in e.attributes() {
                        let a = a.context("Malformed NCX attribute")?;
                        if a.key.local_name().as_ref() == b"src" {
                            let src = a.unescape_value()?.to_string();
                            let (file, fragment) = match src.split_once('#') {
                                Some((file, frag)) => (file.to_string(), Some(frag.to_string())),
                                None => (src, None),
                            };
                            entries[i].file = file;
                            entries[i].fragment = fragment;
                        }
                    }
                }
            }
            Ok(Event::Text(t)) if in_label_text => {
                if let Some(&i) = open.last() {
                    entries[i].label.push_str(&t.unescape()?);
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"navPoint" => {
                    open.pop();
                }
                b"text" => in_label_text = false,
                _ => {}
            },
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(anyhow!("XML error at {}: {}", reader.buffer_position(), e)),
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ncx_is_flattened_with_depths_and_targets() {
        let ncx = r#"<?xml version="1.0"?><ncx><navMap>
<navPoint id="n1"><navLabel><text>Intro</text></navLabel><content src="003.html"/></navPoint>
<navPoint id="n2"><navLabel><text>1. The Chapter about the Pairs, Yamakavagga</text></navLabel><content src="007.html"/>
<navPoint id="n3"><navLabel><text>1.1 The Story &amp; More</text></navLabel><content src="007.html#ncx61"/></navPoint>
</navPoint></navMap></ncx>"#;
        let toc = parse_ncx(ncx).unwrap();
        assert_eq!(toc.len(), 3);
        assert_eq!(toc[1], TocEntry { depth: 1, label: "1. The Chapter about the Pairs, Yamakavagga".into(), file: "007.html".into(), fragment: None });
        assert_eq!(toc[2], TocEntry { depth: 2, label: "1.1 The Story & More".into(), file: "007.html".into(), fragment: Some("ncx61".into()) });
    }
}

use std::thread;
use std::fs::{File, create_dir_all, remove_file, remove_dir_all};
use std::io::{self, Read, Write, BufReader};
use std::path::Path;
use std::error::Error;
use std::time::Duration;

use core::pin::Pin;
use cxx_qt_lib::{QString, QStringList, QUrl};
use cxx_qt::{CxxQtThread, Threading};
use reqwest::blocking::Client;
use bzip2::read::BzDecoder;
use tar::Archive;

use simsapa_backend::{move_folder_contents, AppGlobalPaths};
use simsapa_backend::asset_helpers::import_suttas_lang_to_appdata;
use simsapa_backend::logger::{info, error};
use simsapa_backend::app_settings::LANGUAGES_JSON;

#[cxx_qt::bridge]
pub mod qobject {

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;

        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;

        include!("screen.h");
        fn keep_screen_on(on: bool);

        include!("android_helpers.h");
        fn open_android_display_settings();
        fn check_microphone_permission_impl() -> QString;
        fn request_microphone_permission_impl();
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[namespace = "asset_manager"]
        type AssetManager = super::AssetManagerRust;
    }

    impl cxx_qt::Threading for AssetManager{}

    extern "RustQt" {
        #[qinvokable]
        fn download_urls_and_extract(self: Pin<&mut AssetManager>, urls: QStringList, is_initial_setup: bool);

        #[qinvokable]
        fn get_available_languages(self: Pin<&mut AssetManager>) -> QStringList;

        #[qinvokable]
        fn get_init_languages(self: Pin<&mut AssetManager>) -> QString;

        #[qinvokable]
        fn should_auto_start_download(self: Pin<&mut AssetManager>) -> bool;

        #[qinvokable]
        fn peek_auto_start_download(self: Pin<&mut AssetManager>) -> bool;

        #[qinvokable]
        fn set_keep_screen_on(self: Pin<&mut AssetManager>, on: bool);

        // NOTE: currently unused — the "Open Settings" button on the large-download
        // warning screen was removed once keep-screen-on made the screen-timeout
        // workaround unnecessary. Kept in case it is wired up again.
        #[qinvokable]
        fn open_display_settings(self: Pin<&mut AssetManager>);

        #[qinvokable]
        fn check_microphone_permission(self: &AssetManager) -> QString;

        #[qinvokable]
        fn request_microphone_permission(self: &AssetManager);

        #[qinvokable]
        fn remove_sutta_languages(self: Pin<&mut AssetManager>, language_codes: QStringList);

        #[qsignal]
        #[cxx_name = "downloadProgressChanged"]
        fn download_progress_changed(self: Pin<&mut AssetManager>,
                                     op_msg: QString,
                                     downloaded_bytes: usize,
                                     total_bytes: usize);

        #[qsignal]
        #[cxx_name = "downloadShowMsg"]
        fn download_show_msg(self: Pin<&mut AssetManager>, message: QString);

        #[qsignal]
        #[cxx_name = "downloadsCompleted"]
        fn downloads_completed(self: Pin<&mut AssetManager>, value: bool);

        #[qsignal]
        #[cxx_name = "downloadNeedsRetry"]
        fn download_needs_retry(self: Pin<&mut AssetManager>, failed_url: QString, error_message: QString);

        #[qsignal]
        #[cxx_name = "removalShowMsg"]
        fn removal_show_msg(self: Pin<&mut AssetManager>, message: QString);

        #[qsignal]
        #[cxx_name = "removalProgressChanged"]
        fn removal_progress_changed(self: Pin<&mut AssetManager>,
                                    current_index: usize,
                                    total_count: usize,
                                    language_name: QString);

        #[qsignal]
        #[cxx_name = "removalCompleted"]
        fn removal_completed(self: Pin<&mut AssetManager>, success: bool, error_msg: QString);
    }
}

#[derive(Default, Copy, Clone)]
pub struct AssetManagerRust;

/// Cleanup download and extract folders when download process fails
/// If is_initial_setup is true, also removes app_assets_folder to ensure clean state
fn cleanup_on_failure(download_temp_folder: &Path, extract_temp_folder: &Path, app_assets_folder: &Path, is_initial_setup: bool, qt_thread: &CxxQtThread<qobject::AssetManager>) {
    info(&format!("Cleaning up due to download failure (initial_setup: {})", is_initial_setup));
    let cleanup_msg = QString::from("Removing partially downloaded files due to network error...");
    qt_thread.queue(move |mut qo| {
        qo.as_mut().download_show_msg(cleanup_msg);
    }).unwrap();

    // Remove download temp folder
    if download_temp_folder.exists() {
        if let Err(e) = remove_dir_all(download_temp_folder) {
            error(&format!("Failed to remove download temp folder: {}", e));
        } else {
            info("Removed download temp folder");
        }
    }

    // Remove extract temp folder
    if extract_temp_folder.exists() {
        if let Err(e) = remove_dir_all(extract_temp_folder) {
            error(&format!("Failed to remove extract temp folder: {}", e));
        } else {
            info("Removed extract temp folder");
        }
    }

    // If this is initial setup, also remove app_assets_folder to ensure clean state
    // This prevents the app from launching with incomplete databases
    if is_initial_setup {
        info("Initial setup detected - removing app_assets_folder for clean state");
        let complete_cleanup_msg = QString::from("Removing incomplete initial setup...");
        qt_thread.queue(move |mut qo| {
            qo.as_mut().download_show_msg(complete_cleanup_msg);
        }).unwrap();

        if app_assets_folder.exists() {
            if let Err(e) = remove_dir_all(app_assets_folder) {
                error(&format!("Failed to remove app_assets folder: {}", e));
            } else {
                info("Removed app_assets folder - app will restart download on next launch");
            }
        }
    }
}

impl qobject::AssetManager {
    fn set_keep_screen_on(self: Pin<&mut Self>, on: bool) {
        qobject::keep_screen_on(on);
    }

    fn open_display_settings(self: Pin<&mut Self>) {
        qobject::open_android_display_settings();
    }

    fn check_microphone_permission(&self) -> QString {
        qobject::check_microphone_permission_impl()
    }

    fn request_microphone_permission(&self) {
        qobject::request_microphone_permission_impl();
    }

    /// Get list of available language codes that can be downloaded
    /// Returns format: "code1|Name1|Count1,code2|Name2|Count2,..."
    fn get_available_languages(self: Pin<&mut Self>) -> QStringList {
        use serde_json::Value;

        let mut langs = QStringList::default();

        // Parse languages.json to get language info with counts
        match serde_json::from_str::<Vec<Value>>(LANGUAGES_JSON) {
            Ok(language_list) => {
                let mut lang_strings: Vec<String> = language_list.iter()
                    .filter_map(|lang_obj| {
                        let code = lang_obj.get("code")?.as_str()?;
                        let name = lang_obj.get("name")?.as_str()?;
                        let count = lang_obj.get("sutta_count")?.as_u64()?;

                        // Filter out base languages (en, pli, san) which are always included
                        if ["en", "pli", "san"].contains(&code) {
                            return None;
                        }

                        Some(format!("{}|{}|{}", code, name, count))
                    })
                    .collect();

                lang_strings.sort();

                for lang in lang_strings {
                    langs.append(QString::from(&lang));
                }
            }
            Err(e) => {
                error(&format!("Failed to parse languages.json: {}", e));
            }
        }

        langs
    }

    /// Read download_languages.txt if it exists in app_assets_dir
    /// Returns comma-separated language codes (e.g. "hu, pt, it")
    ///
    /// The file is created in app_assets_dir by export_user_data_to_assets(),
    /// same location as auto_start_download.txt and delete_files_for_upgrade.txt.
    fn get_init_languages(self: Pin<&mut Self>) -> QString {
        let paths = AppGlobalPaths::new();
        let download_languages_path = &paths.download_languages_marker;

        if download_languages_path.exists()
            && let Ok(contents) = std::fs::read_to_string(download_languages_path) {
                info(&format!("Read download_languages.txt: {}", contents.trim()));
                // Remove the file after reading
                let _ = std::fs::remove_file(download_languages_path);
                return QString::from(contents.trim());
            }

        QString::from("")
    }

    /// Check if auto_start_download.txt marker file exists.
    ///
    /// This is used during database upgrades to automatically start the download
    /// without user interaction. The marker file is created by prepare_for_database_upgrade().
    ///
    /// Returns true if the file exists (and removes it), false otherwise.
    ///
    /// This is the **single point of deletion** for the marker; its one caller
    /// is `DownloadAppdataWindow.qml`'s `Component.onCompleted`. Anything that
    /// merely needs to know whether an upgrade download is pending must use the
    /// non-consuming `peek_auto_start_download()` instead — consuming it early
    /// turns an unattended upgrade into a stalled setup screen.
    fn should_auto_start_download(self: Pin<&mut Self>) -> bool {
        let paths = AppGlobalPaths::new();
        let auto_start_path = &paths.auto_start_download_marker;

        if auto_start_path.try_exists().unwrap_or(false) {
            info("Found auto_start_download.txt marker file");
            // Remove the file after checking
            if let Err(e) = std::fs::remove_file(auto_start_path) {
                error(&format!("Failed to remove auto_start_download.txt: {}", e));
            }
            return true;
        }

        false
    }

    /// Whether the auto_start_download.txt marker file exists, **without
    /// removing it**.
    ///
    /// Used by the startup recovery decision, which must not consume the marker:
    /// `DownloadAppdataWindow.qml` reads it later and would then never
    /// auto-start the upgrade download. One `try_exists()`, nothing else — it
    /// runs before `app.exec()`.
    fn peek_auto_start_download(self: Pin<&mut Self>) -> bool {
        let paths = AppGlobalPaths::new();
        let auto_start_path = &paths.auto_start_download_marker;

        let found = auto_start_path.try_exists().unwrap_or(false);
        info(&format!("peek_auto_start_download(): {} at {}", found, auto_start_path.display()));
        found
    }

    /// Remove suttas and related data for specific language codes
    /// Runs in background thread and emits signals for progress and completion
    fn remove_sutta_languages(self: Pin<&mut Self>, language_codes: QStringList) {
        use simsapa_backend::get_app_data;
        use simsapa_backend::lookup::LANG_CODE_TO_NAME;

        info(&format!("remove_sutta_languages(): Removing {} languages", language_codes.len()));

        // Convert QStringList to Vec<String>
        let codes: Vec<String> = language_codes.iter()
            .map(|qs| qs.to_string())
            .collect();

        if codes.is_empty() {
            info("remove_sutta_languages(): No language codes provided");
            let qt_thread = self.qt_thread();
            qt_thread.queue(move |mut qo| {
                qo.as_mut().removal_completed(true, QString::from(""));
            }).unwrap();
            return;
        }

        info(&format!("Removing language codes: {:?}", codes));

        let qt_thread = self.qt_thread();

        // Show initial message
        let msg = QString::from("Preparing to remove languages...");
        qt_thread.queue(move |mut qo| {
            qo.as_mut().removal_show_msg(msg);
        }).unwrap();

        // Spawn a thread so Qt event loop is not blocked
        thread::spawn(move || {
            let app_data = get_app_data();

            // Create a progress callback that sends messages to Qt
            let progress_callback = |current_index: usize, total: usize, lang_code: &str| {
                let lang_name = LANG_CODE_TO_NAME.get(lang_code).copied().unwrap_or(lang_code);
                let lang_name_qstr = QString::from(lang_name);
                let qt_thread_clone = qt_thread.clone();
                qt_thread_clone.queue(move |mut qo| {
                    qo.as_mut().removal_progress_changed(current_index, total, lang_name_qstr);
                }).unwrap();
            };

            match app_data.dbm.remove_sutta_languages(codes, progress_callback) {
                Ok(success) => {
                    info(&format!("remove_sutta_languages(): Completed with success={}", success));
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().removal_completed(success, QString::from(""));
                    }).unwrap();
                },
                Err(e) => {
                    let error_msg = format!("Failed to remove languages: {}", e);
                    error(&format!("remove_sutta_languages(): {}", error_msg));
                    let error_qstr = QString::from(&error_msg);
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().removal_completed(false, error_qstr);
                    }).unwrap();
                }
            }
        });
    }

    fn download_urls_and_extract(self: Pin<&mut Self>, urls: QStringList, is_initial_setup: bool) {
        info(&format!("download_urls_and_extract(): {} urls", urls.len()));

        // AppGlobals was initialized before the storage path selection.
        // Get new paths to apply storage path selection.
        let paths = AppGlobalPaths::new();

        // Save to a temp folder (not in app-assets).
        // Only replace the assets after downloading and extracting succeeds.
        let download_temp_folder = paths.download_temp_folder.clone();
        let extract_temp_folder = paths.extract_temp_folder.clone();
        let app_assets_folder = paths.app_assets_dir.clone();

        let qt_thread = self.qt_thread();

        // Spawn a thread so Qt event loop is not blocked
        thread::spawn(move || {
            for url_qstr in urls.iter() {

                let url_str = url_qstr.to_string();
                let url = QUrl::from(&url_str);

                let download_file_name = url.file_name().to_string();
                let download_temp_file_path = download_temp_folder.join(&download_file_name);

                match extract_temp_folder.try_exists() {
                    Ok(exists) => {
                        if exists {
                            // If it exists, remove and re-create it to make sure we extract to an empty folder
                            let _ = remove_dir_all(&extract_temp_folder);
                        }
                        match create_dir_all(&extract_temp_folder) {
                            Ok(_) => {},
                            Err(e) => {
                                error(&format!("{}", e));
                                cleanup_on_failure(&download_temp_folder, &extract_temp_folder, &app_assets_folder, is_initial_setup, &qt_thread);
                                return;
                            },
                        };
                    }

                    Err(e) => {
                        error(&format!("{}", e));
                        cleanup_on_failure(&download_temp_folder, &extract_temp_folder, &app_assets_folder, is_initial_setup, &qt_thread);
                        return;
                    }
                }

                // Bound the DNS+TCP+TLS phase so a silent network stall surfaces
                // as a retryable error instead of freezing the UI indefinitely.
                // No overall timeout — database archives can be large and slow
                // connections must be allowed to finish the body download.
                let client = match Client::builder()
                    .connect_timeout(Duration::from_secs(30))
                    .build()
                {
                    Ok(c) => c,
                    Err(e) => {
                        error(&format!("Failed to build HTTP client: {}", e));
                        let msg = QString::from(&format!("Failed to build HTTP client: {}", e));
                        qt_thread.queue(move |mut qo| {
                            qo.as_mut().download_show_msg(msg);
                        }).unwrap();
                        cleanup_on_failure(&download_temp_folder, &extract_temp_folder, &app_assets_folder, is_initial_setup, &qt_thread);
                        return;
                    }
                };

                // Retry logic: try up to 5 times with exponential backoff
                const MAX_RETRIES: u32 = 5;
                let mut retry_count = 0;
                let mut resp = None;

                while retry_count < MAX_RETRIES {
                    let connecting_msg = QString::from(&format!(
                        "Connecting to download {}... (attempt {}/{})",
                        &download_file_name, retry_count + 1, MAX_RETRIES
                    ));
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().download_show_msg(connecting_msg);
                    }).unwrap();

                    match client.get(&url_str).send() {
                        Ok(r) => {
                            resp = Some(r);
                            break;
                        }
                        Err(e) => {
                            retry_count += 1;
                            if retry_count < MAX_RETRIES {
                                // Calculate wait time: 2^retry_count seconds (2, 4, 8, 16, 32)
                                let wait_seconds = 2_u64.pow(retry_count);
                                let retry_msg = QString::from(&format!(
                                    "Download failed for {}: {}. Retrying in {} seconds... (Attempt {}/{})",
                                    &download_file_name, e, wait_seconds, retry_count + 1, MAX_RETRIES
                                ));
                                qt_thread.queue(move |mut qo| {
                                    qo.as_mut().download_show_msg(retry_msg);
                                }).unwrap();
                                error(&format!("Download attempt {} failed: {}", retry_count, e));
                                thread::sleep(Duration::from_secs(wait_seconds));
                            } else {
                                // Max retries reached
                                error(&format!("Failed to download {} after {} attempts: {}", &download_file_name, MAX_RETRIES, e));
                                let fail_msg = QString::from(&format!(
                                    "Network error: Failed to download {} after {} attempts. Please check your internet connection and try again later.",
                                    &download_file_name, MAX_RETRIES
                                ));
                                qt_thread.queue(move |mut qo| {
                                    qo.as_mut().download_show_msg(fail_msg);
                                }).unwrap();
                                cleanup_on_failure(&download_temp_folder, &extract_temp_folder, &app_assets_folder, is_initial_setup, &qt_thread);
                                return;
                            }
                        }
                    }
                }

                let resp = match resp {
                    Some(r) => r,
                    None => {
                        // This shouldn't happen, but handle it just in case
                        let msg = QString::from("Unexpected error: failed to initiate download.");
                        qt_thread.queue(move |mut qo| {
                            qo.as_mut().download_show_msg(msg);
                        }).unwrap();
                        cleanup_on_failure(&download_temp_folder, &extract_temp_folder, &app_assets_folder, is_initial_setup, &qt_thread);
                        return;
                    }
                };

                let mut file = match File::create(&download_temp_file_path) {
                    Ok(f) => f,
                    Err(e) => {
                        qt_thread.queue(move |mut qo| {
                            let msg = QString::from(&format!("Error creating the file: {}", e));
                            qo.as_mut().download_show_msg(msg);
                        }).unwrap();
                        return;
                    }
                };

                let total = match resp.content_length() {
                    Some(n) => n as usize,
                    None => {
                        let error_msg = QString::from("Error: can't read download content length.");
                        let url_qstr_clone = url_qstr.clone();
                        qt_thread.queue(move |mut qo| {
                            qo.as_mut().download_needs_retry(url_qstr_clone, error_msg);
                        }).unwrap();
                        // The download file may have already been created with 0 length.
                        let _ = remove_file(download_temp_file_path);
                        // Stop processing the remaining URLs in the download loop and exit the thread.
                        // The QML layer receives the downloadNeedsRetry signal and will:
                        // 1. Display the error message to the user
                        // 2. Show a "Retry" button in the UI
                        // 3. When clicked, call download_urls_and_extract() again with just the failed URL
                        // 4. After successful retry, continue downloading any remaining URLs from the original list
                        return;
                    }
                };

                let mut reader = resp;
                let mut buf = [0u8; 8192]; // 8 KB buffer
                let mut downloaded = 0_usize;

                loop {
                    let n = reader.read(&mut buf).unwrap(); // read up to buf.len()
                    if n == 0 { break; } // EOF
                    file.write_all(&buf[..n]).unwrap();
                    downloaded += n;
                    let op_msg = QString::from(format!("Downloading {}", &download_file_name));
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().download_progress_changed(op_msg, downloaded, total);
                    }).unwrap();
                }

                let op_msg = QString::from(format!("Extracting {}", &download_file_name));
                qt_thread.queue(move |mut qo| {
                    qo.as_mut().download_progress_changed(op_msg, 0, 0);
                }).unwrap();

                // Extract contents to a temp folder and move contents on success
                let extraction_result = extract_tar_bz2_with_progress(&download_temp_file_path,
                                                                       &extract_temp_folder,
                                                                       &qt_thread);

                // Remove the downloaded tar.bz2 whether the extraction was successful or not.
                let _ = remove_file(download_temp_file_path);

                let extraction_success = match extraction_result {
                    Ok(_) => {
                        let msg = QString::from(format!("Completed extracting {}", &download_file_name));
                        qt_thread.queue(move |mut qo| {
                            qo.as_mut().download_show_msg(msg);
                        }).unwrap();
                        true
                    }
                    Err(e) => {
                        let msg = QString::from(format!("Extraction failed: {}", e));
                        error(&format!("Failed to extract {}: {}", &download_file_name, e));
                        qt_thread.queue(move |mut qo| {
                            qo.as_mut().download_show_msg(msg);
                        }).unwrap();
                        false
                    }
                };

                // Import language databases before moving files
                if extraction_success && download_file_name.starts_with("suttas_lang_") && download_file_name.ends_with(".tar.bz2") {
                    let import_msg = QString::from(format!("Importing {}", &download_file_name));
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().download_show_msg(import_msg);
                    }).unwrap();

                    match import_suttas_lang_to_appdata(&extract_temp_folder, &paths.appdata_database_url) {
                        Ok(_) => {
                            info(&format!("Successfully imported {}", &download_file_name));
                            let success_msg = QString::from(format!("Successfully imported {}", &download_file_name));
                            qt_thread.queue(move |mut qo| {
                                qo.as_mut().download_show_msg(success_msg);
                            }).unwrap();
                            // Refresh the per-area language caches off the
                            // calling thread so the search-bar language filter
                            // dropdown reflects the new sutta language on the
                            // next area switch.
                            std::thread::spawn(|| {
                                simsapa_backend::get_app_data().refresh_language_caches();
                            });
                        }
                        Err(e) => {
                            error(&format!("Failed to import {}: {}", &download_file_name, e));
                            let error_msg = QString::from(format!("Import failed for {}: {}", &download_file_name, e));
                            qt_thread.queue(move |mut qo| {
                                qo.as_mut().download_show_msg(error_msg);
                            }).unwrap();
                        }
                    }
                }

                // Move extracted contents to assets only if extraction was successful
                if extraction_success {
                    match move_folder_contents(&extract_temp_folder, &app_assets_folder) {
                        Ok(_) => {}
                        Err(e) => {
                            error(&format!("Failed to move files: {}", e));
                            let msg = QString::from(format!("Failed to move files: {}", e));
                            qt_thread.queue(move |mut qo| {
                                qo.as_mut().download_show_msg(msg);
                            }).unwrap();
                        }
                    }
                }
            } // end of for loop

            // Clean-up. All downloads are completed and extracted, remove the
            // download temp folder.
            let _ = remove_dir_all(&download_temp_folder);

            info("download_urls_and_extract(): all downloads completed");
            qt_thread.queue(move |mut qo| {
                qo.as_mut().downloads_completed(true);
            }).unwrap();

        }); // end of thread
    }
}

/// A wrapper around a Read trait object that tracks progress and sends messages to Qt.
struct ProgressReader<'a, R: Read> {
    /// The underlying reader.
    inner: R,
    /// Total bytes read so far from the inner reader. The self.bytes_read value
    /// can exceed self.total_size if BzDecoder reads a bit more for buffering.
    bytes_read: usize,
    /// Total size of the stream from the inner reader.
    total_size: usize,
    /// Last sent bytes_read value, for only sending increasing values.
    last_bytes_read: usize,
    /// The archive file name for formatting messages.
    file_name: String,
    /// CxxQtThread for sending signals to Qt.
    qt_thread: &'a CxxQtThread<qobject::AssetManager>,
}

impl<'a, R: Read> ProgressReader<'a, R> {
    /// Arguments:
    /// - `inner` - The reader to wrap.
    /// - `total_size` - The total number of bytes expected from the inner reader.
    /// - `qt_thread` - CxxQtThread for sending signals to Qt.
    fn new(inner: R,
           total_size: usize,
           file_name: &str,
           qt_thread: &'a CxxQtThread<qobject::AssetManager>)
           -> ProgressReader<'a, R> {
        ProgressReader {
            inner,
            bytes_read: 0,
            total_size,
            last_bytes_read: 0,
            file_name: file_name.to_string(),
            qt_thread,
        }
    }
}

impl<R: Read> Read for ProgressReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Read data from the inner reader
        let num_bytes = self.inner.read(buf)?;

        self.bytes_read += num_bytes;
        // Don't report greater than total size.
        if self.bytes_read > self.total_size {
            self.bytes_read = self.total_size;
        }

        if self.total_size > 0 {
            // Send only increased values.
            // The self.bytes_read can exceed self.total_size if BzDecoder reads a bit more
            // for its internal buffering before signaling EOF on the decompressed stream.
            if self.bytes_read > self.last_bytes_read {
                let op_msg = QString::from(format!("Extracting {}", &self.file_name));
                let bytes_read = self.bytes_read;
                let total_size = self.total_size;
                self.qt_thread.queue(move |mut qo| {
                    qo.as_mut().download_progress_changed(op_msg, bytes_read, total_size);
                }).unwrap();

                self.last_bytes_read = bytes_read;
            }
        }
        Ok(num_bytes)
    }
}

/// Extracts a .tar.bz2 archive to a specified output folder, printing progress.
///
/// Arguments:
/// - `archive_path` - Path to the .tar.bz2 archive file.
/// - `output_folder` - Path to the directory where contents will be extracted.
/// - `qt_thread` - CxxQtThread for sending signals to Qt.
pub fn extract_tar_bz2_with_progress(
    archive_path: &Path,
    output_folder: &Path,
    qt_thread: &CxxQtThread<qobject::AssetManager>,
) -> Result<(), Box<dyn Error>> {
    // 1. Create the output directory if it doesn't exist.
    create_dir_all(output_folder)
        .map_err(|e| format!("Failed to create output directory '{}': {}", output_folder.display(), e))?;

    // 2. Open the input .tar.bz2 file.
    let input_file = File::open(archive_path)
        .map_err(|e| format!("Failed to open archive file '{}': {}", archive_path.display(), e))?;

    // Get the total size of the compressed file for progress calculation.
    let total_size = input_file.metadata()?.len() as usize;

    if total_size == 0 {
        return Err(format!("Archive '{}' is empty. Nothing to extract.", archive_path.display()).into());
    }

    // 3. Wrap the file reader: File -> BufReader -> ProgressReader
    // BufReader adds buffering for potentially better I/O performance.
    let buffered_reader = BufReader::new(input_file);

    let a = archive_path.file_name().unwrap_or_default();
    let file_name = a.to_str().unwrap_or_default().to_string();

    // ProgressReader tracks bytes read from buffered_reader (the compressed stream).
    let progress_reader = ProgressReader::new(buffered_reader, total_size, &file_name, qt_thread);

    // 4. Set up the bzip2 decompressor.
    // BzDecoder will read from our ProgressReader.
    let bz_decoder = BzDecoder::new(progress_reader);

    // 5. Set up the tar archive reader.
    // Archive will read decompressed data from BzDecoder.
    let mut archive = Archive::new(bz_decoder);

    // Send initial progress status.
    let file_name_b = file_name.clone();
    qt_thread.queue(move |mut qo| {
        let op_msg = QString::from(format!("Extracting {}", &file_name_b));
        qo.as_mut().download_progress_changed(op_msg, 0, total_size);
    }).unwrap();

    // 6. Iterate through the entries in the tar archive and unpack them.
    // Progress signals are sent during read.
    for (i, entry_result) in archive.entries()?.enumerate() {
        let mut entry = entry_result
            .map_err(|e| format!("Failed to read entry {} from tar archive: {}", i, e))?;

        // Unpack the entry into the output folder.
        // This preserves the directory structure from the archive.
        entry.unpack_in(output_folder).map_err(|e| {
            format!(
                "Failed to unpack entry {} ('{}') into '{}': {}",
                i,
                entry.path().unwrap_or_default().display(),
                output_folder.display(),
                e
            )
        })?;
    }

    // 7. Send final progress status.
    qt_thread.queue(move |mut qo| {
        let op_msg = QString::from(format!("Completed extracting {}", &file_name));
        qo.as_mut().download_progress_changed(op_msg, total_size, total_size);
    }).unwrap();

    Ok(())
}

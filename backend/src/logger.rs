use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tracing_subscriber::EnvFilter;
use cfg_if::cfg_if;
use chrono::{DateTime, Local};

use crate::get_create_simsapa_dir;

cfg_if! {
    if #[cfg(target_os = "android")] {
        use std::io::Result as IoResult;
        use android_logger::{Config, FilterBuilder};

        /// Bridges the `tracing` fmt layer onto Android's logcat.
        ///
        /// The level is CARRIED, never inferred from the text. The fmt layer
        /// hands the writer a formatted line that contains the message body as
        /// well as the level, so a `line.contains("TRACE")`-style test would
        /// classify on user content -- and since `platform_setup()` runs
        /// `android_logger` at `LevelFilter::Debug`, anything routed to
        /// `log::trace!` is DISCARDED rather than merely mis-levelled. That is
        /// enough to erase a whole logging convention from logcat (it did:
        /// `STARTUP-TRACE`, C++ and QML alike).
        ///
        /// See `AGENTS.md` "Logging in C++".
        #[derive(Clone, Copy)]
        struct AndroidLogWriter {
            level: log::Level,
        }

        impl Write for AndroidLogWriter {
            fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
                let msg = std::str::from_utf8(buf).unwrap_or("invalid UTF-8");

                for line in msg.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    log::log!(self.level, "{}", line);
                }

                Ok(buf.len())
            }

            fn flush(&mut self) -> IoResult<()> {
                Ok(())
            }
        }

        struct AndroidMakeWriter;

        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for AndroidMakeWriter {
            type Writer = AndroidLogWriter;

            /// Only reached for writes with no event behind them. Events go
            /// through `make_writer_for`.
            fn make_writer(&'a self) -> Self::Writer {
                AndroidLogWriter { level: log::Level::Info }
            }

            /// Do not collapse this back into a closure: the blanket
            /// `MakeWriter` impl for `Fn() -> W` cannot see the event, so it
            /// only ever gets the default `make_writer_for`, which discards the
            /// metadata and makes the real level unreachable. Implementing the
            /// trait by hand is the whole point of the named type.
            fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
                let level = match *meta.level() {
                    tracing::Level::ERROR => log::Level::Error,
                    tracing::Level::WARN => log::Level::Warn,
                    tracing::Level::INFO => log::Level::Info,
                    tracing::Level::DEBUG => log::Level::Debug,
                    tracing::Level::TRACE => log::Level::Trace,
                };
                AndroidLogWriter { level }
            }
        }

        fn platform_setup() {
            android_logger::init_once(
                Config::default()
                    .with_max_level(log::LevelFilter::Debug)
                    .with_tag("simsapa")
                    .with_filter(FilterBuilder::new().parse("debug").build()),
            );
        }

        fn make_writer() -> impl for<'a> tracing_subscriber::fmt::MakeWriter<'a> + Send + Sync {
            AndroidMakeWriter
        }
    } else {
        fn platform_setup() {}

        fn make_writer() -> impl for<'a> tracing_subscriber::fmt::MakeWriter<'a> + Send + Sync {
            std::io::stdout
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LogPrecision {
    Seconds,
    Microseconds,
}

impl LogPrecision {
    pub fn unit_str(&self) -> &'static str {
        match self {
            LogPrecision::Seconds => "s",
            LogPrecision::Microseconds => "µs",
        }
    }
}

/// Log levels representing increasing verbosity.
///
/// # Behavior
/// Setting a log level enables that level and all less verbose levels below it.
/// The levels are ordered from least to most verbose:
///
/// - **Silent (0)**: No logging output
/// - **Error (1)**: Only error messages
/// - **Warn (2)**: Warning and error messages
/// - **Info (3)**: Informational, warning, and error messages (default)
/// - **Debug (4)**: All messages including debug output (most verbose)
///
/// # Examples
/// - Level::Info enables: Info, Warn, and Error (but not Debug)
/// - Level::Error enables: Only Error messages
/// - Level::Silent: No messages are logged
/// - Level::Debug: All messages are logged (Debug, Info, Warn, Error)
///
/// The level can be set via the `LOG_LEVEL` environment variable or at runtime
/// using `set_log_level()` or `Logger::set_level()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Silent = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
}

impl Level {
    pub fn as_str(&self) -> &'static str {
        match self {
            Level::Silent => "Silent",
            Level::Error => "Error",
            Level::Warn => "Warn",
            Level::Info => "Info",
            Level::Debug => "Debug",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseLevelError;

impl std::fmt::Display for ParseLevelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid log level. Valid values: silent, error, warn, info, debug")
    }
}

impl std::error::Error for ParseLevelError {}

impl FromStr for Level {
    type Err = ParseLevelError;

    /// Parse a log level from a string (case insensitive)
    ///
    /// Valid values: "silent", "error", "warn", "info", "debug"
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "silent" => Ok(Level::Silent),
            "error" => Ok(Level::Error),
            "warn" => Ok(Level::Warn),
            "info" => Ok(Level::Info),
            "debug" => Ok(Level::Debug),
            _ => Err(ParseLevelError),
        }
    }
}

pub struct TimeLog {
    start_time: Instant,
    prev_time: Mutex<Instant>,
    precision: LogPrecision,
    log_file: PathBuf,
}

impl TimeLog {
    pub fn new(precision: LogPrecision) -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = get_create_simsapa_dir()
            .map_err(|e| format!("Failed to get simsapa_dir: {}", e))?;

        std::fs::create_dir_all(&data_dir)?;
        let log_file = data_dir.join("profile_time.dat");

        let now = Instant::now();

        Ok(TimeLog {
            start_time: now,
            prev_time: Mutex::new(now),
            precision,
            log_file,
        })
    }

    pub fn start(&mut self, start_new: bool) -> Result<(), Box<dyn std::error::Error>> {
        self.start_time = Instant::now();
        if let Ok(mut prev) = self.prev_time.lock() {
            *prev = self.start_time;
        }

        if start_new {
            // Not checking with .exists() b/c of Android permission errors.
            // Try to remove but don't return on error, assume that writing it is still possible.
            match std::fs::remove_file(&self.log_file) {
                Ok(_) => {},
                Err(e) => error(&format!("{}", e)),
            }
        }

        Ok(())
    }

    pub fn log(&self, msg: &str) -> Result<(), Box<dyn std::error::Error>> {
        let trimmed_msg = if msg.len() > 30 {
            &msg[0..30]
        } else {
            msg
        };

        let now = Instant::now();
        let total_elapsed = now.duration_since(self.start_time);

        let step_elapsed = if let Ok(mut prev) = self.prev_time.lock() {
            let elapsed = now.duration_since(*prev);
            *prev = now;
            elapsed
        } else {
            Duration::from_secs(0)
        };

        let (total_time, step_time) = match self.precision {
            LogPrecision::Seconds => {
                (total_elapsed.as_secs(), step_elapsed.as_secs())
            }
            LogPrecision::Microseconds => {
                (total_elapsed.as_micros() as u64, step_elapsed.as_micros() as u64)
            }
        };

        // Escape underscores for compatibility with original format
        let escaped_msg = trimmed_msg.replace('_', "\\_");
        let dat_line = format!("{}\t{}\t{}\n", total_time, step_time, escaped_msg);

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)?;
        file.write_all(dat_line.as_bytes())?;

        Ok(())
    }
}

/// Rotates log files, keeping only the last 5 log files
fn rotate_log_files(log_file: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    // Use try_exists() to avoid Android permission crashes
    match log_file.try_exists() {
        Ok(true) => {
            // File exists, proceed with rotation
        }
        Ok(false) => {
            // File doesn't exist, nothing to rotate
            return Ok(());
        }
        Err(_) => {
            // Permission error or other issue, skip rotation
            return Ok(());
        }
    }

    // Get the last modified time
    let metadata = std::fs::metadata(log_file)?;
    let modified = metadata.modified()?;

    // Convert to datetime
    let datetime: DateTime<Local> = modified.into();
    let timestamp = datetime.format("%Y-%m-%dT%H-%M-%S");

    // Create the new filename
    let parent = log_file.parent().ok_or("No parent directory")?;
    let new_name = format!("log.{}.txt", timestamp);
    let new_path = parent.join(&new_name);

    // Rename the old log file
    std::fs::rename(log_file, &new_path)?;

    // Find all log.*.txt files
    let mut log_files: Vec<PathBuf> = std::fs::read_dir(parent)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                filename.starts_with("log.") && filename.ends_with(".txt") && filename != "log.txt"
            } else {
                false
            }
        })
        .collect();

    // Sort by filename (which sorts by datetime)
    log_files.sort();

    // Keep only the last 5
    if log_files.len() > 5 {
        for file in &log_files[0..log_files.len() - 5] {
            // Try to remove but don't fail if we can't (Android permissions)
            if let Err(e) = std::fs::remove_file(file) {
                eprintln!("Failed to remove old log file {:?}: {}", file, e);
            }
        }
    }

    Ok(())
}

pub struct Logger {
    log_file: PathBuf,
    time_log: Option<Arc<Mutex<TimeLog>>>,
    disable_log: bool,
    enable_print_log: bool,
    level: Arc<Mutex<Level>>,
}

impl Logger {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = get_create_simsapa_dir()
            .map_err(|e| format!("Failed to get simsapa_dir: {}", e))?;

        std::fs::create_dir_all(&data_dir)?;
        let log_file = data_dir.join("log.txt");

        // Rotate existing log file before creating a new one
        if let Err(e) = rotate_log_files(&log_file) {
            eprintln!("Failed to rotate log files: {}", e);
        }

        // Read environment variables
        let disable_log = std::env::var("DISABLE_LOG")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);


        cfg_if! {
            if #[cfg(target_os = "android")] {
                let enable_print_log = true;
            } else {
                let enable_print_log = std::env::var("ENABLE_PRINT_LOG")
                    .map(|v| v.to_lowercase() == "true")
                    .unwrap_or(false);
            }
        }

        let enable_time_log = std::env::var("ENABLE_TIME_LOG")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        let time_log = if enable_time_log {
            let mut tl = TimeLog::new(LogPrecision::Microseconds)?;
            tl.start(true)?;
            Some(Arc::new(Mutex::new(tl)))
        } else {
            None
        };

        // Read LOG_LEVEL from environment variable, default to Info
        let level = std::env::var("LOG_LEVEL")
            .ok()
            .and_then(|v| v.parse::<Level>().ok())
            .unwrap_or(Level::Info);

        Ok(Logger {
            log_file,
            time_log,
            disable_log,
            enable_print_log,
            level: Arc::new(Mutex::new(level)),
        })
    }

    pub fn init_tracing() -> Result<(), Box<dyn std::error::Error>> {
        platform_setup();

        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info"));

        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(false)
            .with_thread_ids(true)
            .with_file(false)
            .with_line_number(false)
            .with_writer(make_writer())
            .finish();

        tracing::subscriber::set_global_default(subscriber)?;

        Ok(())
    }

    fn write_to_file(&self, message: &str, start_new: bool) -> Result<(), Box<dyn std::error::Error>> {
        if self.disable_log {
            return Ok(());
        }

        let mut file = if start_new {
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&self.log_file)?
        } else {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.log_file)?
        };

        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3fZ");
        let log_line = format!("[{}] {}\n", timestamp, message);

        file.write_all(log_line.as_bytes())?;

        Ok(())
    }

    /// Log a debug message (most verbose level).
    /// Only logs if the current level is set to Debug.
    pub fn debug(&self, msg: &str, start_new: bool) {
        // Check if debug level is enabled (requires Level::Debug)
        if let Ok(level) = self.level.lock()
            && *level < Level::Debug {
                return;
            }

        let formatted_msg = format!("DEBUG: {}", msg);

        if self.enable_print_log {
            tracing::debug!("{}", msg);
        }

        if let Err(e) = self.write_to_file(&formatted_msg, start_new) {
            eprintln!("Failed to write to log file: {}", e);
        }
    }

    /// Log an informational message.
    /// Logs if the current level is Info or Debug.
    pub fn info(&self, msg: &str, start_new: bool) {
        // Check if info level is enabled (requires Level::Info or higher)
        if let Ok(level) = self.level.lock()
            && *level < Level::Info {
                return;
            }

        let formatted_msg = format!("INFO: {}", msg);

        if self.enable_print_log {
            tracing::info!("{}", msg);
        }

        if let Err(e) = self.write_to_file(&formatted_msg, start_new) {
            eprintln!("Failed to write to log file: {}", e);
        }
    }

    /// Log a warning message.
    /// Logs if the current level is Warn, Info, or Debug.
    pub fn warn(&self, msg: &str, start_new: bool) {
        // Check if warn level is enabled (requires Level::Warn or higher)
        if let Ok(level) = self.level.lock()
            && *level < Level::Warn {
                return;
            }

        let formatted_msg = format!("WARN: {}", msg);

        if self.enable_print_log {
            tracing::warn!("{}", msg);
        }

        if let Err(e) = self.write_to_file(&formatted_msg, start_new) {
            eprintln!("Failed to write to log file: {}", e);
        }
    }

    /// Log an error message.
    /// Logs if the current level is Error, Warn, Info, or Debug (all levels except Silent).
    pub fn error(&self, msg: &str, start_new: bool) {
        // Check if error level is enabled (requires Level::Error or higher)
        if let Ok(level) = self.level.lock()
            && *level < Level::Error {
                return;
            }

        let formatted_msg = format!("ERROR: {}", msg);

        if self.enable_print_log {
            tracing::error!("{}", msg);
        }

        if let Err(e) = self.write_to_file(&formatted_msg, start_new) {
            eprintln!("Failed to write to log file: {}", e);
        }
    }

    pub fn profile(&self, msg: &str, start_new: bool) {
        if let Some(time_log) = &self.time_log
            && let Ok(tl) = time_log.lock()
            && let Err(e) = tl.log(msg) {
                eprintln!("Failed to write to profile log: {}", e);
            }

        let elapsed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0));

        let profile_msg = format!("PROFILE: {}: {:?}", msg, elapsed);

        if self.enable_print_log {
            tracing::debug!("{}", profile_msg);
        }

        if let Err(e) = self.write_to_file(&profile_msg, start_new) {
            eprintln!("Failed to write to log file: {}", e);
        }
    }

    /// Get the current log level.
    ///
    /// Returns the currently configured log level. If the lock cannot be acquired,
    /// returns Level::Info as a safe default.
    pub fn get_level(&self) -> Level {
        self.level.lock().map(|l| *l).unwrap_or(Level::Info)
    }

    /// Set the log level.
    ///
    /// Changes the logging verbosity at runtime. Setting a level enables that level
    /// and all less verbose levels. For example:
    /// - Setting Level::Info enables Info, Warn, and Error messages
    /// - Setting Level::Debug enables all message types
    /// - Setting Level::Silent disables all logging
    pub fn set_level(&self, new_level: Level) {
        if let Ok(mut level) = self.level.lock() {
            *level = new_level;
        }
    }
}

// Global logger instance using OnceLock for thread-safe initialization
pub static LOGGER: OnceLock<Logger> = OnceLock::new();
static TRACING_INITIALIZED: OnceLock<()> = OnceLock::new();

/// Check if logger is already initialized
/// Returns 1 if initialized, 0 if not
#[unsafe(no_mangle)]
pub extern "C" fn is_logger_initialized() -> i32 {
    if LOGGER.get().is_some() { 1 } else { 0 }
}

fn with_logger<F, R>(f: F) -> R
where
    F: FnOnce(&Logger) -> R,
{
    // Initialize tracing once, globally
    TRACING_INITIALIZED.get_or_init(|| {
        if let Err(e) = Logger::init_tracing() {
            eprintln!("Failed to initialize tracing: {}", e);
        }
    });
    
    let logger = LOGGER.get_or_init(|| {
        // Create logger instance
        match Logger::new() {
            Ok(logger) => logger,
            Err(e) => {
                eprintln!("Failed to create logger: {}", e);
                // Return a disabled logger that will silently do nothing
                Logger {
                    log_file: std::path::PathBuf::new(),
                    time_log: None,
                    disable_log: true,
                    enable_print_log: false,
                    level: Arc::new(Mutex::new(Level::Info)),
                }
            }
        }
    });

    f(logger)
}

// Public API functions
pub fn info(msg: &str) {
    info_with_options(msg, false);
}

pub fn info_with_options(msg: &str, start_new: bool) {
    with_logger(|logger| logger.info(msg, start_new));
}

pub fn warn(msg: &str) {
    warn_with_options(msg, false);
}

pub fn warn_with_options(msg: &str, start_new: bool) {
    with_logger(|logger| logger.warn(msg, start_new));
}

pub fn error(msg: &str) {
    error_with_options(msg, false);
}

pub fn error_with_options(msg: &str, start_new: bool) {
    with_logger(|logger| logger.error(msg, start_new));
}

pub fn debug(msg: &str) {
    debug_with_options(msg, false);
}

pub fn debug_with_options(msg: &str, start_new: bool) {
    with_logger(|logger| logger.debug(msg, start_new));
}

pub fn profile(msg: &str) {
    profile_with_options(msg, false);
}

pub fn profile_with_options(msg: &str, start_new: bool) {
    with_logger(|logger| logger.profile(msg, start_new));
}

/// Get the current log level.
///
/// Returns the currently configured log level enum value.
pub fn get_log_level() -> Level {
    with_logger(|logger| logger.get_level())
}

/// Set the log level.
///
/// Changes the logging verbosity at runtime. Setting a level enables that level
/// and all less verbose levels:
/// - Level::Silent: No logging
/// - Level::Error: Only errors
/// - Level::Warn: Warnings and errors
/// - Level::Info: Info, warnings, and errors (default)
/// - Level::Debug: All messages (most verbose)
pub fn set_log_level(level: Level) {
    with_logger(|logger| logger.set_level(level));
}

/// Get the current log level as a string.
///
/// Returns one of: "Silent", "Error", "Warn", "Info", or "Debug"
pub fn get_log_level_str() -> String {
    with_logger(|logger| logger.get_level().as_str().to_string())
}

/// Set the log level from a string (case insensitive).
///
/// Valid values: "silent", "error", "warn", "info", "debug" (case insensitive)
///
/// Returns true if successful, false if the string is not a valid level.
///
/// # Example
/// ```ignore
/// use simsapa_backend::logger::set_log_level_str;
///
/// set_log_level_str("debug"); // Enable all logging
/// set_log_level_str("error"); // Only log errors
/// set_log_level_str("INFO");  // Case insensitive - logs Info, Warn, and Error
/// ```
pub fn set_log_level_str(level_str: &str) -> bool {
    if let Ok(level) = level_str.parse::<Level>() {
        set_log_level(level);
        true
    } else {
        false
    }
}

// Utility function to format duration similar to the Python strfdelta
pub fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

// C-compatible logging functions
use std::ffi::CStr;
use std::os::raw::c_char;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn log_info_c(msg: *const c_char) {
    if msg.is_null() {
        return;
    }
    let c_str = unsafe { CStr::from_ptr(msg) };
    if let Ok(rust_str) = c_str.to_str() {
        info(rust_str);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn log_debug_c(msg: *const c_char) {
    if msg.is_null() {
        return;
    }
    let c_str = unsafe { CStr::from_ptr(msg) };
    if let Ok(rust_str) = c_str.to_str() {
        debug(rust_str);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn log_info_with_options_c(msg: *const c_char, start_new: bool) {
    if msg.is_null() {
        return;
    }
    let c_str = unsafe { CStr::from_ptr(msg) };
    if let Ok(rust_str) = c_str.to_str() {
        info_with_options(rust_str, start_new);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn log_error_c(msg: *const c_char) {
    if msg.is_null() {
        return;
    }
    let c_str = unsafe { CStr::from_ptr(msg) };
    if let Ok(rust_str) = c_str.to_str() {
        error(rust_str);
    }
}

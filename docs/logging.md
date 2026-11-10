# Logging in QML, C++ and on Android

## Logging in QML (no console API)

In the QML files under `bridges/assets/qml/`, do **not** use the `console` API
(`console.log()`, `console.error()`, etc.). Use the `Logger { id: logger }`
module's functions for logging instead.

The one exception is the folder
`bridges/assets/qml/com/profoundlabs/simsapa/`: the `console` API is allowed there
because those files are type stubs for `qmllint`.

### Using the Logger module

`Logger.qml` lives in `bridges/assets/qml/`, so it is automatically available to any
other component in that directory — no `import` statement is needed. Declare an
instance once in the component's root element, conventionally with `id: logger`:

``` qml
Item {
    id: root

    Logger { id: logger }

    // ...
}
```

The available functions map to log levels: `logger.debug(message)`,
`logger.info(message)`, `logger.warn(message)`, `logger.error(message)`.

**IMPORTANT — each function takes a single `message` argument, unlike the
variadic `console` API.** Do not pass multiple comma-separated arguments;
build one string with concatenation instead:

``` qml
// ❌ BAD - console-style multiple arguments; extra args are dropped/ignored
logger.error("Failed to parse data_json:", e, "data_json:", root.data_json);

// ✅ GOOD - a single concatenated string
logger.error("Failed to parse data_json: " + e + " data_json: " + root.data_json);
```

When migrating from `console`, map the methods by severity rather than
mechanically: `console.error` → `logger.error`, `console.warn` → `logger.warn`,
and `console.log` → `logger.info` (or `logger.error` when the message actually
reports a failure).

## Logging in C++ (`log_info_c()`, not `qInfo()`)

In the C++ files under `cpp/`, log through the app's own logger — the Rust FFI
functions `log_info_c()` / `log_error_c()` — and **not** Qt's `qInfo()` /
`qWarning()` / `qDebug()`:

``` cpp
extern "C" void log_info_c(const char* msg);
extern "C" void log_error_c(const char* msg);

log_info_c("start(): storage scan requested");
log_info_c(QString("Found %1 volume(s)").arg(count).toUtf8().constData());
```

**Why: on Android, Qt tags its own messages with the *application name*, not
with `Qt`.** The documented way to watch the app's log is

``` sh
adb logcat -s simsapa Qt QtCore QtQml
```

(see [android-beta-distribution-and-play-policy.md](./android-beta-distribution-and-play-policy.md)),
where `simsapa` is the Rust logger's tag. A `qInfo()` call therefore lands under
*neither* `simsapa` nor `Qt` and is filtered out entirely — the message looks
like code that never ran. This has already produced one wasted device-debugging
round trip: a storage-enumeration diagnostic added specifically to prove a JNI
pass had executed was invisible in the log, which is indistinguishable from the
failure it was added to detect.

`log_info_c()` output goes to the same `simsapa` tag as the Rust backend's, and
also into the app's own `log.txt`, so it is available when a user sends logs.

### The Android log level comes from the event metadata

`AndroidMakeWriter` (`backend/src/logger.rs`) implements
`MakeWriter::make_writer_for`, which receives the event's `Metadata`, so the
writer carries the mapped `log::Level` and `write` is a plain
`log::log!(self.level, …)`. Message text plays no part in levelling, so `TRACE`,
`ERROR`, `WARN` and `DEBUG` are safe words to use inside a log message.

> **Trap: never level by inspecting the formatted line, and never reduce the
> writer to a closure.** The line handed to the writer contains the message body
> as well as the level, so a `line.contains("TRACE")` test classifies on user
> content — and since `android_logger` runs at `LevelFilter::Debug`, anything
> routed to `log::trace!` is *discarded*, not merely mis-levelled. That silently
> erased the entire `STARTUP-TRACE` convention from logcat, C++ and QML alike.
>
> The reason the writer is a **named type** rather than `move || writer.clone()`
> is that `MakeWriter`'s blanket impl for `Fn() -> W` cannot see the event and
> only ever gets the default `make_writer_for`, which discards the metadata —
> making the real level unreachable. Simplifying it back to a closure
> reintroduces the bug.

A genuine `tracing::trace!` event is still filtered out by `LevelFilter::Debug`.
That is a level decision, not an accident of wording.

**When instrumentation seems not to have run on device, read the app's own
`log.txt` before concluding anything** — it is what a user sends you, and it
carries messages regardless of logcat's filtering. On a debuggable build:
`adb shell run-as io.github.simsapa.app.beta cat files/log.txt`.
(`run-as PKG sh -c '…'` does **not** work — SELinux denies it, and the shell does
not inherit the app home as cwd — but `run-as PKG <binary>` does.)

Pre-existing `qWarning()` calls remain in some files (e.g. the file-copy helpers
in `cpp/utils.cpp`); do not add new ones, and prefer converting them when
touching that code for another reason.


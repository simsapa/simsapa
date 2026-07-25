# Releases info lookup and the embedded fallback JSON

The app needs **releases info** — a JSON document listing the latest application
and asset (database) releases, with the `github_repo` and `version_tag` used to
build the GitHub asset download URLs. Without it, first-run setup and the
language-download flow cannot construct the URLs to fetch the appdata /
dictionaries / DPD / index / per-language archives.

This releases info is served by a small web app on **pythonanywhere**:

```
https://simsapa.eu.pythonanywhere.com/releases
```

Because that server can be unreachable (no network during first-run setup, the
server is down, etc.), the app ships an **embedded fallback snapshot** of the
same JSON so it can still resolve the asset URLs. This document describes the
lookup, the fallback, and how the user is informed when things actually fail.

## The two sources of releases info

| Source | Where | When used |
| --- | --- | --- |
| **Live server** | `POST https://simsapa.eu.pythonanywhere.com/releases` with system-info params | Always tried first |
| **Embedded fallback** | `assets/releases-fallback.json`, compiled into the binary via `include_str!` | Only when the live fetch fails |

### Live fetch — `fetch_releases_info()`

`backend/src/update_checker.rs` → `fetch_releases_info(screen_size, save_stats_behaviour)`
is **strict**: it does a single `POST` to `RELEASES_API_URL` (with system-info
params for optional analytics) and returns `Err` on any network, HTTP-status, or
parse failure. It does **not** fall back internally — keeping it pure means the
caller can distinguish "server reachable" from "had to fall back".

#### The `no_stats` opt-out and `ReleasesRequestParams`

`ReleasesRequestParams` (same file) always sends `channel` and `no_stats`. Every
other field (`app_version`, `system`, `machine`, `cpu_max`, `cpu_cores`,
`mem_total`, `screen`) is an `Option<String>` with
`#[serde(skip_serializing_if = "Option::is_none")]`, so an opted-out request
carries **only** `{"channel": …, "no_stats": true}`.

`collect_system_info()` decides `no_stats` in two steps:

1. the `SaveStatsBehaviour` argument — `Enabled` → false, `Disabled` → true,
   `Determine` → `!AppGlobals.save_stats` (from the `SAVE_STATS` / `NO_STATS`
   env variables);
2. **OR**ed with the user setting `AppSettings::dont_send_stats` — an opt-out, so
   it overrides the behaviour argument in the "don't send" direction only.

When `no_stats` is true the system info is **not collected at all** (the function
returns early with `None` fields) — CPU/memory/screen are never read. The setting
is exposed in **Settings → General → Updates** as the "Don't send stats"
checkbox (default off), plumbed through
`SuttaBridge::get_dont_send_stats()` / `set_dont_send_stats()` →
`AppData::get_dont_send_stats()` / `set_dont_send_stats()`. `collect_system_info()`
reads it through `try_get_app_data()`, so it degrades to `false` (the default) when
app data is not initialized yet — mirroring `get_release_channel()`.

### Embedded fallback — `FALLBACK_RELEASES_INFO_JSON` / `get_fallback_releases_info()`

Also in `backend/src/update_checker.rs`:

```rust
static FALLBACK_RELEASES_INFO_JSON: &str = include_str!("../../assets/releases-fallback.json");

pub fn get_fallback_releases_info() -> Option<ReleasesInfo>;
```

`get_fallback_releases_info()` parses the embedded snapshot into `ReleasesInfo`.
It returns `None` only if the bundled JSON itself cannot be parsed — which would
be a build-time data error, not a runtime condition.

The fallback mirrors the `PROVIDERS_JSON` pattern in
`backend/src/app_settings.rs` (a bundled JSON asset included at build time).

## Expected runtime behaviour

The orchestration lives in `bridges/src/sutta_bridge.rs` →
`SuttaBridge::check_for_updates()` (run on a background thread). The releases
info, once obtained, is stored in the process-global via
`simsapa_backend::set_releases_info()` and later read by
`get_compatible_asset_version_tag()` / `get_compatible_asset_github_repo()` to
build the download URLs.

Decision flow inside `check_for_updates()` when fetching releases info:

1. **Live fetch succeeds** → `set_releases_info(live)`, proceed normally
   (app-update / db-update detection, then asset URLs are available).
2. **Live fetch fails, fallback usable** → log a `warn`
   (`"Failed to fetch releases info (…), using embedded fallback"`),
   `set_releases_info(fallback)`, and **proceed silently**. The user is **not**
   shown an update-check error, because the asset URLs are still resolvable from
   the snapshot.
3. **Live fetch fails, fallback unusable** (`get_fallback_releases_info()` →
   `None`, i.e. the bundled JSON won't parse) → emit `update_check_error`
   ("Failed to fetch updates") and `releases_check_completed`. This is the only
   path that reports a releases-info failure to the user, and in practice means a
   broken build.

### Why a pythonanywhere outage is not surfaced as an error

A mere server outage is **not** a user-facing error: the embedded fallback
covers it, setup/downloads continue, and a scary "Failed to fetch updates"
dialog would be misleading. The genuine, user-relevant failure is **not being
able to download the asset**, which is handled and shown separately (below).

### Asset URL resolution also falls back independently — `compatible_assets_release()`

`get_compatible_asset_version_tag()` / `get_compatible_asset_github_repo()` do
**not** rely on `check_for_updates()` having run. They go through the shared
helper `compatible_assets_release()` (in `bridges/src/sutta_bridge.rs`), which
prefers `try_get_releases_info()` (the live global) and **falls back to
`get_fallback_releases_info()`** when the global is empty:

```rust
fn compatible_assets_release() -> Option<update_checker::ReleaseEntry> {
    let releases_info = simsapa_backend::try_get_releases_info()
        .or_else(update_checker::get_fallback_releases_info)?;
    let app_version = update_checker::to_version(&update_checker::get_app_version()).ok()?;
    update_checker::get_latest_app_compatible_assets_release(&releases_info, &app_version).cloned()
}
```

This matters for **`SuttaLanguagesWindow.qml`** (downloading additional sutta
languages): that window does **not** call `check_for_updates()` — its
`start_download()` just reads `get_compatible_asset_*`. Without this fallback,
the global could be empty (the startup check in `SuttaSearchWindow.qml` only runs
when `get_notify_about_simsapa_updates()` is true, and runs on a background
thread that may not have finished), and the user would hit
"Unable to retrieve download information." even though the embedded snapshot is
available. With the fallback, language downloads resolve the asset URL offline,
and only the actual asset download (via `AssetManager`) reports a network error
if it then fails.

## How the user is informed

There are three distinct failure points; only the meaningful ones reach the user.

- **Releases-info fetch (pythonanywhere) fails** → silently handled by the
  fallback (step 2 above). Logged as a `warn`, no dialog.
- **Asset download fails** (the fallback gave a URL, but downloading the GitHub
  asset fails — e.g. network error) → surfaced by `AssetManager`
  (`bridges/src/asset_manager.rs`). The download has a 5-try exponential-backoff
  retry loop; on exhaustion it shows
  `"Network error: Failed to download … Please check your internet connection
  and try again later."` via `download_show_msg` and runs `cleanup_on_failure`.
  `DownloadAppdataWindow.run_download()` also opens its `error_dialog`
  ("Unable to retrieve download information…") if the asset `github_repo` /
  `version_tag` are empty. **This is the case the user actually sees and acts
  on.**
- **Embedded fallback unparseable** (broken build) → `update_check_error`
  → "Failed to fetch updates". In the main window this is only logged
  (`SuttaSearchWindow.qml` `onUpdateCheckError`) and falls through to startup
  validation.

So, end to end:

1. pythonanywhere JSON fails → silently use the embedded fallback → asset URLs
   are available.
2. asset URL download fails → `AssetManager` shows the network error / retry to
   the user.
3. embedded fallback JSON itself unparseable → "Failed to fetch updates".

## Refreshing the fallback snapshot — CLI command

`assets/releases-fallback.json` is a **manually refreshed snapshot**. It is
**not** updated during the bootstrap procedure, because the server-side releases
data is updated *after* a bootstrap/build. Refresh it with the dedicated CLI
command after updating the server data:

```sh
# defaults: --channel main, output → the source-tree assets/ folder
cargo run --manifest-path cli/Cargo.toml -- update-releases-fallback
# equivalently, from inside cli/:
cargo run -- update-releases-fallback
```

Implementation: `cli/src/update_releases_fallback.rs`, wired into
`cli/src/main.rs` as the `UpdateReleasesFallback { channel, output }` command
(in the skip-`init_app_data` list, like `UpdateProviderModels`). It does a
`GET {RELEASES_API_URL}?channel=<channel>&no_stats=true`, validates the response
parses as `ReleasesInfo` before overwriting the file (so a bad response can't
break the build), and writes the raw JSON.

The `--output` default is `DEFAULT_RELEASES_FALLBACK_PATH`, resolved at compile
time via `concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/releases-fallback.json")`
— so it always targets the workspace `assets/` folder regardless of the current
working directory (running from `cli/` works). Errors from this command are
printed with the full anyhow context chain (`{:#}`) so path/IO causes are
visible.

`no_stats=true` is used so the server does **not** log the request — this is a
maintenance fetch, not a real user check.

> The JSON is bundled at build time via `include_str!`, so after refreshing the
> file you must **rebuild** the app for the new fallback to take effect.

## Release channel — `get_release_channel()`

The channel sent to the server (and used for the fallback CLI default) comes from
`get_release_channel()` in `backend/src/update_checker.rs`, in precedence order:

1. `RELEASE_CHANNEL` environment variable
2. `release_channel` in `AppSettings`
3. Default: `"main"`

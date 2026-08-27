# Mobile rendering troubleshooting toggles

On some Android devices with flaky GPU drivers (certain Adreno/Mali), the Qt
Quick scene graph renders corrupted output — coloured blocks (red/green/yellow/
black) and diagonal white streaks — over the search-results `ListView`. This is
GPU framebuffer / uninitialized-texture corruption from the device driver, not
an app-logic or layout bug. It reproduces across Qt versions, which points at
the driver rather than a Qt regression.

To let users work around it without a custom build, the **Rendering** tab in
Settings (a `TabButton` shown only on mobile) exposes three toggles, ordered
cheapest/most targeted first. All default to **off**, and the tab warns the user
not to enable them unless they are actually seeing rendering errors.

| Toggle (UI label) | Setting field | Applied where | Mechanism |
|---|---|---|---|
| Flat result backgrounds (no gradient) | `render_use_flat_results_background` | QML | `use_flat_bg` on `ListBackground` in `FulltextResults.qml` — collapses the per-card gradient (a common Adreno/Mali corruption trigger) to a flat fill |
| Disable clipping of the results list | `render_disable_results_clip` | QML | `clip: !…` on the results `ListView` (stencil/scissor clip mishandled by some drivers) |
| Use the basic (single-threaded) render loop | `render_loop_basic` | `gui.cpp` | `QSG_RENDER_LOOP=basic` |

> Two further toggles were tried and removed: `QSG_RHI_BACKEND=vulkan` **crashed
> the app**, and `QT_QUICK_BACKEND=software` produced an **unusable UI** on the
> test device.

## Why two application paths

The `render_loop_basic` env-var toggle must be set **before `QApplication` is
constructed**, which is **before `init_app_data()`** runs. So `gui.cpp` reads it
directly from the DB via the standalone `db::get_app_settings()` (which needs
only `AppGlobals`, already initialized by `init_app_globals()`), through the FFI
function `render_loop_basic_c()` in `backend/src/lib.rs`. It is read once and
cached (`RENDER_SETTINGS_CACHE`), gated on `appdata_db_exists()`.
**Consequence: changing the env-var toggle only takes effect after an app
restart** — the Settings text says so.

The two QML toggles are read through the normal `SuttaBridge` getters in
`SuttaSearchWindow.qml` and passed down to `FulltextResults.qml` as properties
(mirroring `is_dark`). They apply to newly opened windows.

## Code map

- Settings struct: `backend/src/app_settings.rs` (three `bool` fields, default false)
- Get/set + persistence: `backend/src/app_data.rs`
- Bridge: `bridges/src/sutta_bridge.rs` (+ qmllint stubs in
  `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`)
- Early-read FFI: `backend/src/lib.rs` → consumed in `cpp/gui.cpp`
- UI: `bridges/assets/qml/AppSettingsWindow.qml` (`is_mobile`-gated Rendering tab)

Related: [Android / ChromeOS soft keyboard](./android-soft-keyboard.md),
[Startup sequence and caches](./startup-sequence-and-caches.md).

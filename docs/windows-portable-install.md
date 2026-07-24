# Windows Portable Install

The Windows installer (`Simsapa-Setup-<version>.exe`, built by
`build-windows.ps1` from `simsapa-installer.iss`) offers three install options
on a custom wizard page shown right after Welcome. Each option's title is bold
and its description lists the default folder it installs to, so the user can pick
the same location when re-installing. For the two Standard options, if Simsapa is
already installed in that default folder the page shows a bold note ("There is
already a Simsapa version in … - installing will update it.") via
`ExistingInstallNote` (a `FileExists` check for the exe in the default folder):

- **Standard Install - all users**: installs to `C:\Program Files\Simsapa`
  (`{commonpf}\Simsapa`) for everyone on the computer. **Requires administrator
  rights**, supplied by launching the installer with "Run as administrator".
  Registers a normal uninstaller (with the user-data prompt).
- **Standard Install - this user only**: installs to
  `C:\Users\<user>\AppData\Local\Programs\Simsapa` (`{localappdata}\Programs\Simsapa`)
  for the current account only. **No administrator rights required.** Registers a
  normal uninstaller (with the user-data prompt).
- **Portable Install**: installs into any folder the user picks (default
  `Desktop\Simsapa`; e.g. a USB drive), keeps all data in a sibling folder next
  to the app, requires no administrator rights, and registers **no** uninstaller.

Both Standard options store user data in `%LOCALAPPDATA%\profound-labs\simsapa`.

This document focuses on the portable mode; the rest of this section explains how
the three options are wired together.

## Privileges

The installer runs with the user's own privileges: `PrivilegesRequired=lowest`
with **no** `PrivilegesRequiredOverridesAllowed`. So there is **no UAC prompt and
no "Select Setup Install Mode" dialog at startup**, whether launched normally or
via "Run as administrator". The install location is chosen explicitly on the mode
page instead.

Only **Standard - all users** needs administrator rights (it writes to
`C:\Program Files`). The user supplies those by launching the installer with
**"Run as administrator"**. On the mode page:

- The **default** option is *all users* when the installer was launched elevated
  (`IsAdmin()`), otherwise *this user only*. Portable is never the default. A
  Standard option is therefore always the default so users who do not expect
  portable mode are not surprised.
- `NextButtonClick` **warns and blocks** if *all users* is selected while the
  installer is **not** elevated, steering the user to re-run as administrator or
  to pick *this user only* / *Portable*.

The mode page is built from custom `TNewRadioButton` + `TNewStaticText` controls
(rather than `CreateInputOptionPage`) so each title is bold while its description
(including the default path) stays normal weight. The directory-page default is
set per option in `CurPageChanged` using explicit folder constants (`{commonpf}`,
`{localappdata}\Programs`, `{userdesktop}`) rather than `{autopf}`, because under
`lowest` privileges `{autopf}` always resolves to the per-user location.

**The installed app never needs administrator rights at runtime** in any mode —
the two Standard modes store data under `%LOCALAPPDATA%`, Portable in its sibling
data folder.

> **Note (uninstaller scope).** Because the installer runs in the lowest
> (per-user) install mode, even an *all users* install registers its uninstaller
> under the per-user (HKCU) Add/Remove Programs list rather than HKLM. For this
> single-user desktop app that is acceptable; the uninstaller works normally for
> the installing user. An earlier iteration used
> `PrivilegesRequiredOverridesAllowed=dialog`, but its "Select Setup Install
> Mode" dialog appeared even when the exe was started with "Run as
> administrator", which was confusing, so it was removed.

## Folder layout (Desktop example)

```
Desktop\
  Simsapa.lnk  (or Simsapa.cmd)      <- launcher in the parent folder
  Simsapa\                            <- install (app) folder = {app}
    simsapadhammareader.exe
    config.txt        (SIMSAPA_DIR=../SimsapaData)
    simsapa.ico
    <Qt libs, plugins, ...>
  SimsapaData\                        <- SIMSAPA_DIR data folder
    app-assets\   (downloaded on first launch: appdata.sqlite3, dictionaries, ...)
    logs\
```

On a USB stick the same layout is rooted at the drive, e.g. `E:\Simsapa.cmd`,
`E:\Simsapa\...`, `E:\SimsapaData\...`.

## How it works

### Installer (`simsapa-installer.iss`)

1. **Mode page** (`ModePage`, a `CreateCustomPage` after Welcome) offers three
   options via custom radio buttons (all users / this user / portable) and sets
   the `IsPortable` script-global (`PortableRadio.Checked`); the two Standard
   options are distinguished by `AllUsersRadio.Checked` / `ThisUserRadio.Checked`.
   `ShouldRunPortable` / `ShouldRunStandard` gate `[Icons]` and the
   `Uninstallable=ShouldRunStandard` directive (a `[Code]` Boolean function —
   `Uninstallable` takes a boolean expression, not a `{code:...}` string constant
   — so Portable registers no uninstaller / Add-Remove-Programs entry).
2. **Directory page** default is set per option in `CurPageChanged`: all users →
   `{commonpf}\Simsapa`, this user → `{localappdata}\Programs\Simsapa`, portable →
   `{userdesktop}\Simsapa`. The user may pick any folder. `[Setup]` sets
   `DisableDirPage=no` and `UsePreviousAppDir=no` so this page **always** appears
   and never reuses a folder remembered from a previous install — otherwise Inno
   auto-hides it when a prior install of the same `AppId` is detected, and a
   Portable re-install would skip the folder chooser entirely.
3. **Data folder** is a sibling of the install folder named by appending `Data`
   to the install folder's name (`…\Simsapa` → `…\SimsapaData`), computed by
   `GetPortableDataDir`. In `CurStepChanged` (`ssPostInstall`) it is created if
   absent and **reused as-is if it already exists** (so a reinstall keeps
   already-downloaded databases).
4. **`config.txt`** is written into the install folder next to the exe with a
   single line:
   ```
   SIMSAPA_DIR=../SimsapaData
   ```
   The path is **relative**, **unquoted**, and uses **forward slashes**. Rust
   accepts `/` as a path separator on Windows; `dotenvy` treats `\` as an escape
   character, so backslashes are avoided. The suffix matches the data-folder name.
   In Portable mode the standard **"Select Start Menu Folder"** page
   (`wpSelectProgramGroup`) is skipped via `ShouldSkipPage`, and the
   **"Create a desktop icon"** task (`[Tasks]` `desktopicon`) is gated with
   `Check: ShouldRunStandard` — neither does anything for a portable install
   (the launcher lives in the parent folder), so they are hidden to avoid
   confusion.
5. **Launcher page** (`LauncherPage`, Portable only via `ShouldSkipPage`) lets
   the user choose, and the launcher is created in the **parent** of the install
   folder in `CurStepChanged`:
   - **`.lnk` shortcut** — created with `CreateShellLink`, targets the installed
     exe and uses `simsapa.ico`. Simplest; may break if a USB drive is given a
     different letter on another computer (a `.lnk` stores an absolute target).
   - **`.cmd` launcher** — recommended for USB. Contents:
     ```bat
     @echo off
     start "" "%~dp0Simsapa\simsapadhammareader.exe"
     ```
     `%~dp0` expands to the launcher's own drive+path, so the exe is resolved
     relative to the `.cmd`'s location and survives drive-letter changes. The
     install subfolder name is substituted for `Simsapa`. Downsides: a brief
     console flash and the generic batch icon.

   Neither launcher relies on a "Start in"/CWD value — the app finds `config.txt`
   via the exe's own directory (see below).

### Application (`backend/src/lib.rs`)

1. `init_dotenv()` loads `config.txt` from the **executable's own directory**
   (via `exe_dir()`), in addition to the existing CWD `.env`/`config.txt` and
   the `get_create_simsapa_dir()` `config.txt`. `dotenvy` does not override
   variables already set, so an explicit `SIMSAPA_DIR` env var still wins. The
   exe-dir `config.txt` is loaded **before** any path resolution that reads
   `SIMSAPA_DIR`. If `current_exe()` fails, `exe_dir()` returns `None` and the
   app falls back to existing behavior without panicking.
2. `resolve_simsapa_dir(raw, exe_dir)` resolves the value: a **relative** path is
   joined onto the exe directory and normalized with `normalize_lexically()`
   (lexical `..`/`.` collapse, **not** `std::fs::canonicalize()` — which returns
   `\\?\`-prefixed paths on Windows that break Qt/SQLite). An **absolute** path
   is used unchanged. A relative value with no exe dir falls back to the raw
   string.
3. The resolved `SIMSAPA_DIR` flows through `get_create_simsapa_dir()` and the
   downstream helpers (`get_create_simsapa_app_assets_path()`,
   `get_create_simsapa_appdata_db_path()`, logging), so databases, `app-assets/`,
   and logs all land under the portable data folder.

**Development fallback (cwd-relative `.env`):** the project `.env` sets a
`SIMSAPA_DIR` relative to the **current working directory** (the project root,
where `make run` is invoked), not the exe directory
(`build/simsapadhammareader/`). To support both conventions,
`get_create_simsapa_dir()` resolves a relative value against the exe directory
first (portable), and if that path does **not** exist but the **cwd-relative**
path does, it falls back to the cwd-relative path. This keeps the dev workflow
working without breaking portable installs (whose exe-relative data folder is
created on first run when no cwd-relative match exists). Without this fallback,
a dev `SIMSAPA_DIR=../bootstrap-assets-resources/dist/simsapa` resolved
against `build/simsapadhammareader/` to a nonexistent
`build/bootstrap-assets-resources/...`, so the app saw no databases and showed
the download window.

On **first** portable launch the data folder exists but has no databases, so the
existing first-run/download flow downloads them into it. On **subsequent**
launches the existing asset-presence checks find them and load without
re-downloading.

## USB drive-letter robustness

Because `config.txt` uses a relative `SIMSAPA_DIR` resolved against the exe
directory, and the `.cmd` launcher resolves the exe via `%~dp0`, a portable
install copied to a USB stick keeps working when the drive mounts under a
different letter on another machine — no reconfiguration and no re-download.

## Removal

Portable mode registers no uninstaller. To remove it, delete the install folder,
the sibling `…Data` folder, and the launcher in the parent folder. The Standard
uninstaller (with its `%LOCALAPPDATA%` user-data prompt) is unchanged and applies
to Standard installs only.

## Tests

`backend/tests/test_portable_path_resolution.rs` covers `resolve_simsapa_dir`
and `normalize_lexically`: relative resolution against a given exe dir, lexical
`..` collapse, absolute pass-through, forward-slash handling, and the no-exe-dir
fallback. Run with `cd backend && cargo test --test test_portable_path_resolution`.

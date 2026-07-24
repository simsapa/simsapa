# Windows User Data Paths - Implementation Reference

This document explains how the Windows installer correctly determines the user data directory used by the Simsapa application.

## Backend Implementation (Rust)

### App Identity

**File:** `backend/src/lib.rs:45`

```rust
pub static APP_INFO: AppInfo = AppInfo{
    name: "simsapa",
    author: "profound-labs"
};
```

### User Data Directory Resolution

**File:** `backend/src/lib.rs:269-290`

```rust
pub fn get_create_simsapa_internal_app_root() -> Result<PathBuf, Box<dyn Error>> {
    // AppDataType::UserData on Windows resolves to:
    // %LOCALAPPDATA%\{author}\{name}
    let mut p = get_app_root(AppDataType::UserData, &APP_INFO)?;
    // ...
}
```

**File:** `backend/src/lib.rs:292-330`

```rust
pub fn get_create_simsapa_dir() -> Result<PathBuf, Box<dyn Error>> {
    // On desktop (including Windows), always use the internal app root
    if !is_mobile() {
        return Ok(internal_app_root);
    }
    // ...
}
```

## app_dirs2 Crate Behavior

The `app_dirs2` crate follows platform conventions:

### Windows Path Structure

```
AppDataType::UserData + AppInfo{name: "simsapa", author: "profound-labs"}
    ↓
%LOCALAPPDATA%\profound-labs\simsapa
```

**Typical resolved path:**
```
C:\Users\{username}\AppData\Local\profound-labs\simsapa
```

### Directory Contents

```
%LOCALAPPDATA%\profound-labs\simsapa\
├── app-assets/               # Application database + downloaded assets
│   ├── appdata.sqlite3       # Single application database (seeded content + user data)
│   ├── dpd.sqlite3           # DPD dictionary database
│   ├── dictionaries.sqlite3  # Other dictionaries
│   └── ...
├── log.txt                   # Application logs
└── storage-path.txt          # Only used on mobile platforms
```

**Note:** Earlier versions kept user-generated data (bookmarks, chants, imported books, app settings) in a separate `userdata.sqlite3` next to `appdata.sqlite3`. That split has been removed: all user-generated rows now live in `appdata.sqlite3` and are distinguished from seeded rows by the `is_user_added` column. Alpha testers with a pre-existing `userdata.sqlite3` are migrated automatically on first launch via the one-shot legacy bridge (see PROJECT_MAP.md → "One-Shot Legacy Userdata Bridge"), after which the stale file is removed on a subsequent startup.

## Installer Implementation (Inno Setup)

**File:** `simsapa-installer.iss`

### User Data Directory Function

```pascal
// Get the user data directory where app databases are stored
// Uses app_dirs2 crate convention: AppInfo{name: "simsapa", author: "profound-labs"}
// From backend/src/lib.rs:45: APP_INFO: AppInfo = AppInfo{name: "simsapa", author: "profound-labs"}
// From backend/src/lib.rs:274: get_app_root(AppDataType::UserData, &APP_INFO)
// On Windows, app_dirs2 creates: %LOCALAPPDATA%\{author}\{name}
// Result: %LOCALAPPDATA%\profound-labs\simsapa
function GetUserDataDir: String;
begin
  Result := ExpandConstant('{localappdata}\profound-labs\simsapa');
end;
```

### Uninstall Behavior

The installer's uninstall process:

1. **Always removes:** Application files in `C:\Program Files\Simsapa`
2. **Optional removal:** User data directory (requires user confirmation)

```pascal
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  UserDataDir: String;
begin
  if CurUninstallStep = usPostUninstall then
    if DeleteUserDataCheckbox.Checked then
      UserDataDir := GetUserDataDir;
      // Prompts user before deleting %LOCALAPPDATA%\profound-labs\simsapa
```

## Verification During Development

### Test User Data Path

After installing and running the application, verify the path:

1. Open File Explorer
2. Navigate to: `%LOCALAPPDATA%\profound-labs\simsapa`
   - Or paste in address bar: `C:\Users\YourUsername\AppData\Local\profound-labs\simsapa`
3. Confirm presence of:
   - `app-assets/` directory containing `appdata.sqlite3`
   - `logs/` directory

### Check Installer Behavior

1. Install the application
2. Download some language databases
3. Check that files are created in `%LOCALAPPDATA%\profound-labs\simsapa\app-assets\`
4. Uninstall the application
5. Verify that the uninstaller:
   - Shows the correct path in the checkbox
   - Only deletes user data if checkbox is selected

## Important Notes

### Path Consistency

The path **MUST** match exactly between:
- Rust backend: `APP_INFO` in `backend/src/lib.rs:45`
- Installer: `GetUserDataDir()` function in `simsapa-installer.iss`

### Do Not Change

Changing these values will cause the application to create a new user data directory, and existing users will lose access to their:
- Downloaded databases
- User preferences
- Bookmarks and notes

### Migration Consideration

If the app identity ever needs to change, implement a migration:
1. Detect old directory
2. Copy data to new directory
3. Optionally remove old directory

## Testing Checklist

- [ ] Fresh install creates user data at correct path
- [ ] Application can write to user data directory
- [ ] Downloaded databases appear in `app-assets/` subdirectory
- [ ] Logs appear in `logs/` subdirectory
- [ ] Uninstaller shows correct path in deletion checkbox
- [ ] Uninstaller can successfully delete user data when requested
- [ ] User data persists when "delete user data" is NOT checked

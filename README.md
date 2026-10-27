# Simsapa Dhamma Reader

A sutta reader and Pali dictionary application.

Linux:

![Contains Match Search on Linux](docs/contains-match-dpd-lookup-linux.png)

Android:

![Contains Match Search on Android](docs/contains-match-dpd-lookup-android.png)

## Command-line tool: `@simsapa/cli`

`simsapa` is the command-line version of the app's data engine: search the
suttas and the Pāli dictionaries from the terminal, gloss Pāli text, import
dictionaries and books, and serve the Simsapa localhost HTTP API, without the
desktop app. Every command prints plain text, or one JSON document with
`--json`, so it suits agents as well as people.

Install it from npm (Node.js 18 or later) and download the databases:

```sh
npm install -g @simsapa/cli
simsapa setup
```

Supported platforms: Linux x64 and arm64, macOS on Apple Silicon, and Windows
x64. The databases take about 4.8 GB on disk. The CLI uses the same data folder
as the desktop app, so if the app is already set up, there is no need to run
`simsapa setup`.

`simsapa --help` lists the commands, and `simsapa docs` prints the full CLI and
localhost API reference. See [npm/cli/README.md](npm/cli/README.md) for more.

## Building

### Prerequisites

- Qt 6.9.3
- Rust (latest stable)
- CMake 3.24+
- Platform-specific build tools

### Development Build

```bash
# Build the project
make build -B

# Run the application
make run
```

### Distribution Packages

#### Linux AppImage

```bash
# Build AppImage
make appimage -B

# Or with custom options
./build-appimage.sh --app-version v0.2.0 --os-suffix -ubuntu24

# Clean rebuild
make appimage-rebuild
```

#### macOS .app and DMG

```bash
# Build both .app bundle and .dmg
make macos -B

# Or build only .app bundle (skip DMG creation)
make macos-app

# Clean rebuild
make macos-rebuild
```

The macOS build will:
- Create a properly bundled .app in `./dist/Simsapa.app`
- Bundle all Qt frameworks and dependencies using `macdeployqt`
- Create a styled DMG installer: `Simsapa-vX.X.X-{arch}.dmg`

**Note:** For distribution, you may want to code sign the app. Set the `APPLE_SIGNING_IDENTITY` environment variable:

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAM_ID)"
make macos
```

#### Android APK

Build using Qt Creator with the Android kit configured.

### Testing

```bash
# Run all tests
make test

# Individual test suites
make rust-test   # Backend tests
make qml-test    # QML tests
make js-test     # JavaScript tests
```

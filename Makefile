all: run

# Detect platform and set Qt path for macOS
ifeq ($(shell uname),Darwin)
    QT_PATH ?= $(HOME)/Qt/6.9.3/macos
    BUILD_CMD = cmake -S . -B ./build/simsapadhammareader/ -DCMAKE_PREFIX_PATH=$(QT_PATH) && cmake --build ./build/simsapadhammareader/
    BUILD_OFFLINE_CMD = CARGO_NET_OFFLINE=true cmake -S . -B ./build/simsapadhammareader/ -DCMAKE_PREFIX_PATH=$(QT_PATH) -DFETCHCONTENT_UPDATES_DISCONNECTED=ON && cmake --build ./build/simsapadhammareader/
    RUN_CMD = ./build/simsapadhammareader/simsapadhammareader.app/Contents/MacOS/simsapadhammareader
else
    BUILD_CMD = cmake -S . -B ./build/simsapadhammareader/ && cmake --build ./build/simsapadhammareader/
    BUILD_OFFLINE_CMD = CARGO_NET_OFFLINE=true cmake -S . -B ./build/simsapadhammareader/ -DFETCHCONTENT_UPDATES_DISCONNECTED=ON && cmake --build ./build/simsapadhammareader/
    RUN_CMD = ./build/simsapadhammareader/simsapadhammareader
endif

# Fails if .claude/settings.json's literal Qt paths have drifted from
# CMakeLists.txt's QT_LINUX. See docs/qt-kit-selection.md.
qt-env-check:
	./scripts/qt-env-check.sh

build:
	$(BUILD_CMD)

build-offline:
	$(BUILD_OFFLINE_CMD)

run: build
	$(RUN_CMD)

sass:
	sass --no-source-map './assets/sass/:./assets/css/'

sass-watch:
	sass --no-source-map --watch './assets/sass/:./assets/css/'

parse-cips:
	cd cli && cargo run -- parse-cips-index --csv-path ../../src-lib/CIPS/src/data/general-index.csv --json-path ../assets/general-index.json --db-path ../../bootstrap-assets-resources/dist/simsapa/app-assets/appdata.sqlite3 --minify

count-code:
	tokei --types Rust,QML,C++,TypeScript,Javascript,CMake --compact --exclude assets/qml/data/ --exclude assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml --exclude assets/js/simsapa.min.js --exclude assets/js/vendor/ --exclude assets/pdf-viewer/ --exclude assets/dpd-res/ --exclude backend/src/lookup.rs --exclude "*/tests/" --exclude "tst_*.qml" . | grep -vE '===|---'

count-code-pie:
	tokei -o json --types Rust,QML,C++,TypeScript,Javascript,CMake --exclude assets/qml/data/ --exclude assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml --exclude assets/js/simsapa.min.js --exclude assets/js/vendor/ --exclude assets/pdf-viewer/ --exclude assets/dpd-res/ --exclude backend/src/lookup.rs --exclude "*/tests/" --exclude "tst_*.qml" . | tokei-pie

simsapa.min.js:
	npx webpack

test: rust-test qml-test js-test

# NOTE: Running 'cargo test' in 'bridges/' doesn't compile, but there are no tests there anyway.
# error: linking with `cc` failed

rust-test:
	cd backend && cargo test && cd ../cli && cargo test

js-test:
	npm test

# qml-test-one:
# 	env QT_QPA_PLATFORM=offscreen qmltestrunner -import ./assets/qml/ -input ./assets/qml/ -functions 'CommonWords::test_clean_stem'

qml-test:
	env QT_QPA_PLATFORM=offscreen qmltestrunner -import ./assets/qml/ -input ./assets/qml/

project-tree:
	tree --gitignore --dirsfirst -I docs/ -I CMakeLists.txt.user -I res/ -I gradle/ -I vendor/ -I dpd-res/ -I fonts/ -I icons/ -I scripts/ -I package-lock.json -I Cargo.lock -o project_tree.txt

bootstrap:
	cd cli/ && cargo build && cargo run -- bootstrap --write-new-dotenv

# First 'cargo clean' on each module, then build.
cargo-clean-build:
	cd backend && cargo clean && cd ../bridges && cargo clean && cd ../cli && cargo clean && cd .. && \
	cd backend && cargo build && cd ../bridges && cargo build && cd ../cli && cargo build && cd ..

appimage: build
	./build-appimage.sh

appimage-clean:
	rm -rf Simsapa.AppDir appimage-tools Simsapa-*.AppImage

appimage-rebuild: appimage-clean
	./build-appimage.sh --clean --force-download

macos: build
	./build-macos.sh

macos-app: build
	./build-macos.sh --skip-dmg

macos-clean:
	rm -rf ./dist Simsapa-*.dmg

macos-rebuild: macos-clean
	./build-macos.sh --clean

# --- Android ---------------------------------------------------------------
#
# Multi-ABI signed packages. Do NOT build release packages from the Qt Creator
# interface: its kits are single-ABI, and an arm64-only bundle is filtered out
# of the Play Store on Intel/AMD Chromebooks.
#
# Signing credentials come from the gitignored android/signing.env
# (see android/signing.env.example).
#
# Google Play requires a strictly increasing versionCode on every upload. Bump
# it by editing android/version.txt — the release is then just:
#
#   make android-aab
#
# build-android.sh reads the versionCode from android/version.txt and the
# versionName from the [package] version in bridges/Cargo.toml. The exports
# below stay so that an explicit, NON-EMPTY override still wins:
#
#   make android-aab ANDROID_VERSION_CODE=99
#
# Full design: docs/android-multi-abi-and-chromeos.md

ANDROID_ABIS ?= arm64-v8a;x86_64;armeabi-v7a
ANDROID_BUILD_DIR ?= build/android-multiabi
export ANDROID_ABIS
export ANDROID_BUILD_DIR
export ANDROID_VERSION_CODE
export ANDROID_VERSION_NAME

# Signed App Bundle for Google Play.
android-aab:
	./build-android.sh --aab

# Signed APK for sideloading. Note that a Qt Creator *deploy* of this same APK
# triggers the spurious "This app isn't 16 KB compatible" dialog while a plain
# sideload does not — the warning is gated on the install path, not the
# contents. See CLAUDE.md.
android-apk:
	./build-android.sh --apk

# Unsigned debug APK for local testing.
android-apk-debug:
	./build-android.sh --apk --debug

# --- Beta package ----------------------------------------------------------
#
# The beta package has its own application id — io.github.simsapa.app.beta,
# label "Simsapa (beta)" — so it installs ALONGSIDE the released app instead of
# replacing it. Both are signed with the release keystore.
#
# Why a separate id: a copy installed from Google Play is signed by Play App
# Signing (Google's key, not our upload key), and Android has no key-swap path,
# so a locally built package can NEVER replace a Play install. Without the
# suffix the only way to test a local build on a device carrying the Play build
# is to uninstall it, wiping its app-private data including the downloaded
# appdata. With the suffix both coexist — at the cost of the beta install
# running first-time asset setup of its own.
#
# Two variants of that same package:
#
#   android-beta-debug   debuggable, for local testing with adb logcat.
#                        NEVER distribute it — a debuggable package lets
#                        anything with adb access read its private data and
#                        attach a debugger.
#   android-beta-dist    not debuggable, for GitHub Releases.
#
# They share the id, so one replaces the other on a device; versionName tells
# them apart ("-beta-debug" vs "-beta"). The suffixes live in
# android/build.gradle and android/AndroidManifest.beta.xml, not here.

APK_BETA_DEBUG := $(ANDROID_BUILD_DIR)/android-build/build/outputs/apk/debug/android-build-debug.apk
ANDROID_BETA_PKG := io.github.simsapa.app.beta

android-beta-debug:
	./build-android.sh --apk --debug --sign

# Installs the debuggable beta alongside whatever release build is on the device.
android-beta-debug-install:
	@test -f "$(APK_BETA_DEBUG)" || { echo "Not built yet: $(APK_BETA_DEBUG) — run 'make android-beta-debug'"; exit 1; }
	adb install -r "$(APK_BETA_DEBUG)"

# Launches the beta app and streams its log messages to this console — the same
# output Qt Creator shows in its "Application Output" pane, which is a filtered
# logcat and nothing more.
#
# Tags: `simsapa` is the Rust backend (android_logger, set in
# backend/src/logger.rs), `Qt`/`QtCore`/`QtQml` are Qt's own message handler,
# which is also where the QML Logger module's output arrives. AndroidRuntime
# and DEBUG carry Java exceptions and native crash traces.
#
# Filtering by tag rather than by pid deliberately: a pid filter cannot be
# established until the process exists, which loses the startup messages.
# Ctrl-C to stop; the app keeps running.
android-beta-debug-run:
	adb logcat -c
	adb shell monkey -p $(ANDROID_BETA_PKG) -c android.intent.category.LAUNCHER 1 >/dev/null
	adb logcat -v brief simsapa:V Qt:V QtCore:V QtQml:V AndroidRuntime:E DEBUG:E '*:S'

# The distributable beta: release build type, NOT debuggable, release-signed.
# Copied out under a version-stamped name ready to attach to a GitHub release.
#
# Note dist/ is shared with the macOS packaging, and `make macos-clean` does
# `rm -rf ./dist` — so it will take the beta APK with it. Re-run this target.
android-beta-dist:
	./build-android.sh --apk --beta
	@mkdir -p dist
	@version=$$(sed -n '/^\[package\]/,/^\[/ s/^version *= *"\([^"]*\)".*/\1/p' bridges/Cargo.toml | head -1); \
	  src="$(ANDROID_BUILD_DIR)/android-build/build/outputs/apk/release/android-build-release-signed.apk"; \
	  test -f "$$src" || { echo "Not found: $$src"; exit 1; }; \
	  cp "$$src" "dist/Simsapa-$$version-beta.apk"; \
	  echo "==> dist/Simsapa-$$version-beta.apk"

# Removes the WHOLE build directory. Do not hand-delete just android-build/ —
# the per-ABI copy steps are driven by ExternalProject stamps in the sub-build
# trees, so they would then consider themselves up to date and never repopulate
# the staging directory, wedging the build permanently.
android-clean:
	rm -rf $(ANDROID_BUILD_DIR)

# The release path.
android-rebuild: android-clean android-aab

windows:
	powershell -ExecutionPolicy Bypass -File build-windows.ps1

windows-clean:
	powershell -Command "if (Test-Path './build/simsapadhammareader') { Remove-Item -Recurse -Force './build/simsapadhammareader' }; if (Test-Path './dist') { Remove-Item -Recurse -Force './dist' }; if (Test-Path 'Simsapa-Setup-*.exe') { Remove-Item -Force 'Simsapa-Setup-*.exe' }"

windows-rebuild: windows-clean
	powershell -ExecutionPolicy Bypass -File build-windows.ps1 -Clean

SNOWBALL_COMPILER := pali-stemmer-in-snowball/assets/snowball/snowball
SNOWBALL_ALGO_DIR := pali-stemmer-in-snowball/assets/snowball/algorithms
PALI_SBL := pali-stemmer-in-snowball/algorithms/pali.sbl
STEMMER_OUT_DIR := backend/src/snowball/algorithms

# All .sbl files excluding legacy/variant stemmers and pali (handled separately)
SNOWBALL_SBLS := $(filter-out $(SNOWBALL_ALGO_DIR)/dutch_porter.sbl $(SNOWBALL_ALGO_DIR)/lovins.sbl $(SNOWBALL_ALGO_DIR)/porter.sbl $(SNOWBALL_ALGO_DIR)/pali.sbl, $(wildcard $(SNOWBALL_ALGO_DIR)/*.sbl))

compile-stemmers:
	@echo "Compiling Snowball stemmers to Rust..."
	@mkdir -p $(STEMMER_OUT_DIR)
	@for sbl in $(SNOWBALL_SBLS); do \
		lang=$$(basename $$sbl .sbl); \
		echo "  $$lang"; \
		$(SNOWBALL_COMPILER) $$sbl -rust -o $(STEMMER_OUT_DIR)/$${lang}_stemmer; \
		sed -i 's/use snowball::/use crate::snowball::/g' $(STEMMER_OUT_DIR)/$${lang}_stemmer.rs; \
	done
	@echo "  pali"
	@$(SNOWBALL_COMPILER) $(PALI_SBL) -rust -o $(STEMMER_OUT_DIR)/pali_stemmer
	@sed -i 's/use snowball::/use crate::snowball::/g' $(STEMMER_OUT_DIR)/pali_stemmer.rs
	@echo "Done. Generated stemmers in $(STEMMER_OUT_DIR)/"

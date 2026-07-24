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

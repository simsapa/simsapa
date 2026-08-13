use std::env;
use cxx_qt_build::{CxxQtBuilder, QmlModule};
use qt_build_utils::{QResource, QResourceFile, QResources};

const QML_MODULE_URI: &str = "com.profoundlabs.simsapa";

fn main() {
    let s = match env::var("CXX_QT_QT_MODULES") {
        Ok(s) => s,
        Err(_) => "".to_string(),
    };
    let mobile_build = s.contains("Qt::WebView");

    let qml_files = vec![
        "../assets/qml/SuttaSearchWindow.qml",
        "../assets/qml/DownloadAppdataWindow.qml",
        "../assets/qml/SuttaLanguagesWindow.qml",
        "../assets/qml/LibraryWindow.qml",
        "../assets/qml/ReferenceSearchWindow.qml",
        "../assets/qml/ReferenceSearchInfoDialog.qml",
        "../assets/qml/TopicIndexWindow.qml",
        "../assets/qml/TopicIndexInfoDialog.qml",
        "../assets/qml/TopicIndexUpdateWindow.qml",
        "../assets/qml/BooksList.qml",
        "../assets/qml/ChapterListItem.qml",
        "../assets/qml/DocumentImportDialog.qml",
        "../assets/qml/DocumentMetadataEditDialog.qml",
        "../assets/qml/LanguageListSelector.qml",
        "../assets/qml/DownloadProgressFrame.qml",
        "../assets/qml/SearchBarInput.qml",
        "../assets/qml/MobileKeyboardHelper.qml",
        "../assets/qml/DialogHeader.qml",
        "../assets/qml/MobileOverlayTracker.qml",
        "../assets/qml/FulltextResults.qml",
        "../assets/qml/CMenuItem.qml",
        "../assets/qml/KeySequenceDisplay.qml",
        "../assets/qml/SuttaTabButton.qml",
        "../assets/qml/TabListDialog.qml",
        "../assets/qml/WindowListDialog.qml",
        "../assets/qml/SuttaHtmlView.qml",
        "../assets/qml/WebEngineRepaintNudge.qml",
        "../assets/qml/SuttaHtmlView_Desktop.qml",
        "../assets/qml/SuttaHtmlView_Mobile.qml",
        "../assets/qml/DictionaryHtmlView.qml",
        "../assets/qml/DictionaryHtmlView_Desktop.qml",
        "../assets/qml/DictionaryHtmlView_Mobile.qml",
        "../assets/qml/DictionaryTab.qml",
        "../assets/qml/TocTab.qml",
        "../assets/qml/QueryTab.qml",
        "../assets/qml/SuttaStackLayout.qml",
        "../assets/qml/AboutDialog.qml",
        "../assets/qml/DatabaseValidationDialog.qml",
        "../assets/qml/StorageDiagnosticsDialog.qml",
        "../assets/qml/DhammaTextSourcesDialog.qml",
        "../assets/qml/SearchHelpWindow.qml",
        "../assets/qml/SystemPromptsDialog.qml",
        "../assets/qml/ModelsDialog.qml",
        "../assets/qml/ModelUsageLists.qml",
        "../assets/qml/AiErrorUtils.qml",
        "../assets/qml/AiResponseCoordinator.qml",
        "../assets/qml/AnkiExportDialog.qml",
        "../assets/qml/AppSettingsWindow.qml",
        "../assets/qml/DrawerMenu.qml",
        "../assets/qml/DrawerEmptyItem.qml",
        "../assets/qml/ListBackground.qml",
        "../assets/qml/WordSummary.qml",
        "../assets/qml/DeconstructorSelector.qml",
        "../assets/qml/DeconstructorUtils.qml",
        "../assets/qml/StorageCandidatesList.qml",
        "../assets/qml/StorageDialog.qml",
        "../assets/qml/StorageRecoveryWindow.qml",
        "../assets/qml/GlossTab.qml",
        "../assets/qml/GlossWordSelectionDialog.qml",
        "../assets/qml/PromptsTab.qml",
        "../assets/qml/AssistantResponses.qml",
        "../assets/qml/ResponseTabButton.qml",
        "../assets/qml/ScrollableHelper.qml",
        "../assets/qml/ThemeHelper.qml",
        "../assets/qml/Logger.qml",
        "../assets/qml/UnrecognizedWordsList.qml",
        "../assets/qml/UpdateNotificationDialog.qml",
        "../assets/qml/KeybindingCaptureDialog.qml",
        "../assets/qml/ShortcutConflictDialog.qml",
        "../assets/qml/ChantingPracticeWindow.qml",
        "../assets/qml/ChantingPracticeReviewWindow.qml",
        "../assets/qml/ChantingTreeList.qml",
        "../assets/qml/RecordingPlaybackItem.qml",
        "../assets/qml/WaveformView.qml",
        "../assets/qml/BookmarksTab.qml",
        "../assets/qml/BookmarkFolderItem.qml",
        "../assets/qml/BookmarkListItem.qml",
        "../assets/qml/HistoryListItem.qml",
        "../assets/qml/HistoryUtils.qml",
        "../assets/qml/BookmarkEditDialog.qml",
        "../assets/qml/BookmarkFolderDialog.qml",
        "../assets/qml/DictionaryIndexProgressWindow.qml",
        "../assets/qml/DictionariesWindow.qml",
        "../assets/qml/DictionaryListItem.qml",
        "../assets/qml/DictionaryImportRow.qml",
        "../assets/qml/DictionaryImportDialog.qml",
        "../assets/qml/DictionaryEditDialog.qml",
        "../assets/qml/DictionarySearchDictionariesPanel.qml",
        "../assets/qml/DictionaryInfoDialog.qml",
        "../assets/qml/GlobalHotkeysSection.qml",
        "../assets/qml/GlobalHotkeysWaylandNote.qml",
    ];

    // The QML files above are registered as plain Qt resources here, with an
    // explicitly derived alias, rather than being passed to the QML module as
    // its `qml_files`. Both halves of that are deliberate.
    //
    // cxx-qt feeds a `qml_files` path string, verbatim, into three separate
    // derivations that disagree about a leading `../`:
    //
    //   rcc alias         `../assets/qml/Logger.qml` -- rcc folds the `..` away,
    //                     so the file really lands at
    //                     :/qt/qml/com/profoundlabs/simsapa/assets/qml/Logger.qml
    //   qmldir component  `Logger 1.0 ../assets/qml/Logger.qml` -- resolved as a
    //                     URL against the module directory, so it points one
    //                     level too high, at com/profoundlabs/assets/qml/
    //   qmlcachegen       `--resource-path /qt/qml/<uri>/../assets/qml/Logger.qml`
    //                     -- inserted into the loader table unnormalized, while
    //                     the loader looks up through QDir::cleanPath, so a key
    //                     containing `/../` can never be matched
    //
    // Under cxx-qt 0.7 the generated qmldir carried no component lines, so type
    // lookup fell through to implicit same-directory resolution and the
    // mismatch was invisible. 0.8's "correct QML module export" made the broken
    // entry authoritative, and every lookup through the module then failed at
    // runtime: "Type Logger unavailable --
    // qrc:/qt/qml/com/profoundlabs/assets/qml/Logger.qml: No such file".
    //
    // Registering the files here keeps every resource path byte-identical to
    // what the rest of the codebase hardcodes (the
    // qrc:/qt/qml/com/profoundlabs/simsapa/assets/qml/*.qml literals in cpp/),
    // and restores the 0.7 semantics that ship today: all QML files land in one
    // resource directory and resolve their neighbours implicitly, which is why
    // Logger.qml needs no import.
    //
    // Consequence: qmlcachegen does not run, so QML is parsed from source at
    // load time. That is not a regression -- per the third bullet above, the AOT
    // cache has never once been consulted in this project, under 0.7 or 0.9, so
    // its 88 compiled units were dead weight in the binary. Enabling it for real
    // is a separate, measured change; it requires `..`-free paths, i.e. moving
    // assets/qml/ under bridges/. See docs/cxx-qt-fork.md.
    let qml_resources = QResources::new().resource(
        QResource::new()
            // Set explicitly rather than relying on qrc_resources() applying the
            // QML module's prefix implicitly, so the resource path this file
            // produces is greppable here -- an invisible path derivation is what
            // caused the bug described above.
            .prefix(format!("/qt/qml/{}", QML_MODULE_URI.replace('.', "/")))
            .files(qml_files.iter().map(|path| {
                let path = *path;
                // The alias becomes the resource path, so it must not contain
                // `..`. Deriving it here means the list above keeps its usual
                // "../assets/qml/<Name>.qml" form and a malformed entry fails
                // the build instead of failing when that screen is first shown.
                let alias = path.strip_prefix("../").unwrap_or_else(|| {
                    panic!(
                        "QML file paths must be written relative to bridges/ as \
                         \"../assets/qml/<Name>.qml\"; got \"{path}\""
                    )
                });
                QResourceFile::new(path).alias(alias)
            })),
    );

    // Since cxx-qt 0.8 a QML module carries only its QML files; the Rust bridge
    // sources move to CxxQtBuilder::files(), and there may be only one QML module
    // per builder. CxxQtBuilder::files() panics if the sources span more than one
    // directory (Qt bug QTBUG-93443) -- all nine bridges are under src/.
    let builder = CxxQtBuilder::new_qml_module(QmlModule::new(QML_MODULE_URI))
        .qrc_resources(qml_resources)
        // Link Qt's Network library
        // - Qt Core is always linked
        // - Qt Gui is linked by enabling the qt_gui Cargo feature of cxx-qt-lib.
        // - Qt Qml is linked by enabling the qt_qml Cargo feature of cxx-qt-lib.
        // - Qt Qml requires linking Qt Network on macOS
        .qt_module("Network")
        .qt_module("Widgets")
        .qt_module("Quick")
        .files([
            "src/api.rs",
            "src/sutta_bridge.rs",
            "src/asset_manager.rs",
            "src/audio_manager.rs",
            "src/storage_manager.rs",
            "src/prompt_manager.rs",
            "src/clipboard_manager.rs",
            "src/dictionary_manager.rs",
            "src/global_hotkey_manager.rs",
        ])
        // The cc_builder() closure is `unsafe fn` since 0.9. It is not needed
        // here: include_dir() and cpp_files() are the safe equivalents of the
        // cc.include() / cc.file() calls this used to make. cpp_files() compiles
        // non-header files and does not run moc over them, matching cc.file().
        .include_dir("../cpp/")
        .cpp_files([
            "../cpp/utils.cpp",
            "../cpp/system_palette.cpp",
            "../cpp/gui.cpp",
        ]);

    if mobile_build {
        builder.qt_module("WebView").build();
    } else {
        builder.qt_module("WebEngineQuick").build();
    }
}

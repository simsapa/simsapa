use std::env;
use cxx_qt_build::{CxxQtBuilder, QmlModule};

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
        "../assets/qml/BooksList.qml",
        "../assets/qml/ChapterListItem.qml",
        "../assets/qml/DocumentImportDialog.qml",
        "../assets/qml/DocumentMetadataEditDialog.qml",
        "../assets/qml/LanguageListSelector.qml",
        "../assets/qml/DownloadProgressFrame.qml",
        "../assets/qml/SearchBarInput.qml",
        "../assets/qml/MobileKeyboardHelper.qml",
        "../assets/qml/FulltextResults.qml",
        "../assets/qml/CMenuItem.qml",
        "../assets/qml/KeySequenceDisplay.qml",
        "../assets/qml/SuttaTabButton.qml",
        "../assets/qml/TabListDialog.qml",
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

    // Since cxx-qt 0.8 a QML module carries only its QML files; the Rust bridge
    // sources move to CxxQtBuilder::files(), and there may be only one QML module
    // per builder. CxxQtBuilder::files() panics if the sources span more than one
    // directory (Qt bug QTBUG-93443) -- all nine bridges are under src/.
    let builder = CxxQtBuilder::new_qml_module(
            QmlModule::new("com.profoundlabs.simsapa").qml_files(qml_files),
        )
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

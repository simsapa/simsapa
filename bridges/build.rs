use std::env;
use cxx_qt_build::{CxxQtBuilder, QmlModule};
// use cxx_qt_build::{is_ios_target, thin_generated_fat_library_with_lipo};

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
        "../assets/qml/StorageDialog.qml",
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

    let builder = CxxQtBuilder::new()
        // Link Qt's Network library
        // - Qt Core is always linked
        // - Qt Gui is linked by enabling the qt_gui Cargo feature of cxx-qt-lib.
        // - Qt Qml is linked by enabling the qt_qml Cargo feature of cxx-qt-lib.
        // - Qt Qml requires linking Qt Network on macOS
        .qt_module("Network")
        .qt_module("Widgets")
        .qt_module("Quick")
        .qml_module(QmlModule {
                uri: "com.profoundlabs.simsapa",
                rust_files: &[
                        "src/api.rs",
                        "src/sutta_bridge.rs",
                        "src/asset_manager.rs",
                        "src/audio_manager.rs",
                        "src/storage_manager.rs",
                        "src/prompt_manager.rs",
                        "src/clipboard_manager.rs",
                        "src/dictionary_manager.rs",
                        "src/global_hotkey_manager.rs",
                ],
                qml_files: &qml_files,
                ..Default::default()
        })
        .cc_builder(|cc| {
            // Add include directory for custom headers
            cc.include("../cpp/");
            cc.file("../cpp/utils.cpp");
            cc.file("../cpp/system_palette.cpp");
            cc.file("../cpp/gui.cpp");
        });

    if mobile_build {
        builder.qt_module("WebView").build();
    } else {
        builder.qt_module("WebEngineQuick").build();
    }

    // if is_ios_target() {
    //     thin_generated_fat_library_with_lipo("libsimsapa_bridges-cxxqt-generated.a", "arm64");
    // }
}

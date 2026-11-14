import QtQuick
import QtQuick.Dialogs

import com.profoundlabs.simsapa

// "Clear Word-Selection Cache": confirms, then deletes the local tier of the
// gloss word-selection cache (the ai-selected and user-selected rows). The
// built-in rows and the phrase table are kept. Used by the AI word selection
// dialog and the Gloss Settings window, each declaring its own so the
// confirmation opens in front of that window. Open it with ask().
MessageDialog {
    id: root
    property int cache_count: 0
    title: "Clear Word-Selection Cache"
    text: "Delete all " + root.cache_count + " cached word selections (AI-selected and user-confirmed)? Built-in entries shipped with the app are kept."
    buttons: MessageDialog.Cancel | MessageDialog.Ok

    Logger { id: logger }

    function ask() {
        root.cache_count = SuttaBridge.gloss_word_cache_count();
        root.open();
    }

    onAccepted: {
        if (!SuttaBridge.clear_gloss_word_cache()) {
            logger.error("Failed to clear the gloss word-selection cache");
        }
    }
}

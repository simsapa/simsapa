#ifndef GUI_H
#define GUI_H

#include <QString>

int start(int argc, char* argv[]);

extern "C++" {
    void callback_run_lookup_query(QString query_text = "");
    void callback_run_summary_query(QString window_id, QString query_text = "");
    void callback_run_sutta_menu_action(QString window_id, QString action, QString query_text = "");
    void callback_run_dppn_dictionary_query(QString window_id, QString query);
    void callback_run_combined_dictionary_query(QString window_id, QString query);
    void callback_open_sutta_search_window(QString show_result_data_json = "");
    void callback_open_sutta_tab(QString window_id, QString show_result_data_json = "");
    void callback_open_sutta_languages_window();
    void callback_open_dictionaries_window();
    void callback_open_library_window();
    void callback_open_reference_search_window();
    void callback_open_topic_index_window();
    void callback_show_chapter_in_sutta_window(QString window_id, QString result_data_json);
    void callback_show_sutta_from_reference_search(QString window_id, QString result_data_json);
    void callback_toggle_reading_mode(QString window_id, bool is_active);
    void callback_open_in_lookup_window(QString result_data_json);
    void callback_open_chanting_practice_window(QString window_id);
    void callback_open_chanting_review_window(QString window_id, QString section_uid);
    /// Connected to GlobalHotkeyManager::hotkeyActivated.
    void callback_global_hotkey_activated(int handle);
}

void open_sutta_search_window(QString query_text = "");

#endif

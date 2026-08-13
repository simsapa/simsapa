#ifndef WINDOW_MANAGER_H
#define WINDOW_MANAGER_H

#include <QObject>
#include <QString>
#include <QList>
#include <QMainWindow>
#include <QVariantMap>

class SuttaSearchWindow;
class DownloadAppdataWindow;
class StorageRecoveryWindow;
class SuttaLanguagesWindow;
class DictionariesWindow;
class LibraryWindow;
class ReferenceSearchWindow;
class TopicIndexWindow;
class ChantingPracticeWindow;
class ChantingReviewWindow;

class WindowManager : public QObject {
        Q_OBJECT
    public:
        static WindowManager& instance(QApplication* app);
        static void lookup_word(const QString& word);

        void create_plain_sutta_search_window();
        SuttaSearchWindow* create_sutta_search_window();

        /// Closing a SuttaSearchWindow only hides it — the object stays in
        /// sutta_search_windows (same assumption the aboutToQuit session-save in
        /// gui.cpp makes when it skips windows whose root is not `visible`).
        /// These pick the newest / oldest window the user still has open, so a
        /// window_id-less dispatch never targets a closed one. They fall back to
        /// last()/first() when nothing is open, so the request still lands
        /// somewhere (re-showing a closed window) instead of being dropped.
        SuttaSearchWindow* last_open_sutta_search_window();
        SuttaSearchWindow* first_open_sutta_search_window();
        SuttaSearchWindow* take_closed_sutta_search_window();
        void restore_last_session();
        DownloadAppdataWindow* create_download_appdata_window(
            const QVariantMap& initial_properties = QVariantMap());
        StorageRecoveryWindow* create_storage_recovery_window();
        SuttaLanguagesWindow* create_sutta_languages_window();
        DictionariesWindow* create_dictionaries_window();
        LibraryWindow* create_library_window();
        ReferenceSearchWindow* create_reference_search_window();
        TopicIndexWindow* create_topic_index_window();
        ChantingPracticeWindow* create_chanting_practice_window(const QString& window_id);
        ChantingReviewWindow* create_chanting_review_window(const QString& window_id, const QString& section_uid);

        /// Destroy the single-instance secondary window of the given type, after
        /// QML has accepted its close. Called from the QML onClosing handlers via
        /// SuttaBridge.notify_window_closed().
        void on_window_closed(const QString& window_type);

        static WindowManager *m_instance;
        QApplication* m_app;
        int m_window_id_count;
        QList<SuttaSearchWindow*> sutta_search_windows;
        QList<DownloadAppdataWindow*> download_appdata_windows;
        QList<StorageRecoveryWindow*> storage_recovery_windows;
        QList<SuttaLanguagesWindow*> sutta_languages_windows;
        QList<DictionariesWindow*> dictionaries_windows;
        QList<LibraryWindow*> library_windows;
        QList<ReferenceSearchWindow*> reference_search_windows;
        QList<TopicIndexWindow*> topic_index_windows;
        QList<ChantingPracticeWindow*> chanting_practice_windows;
        QList<ChantingReviewWindow*> chanting_review_windows;

    private:
        WindowManager(QApplication* app, QObject *parent = nullptr);
        ~WindowManager();

    signals:
        void signal_run_lookup_query(const QString& query_text);
        void signal_run_summary_query(const QString& window_id, const QString& query_text);
        void signal_run_sutta_menu_action(const QString& window_id, const QString& action, const QString& query_text);
        void signal_run_dppn_dictionary_query(const QString& window_id, const QString& query);
        void signal_run_combined_dictionary_query(const QString& window_id, const QString& query);
        void signal_open_sutta_search_window(const QString& show_result_data_json);
        void signal_open_sutta_tab(const QString& window_id, const QString& show_result_data_json);
        void signal_toggle_reading_mode(const QString& window_id, bool is_active);
        void signal_open_in_lookup_window(const QString& result_data_json);

    public slots:
        void run_lookup_query(const QString& query_text);
        void run_summary_query(const QString& window_id, const QString& query_text);
        void run_sutta_menu_action(const QString& window_id, const QString& action, const QString& query_text);
        void run_dppn_dictionary_query(const QString& window_id, const QString& query);
        void run_combined_dictionary_query(const QString& window_id, const QString& query);
        void open_sutta_search_window_with_query(const QString& show_result_data_json);
        void open_sutta_tab_in_window(const QString& window_id, const QString& show_result_data_json);
        void show_chapter_in_sutta_window(const QString& window_id, const QString& result_data_json);
        void show_sutta_from_reference_search(const QString& window_id, const QString& result_data_json);
        void toggle_reading_mode(const QString& window_id, bool is_active);
        void open_in_lookup_window(const QString& result_data_json);

};

#endif

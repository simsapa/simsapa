from functools import partial
import json, queue
from typing import Any, Callable, List, Optional
from pathlib import Path

from PyQt6.QtCore import QTimer, pyqtSignal
from PyQt6.QtGui import QCloseEvent, QAction
from PyQt6.QtWidgets import (QHBoxLayout, QListWidget, QMessageBox, QTabWidget, QVBoxLayout)

from simsapa import logger, ApiAction, ApiMessage, SearchResult, APP_QUEUES, GRAPHS_DIR, TIMER_SPEED
from simsapa.assets.ui.sutta_search_window_ui import Ui_SuttaSearchWindow

from simsapa.app.app_data import AppData
from simsapa.app.types import USutta

from simsapa.layouts.gui_types import SuttaSearchWindowInterface
from simsapa.layouts.preview_window import PreviewWindow
from simsapa.layouts.help_info import show_search_info
from simsapa.layouts.sutta_search_window_state import SuttaSearchWindowState

from simsapa.layouts.parts.memos_sidebar import HasMemosSidebar
from simsapa.layouts.parts.links_sidebar import HasLinksSidebar
from simsapa.layouts.parts.fulltext_list import HasFulltextList

class SuttaSearchWindow(SuttaSearchWindowInterface, Ui_SuttaSearchWindow, HasLinksSidebar,
                        HasMemosSidebar, HasFulltextList):

    searchbar_layout: QHBoxLayout
    sutta_tabs_layout: QVBoxLayout
    tabs_layout: QVBoxLayout
    selected_info: Any
    recent_list: QListWidget
    s: SuttaSearchWindowState
    fulltext_results_tab_idx: int = 0
    rightside_tabs: QTabWidget
    _app_data: AppData
    _show_sutta: Callable

    lookup_in_dictionary_signal = pyqtSignal(str)
    graph_link_mouseover = pyqtSignal(dict)
    lookup_in_new_sutta_window_signal = pyqtSignal(str)

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)
        logger.info("SuttaSearchWindow::__init__()")

        self._app_data = app_data

        self.queue_id = 'window_' + str(len(APP_QUEUES))
        APP_QUEUES[self.queue_id] = queue.Queue()
        self.messages_url = f'{self._app_data.api_url}/queues/{self.queue_id}'

        self.selected_info = {}

        self.graph_path: Path = GRAPHS_DIR.joinpath(f"{self.queue_id}.html")

        self.timer = QTimer()
        self.timer.timeout.connect(self.handle_messages)
        self.timer.start(TIMER_SPEED)

        self._setup_ui()

        self.s = SuttaSearchWindowState(app_data,
                                        self,
                                        self.searchbar_layout,
                                        self.sutta_tabs_layout,
                                        self.tabs_layout,
                                        enable_nav_buttons=False)


        self.page_len = self.s.page_len

        self._connect_signals()

        self.init_fulltext_list()
        self.init_memos_sidebar()
        self.init_links_sidebar()

        logger.profile("SuttaSearchWindow::__init__() end")

    def _setup_ui(self):
        self.links_tab_idx = 1
        self.memos_tab_idx = 2

        show = self._app_data.app_settings.get('show_sutta_sidebar', True)
        self.action_Show_Sidebar.setChecked(show)

        if show:
            self.splitter.setSizes([2000, 2000])
        else:
            self.splitter.setSizes([2000, 0])

        show = self._app_data.app_settings.get('show_related_suttas', True)
        self.action_Show_Related_Suttas.setChecked(show)

    def closeEvent(self, event: QCloseEvent):
        if self.queue_id in APP_QUEUES.keys():
            del APP_QUEUES[self.queue_id]

        msg = ApiMessage(queue_id = 'app_windows',
                         action = ApiAction.remove_closed_window_from_list,
                         data = self.queue_id)
        s = json.dumps(msg)
        APP_QUEUES['app_windows'].put_nowait(s)

        if self.graph_path.exists():
            self.graph_path.unlink()

        event.accept()

    def handle_messages(self):
        if self.queue_id in APP_QUEUES.keys():
            try:
                s = APP_QUEUES[self.queue_id].get_nowait()
                msg: ApiMessage = json.loads(s)
                if msg['action'] == ApiAction.show_sutta:
                    info = json.loads(msg['data'])
                    self.s._show_sutta_from_message(info)

                elif msg['action'] == ApiAction.show_sutta_by_uid:
                    info = json.loads(msg['data'])
                    if 'uid' in info.keys():
                        self.s._show_sutta_by_uid(info['uid'])

                elif msg['action'] == ApiAction.show_word_by_uid:
                    info = json.loads(msg['data'])
                    if 'uid' in info.keys():
                        self.s._show_word_by_uid(info['uid'])

                elif msg['action'] == ApiAction.lookup_clipboard_in_suttas:
                    self._lookup_clipboard_in_suttas()

                elif msg['action'] == ApiAction.lookup_in_suttas:
                    text = msg['data']
                    self.s._set_query(text)
                    self.s._handle_query()

                elif msg['action'] == ApiAction.set_selected:
                    info = json.loads(msg['data'])
                    self.selected_info = info

                APP_QUEUES[self.queue_id].task_done()
            except queue.Empty:
                pass

    def _lookup_clipboard_in_suttas(self):
        self.activateWindow()
        s = self._app_data.clipboard_getText()
        if s is not None:
            self.s._set_query(s)
            self.s._handle_query()

    def _lookup_clipboard_in_dictionary(self):
        text = self._app_data.clipboard_getText()
        if text is not None:
            self.lookup_in_dictionary_signal.emit(text)

    def results_page(self, page_num: int) -> List[SearchResult]:
        return self.s._queries.results_page(page_num)

    def query_hits(self) -> Optional[int]:
        return self.s._queries.query_hits()

    def result_pages_count(self) -> Optional[int]:
        return self.s._queries.result_pages_count()

    def hide_network_graph(self):
        self.hide_links_graph()
        self.set_links_tab_text("Links")

    def show_network_graph(self, sutta: Optional[USutta] = None, show_anyway = False):
        if show_anyway:
            is_on = True
        else:
            is_on = self._app_data.app_settings.get('generate_links_graph', False)

        if not is_on:
            return

        if sutta is None:
            active_sutta = self.s._get_active_tab().sutta
            if active_sutta is None:
                return
            else:
                sutta = active_sutta

        self.generate_and_show_graph(sutta, None, self.queue_id, self.graph_path, self.messages_url)

    def _update_sidebar_fulltext(self, hits: Optional[int]) -> List[SearchResult]:
        if hits is None \
           or hits == 0:
            self.rightside_tabs.setTabText(self.fulltext_results_tab_idx, "Results")
        else:
            self.rightside_tabs.setTabText(self.fulltext_results_tab_idx, f"Results ({hits})")

        results = self.render_fulltext_page()

        if hits is None:
            self.fulltext_page_input.setMinimum(1)
            self.fulltext_page_input.setMaximum(99)
            self.fulltext_first_page_btn.setEnabled(False)
            self.fulltext_last_page_btn.setEnabled(False)

        elif hits == 0:
            self.fulltext_page_input.setMinimum(0)
            self.fulltext_page_input.setMaximum(0)
            self.fulltext_first_page_btn.setEnabled(False)
            self.fulltext_last_page_btn.setEnabled(False)

        elif hits <= self.page_len:
            self.fulltext_page_input.setMinimum(1)
            self.fulltext_page_input.setMaximum(1)
            self.fulltext_first_page_btn.setEnabled(False)
            self.fulltext_last_page_btn.setEnabled(False)

        else:
            n = self.result_pages_count()
            if n is None:
                pages = 99
            else:
                pages = n

            self.fulltext_page_input.setMinimum(1)
            self.fulltext_page_input.setMaximum(pages)
            self.fulltext_first_page_btn.setEnabled(True)
            self.fulltext_last_page_btn.setEnabled(True)

        return results

    def _show_selected(self):
        self.s._show_sutta_from_message(self.selected_info)

    def _handle_result_select(self):
        logger.info("_handle_result_select()")

        if len(self.fulltext_list.selectedItems()) == 0:
            # .itemSelectionChanged was triggered by changing the page, but no
            # new item is selected.
            return

        page_num = self.fulltext_page_input.value() - 1
        results = self.results_page(page_num)

        selected_idx = self.fulltext_list.currentRow()
        logger.info(f"selected_idx: {selected_idx}")

        if selected_idx < len(results):
            sutta = self.s._sutta_from_result(results[selected_idx])
            if sutta is not None:
                self.s._add_recent(sutta)
                self.s._show_sutta(sutta)

    def _handle_recent_select(self):
        selected_idx = self.recent_list.currentRow()
        sutta: USutta = self.s._recent[selected_idx]
        self.s._show_sutta(sutta)

    def _set_recent_list(self, titles: List[str]):
        self.recent_list.clear()
        self.recent_list.insertItems(0, titles)

    def _select_prev_recent(self):
        selected_idx = self.recent_list.currentRow()
        if selected_idx == -1:
            self.recent_list.setCurrentRow(0)
        elif selected_idx == 0:
            return
        else:
            self.recent_list.setCurrentRow(selected_idx - 1)

    def _select_next_recent(self):
        selected_idx = self.recent_list.currentRow()
        # List is empty or lost focus (no selected item)
        if selected_idx == -1:
            if len(self.recent_list) == 1:
                # Only one viewed item, which is presently being shown, and no
                # next item
                return
            else:
                # The 0 index is already the presently show item
                self.recent_list.setCurrentRow(1)

        elif selected_idx + 1 < len(self.recent_list):
            self.recent_list.setCurrentRow(selected_idx + 1)

    def _select_prev_result(self):
        selected_idx = self.fulltext_list.currentRow()
        if selected_idx == -1:
            self.fulltext_list.setCurrentRow(0)
        elif selected_idx == 0:
            return
        else:
            self.fulltext_list.setCurrentRow(selected_idx - 1)

    def _select_next_result(self):
        selected_idx = self.fulltext_list.currentRow()
        if selected_idx == -1:
            self.fulltext_list.setCurrentRow(0)
        elif selected_idx + 1 < len(self.fulltext_list):
            self.fulltext_list.setCurrentRow(selected_idx + 1)

    def _focus_search_input(self):
        self.s.search_input.setFocus()

    def _increase_text_size(self):
        font_size = self._app_data.app_settings.get('sutta_font_size', 22)
        self._app_data.app_settings['sutta_font_size'] = font_size + 2
        self._app_data._save_app_settings()
        self.s._get_active_tab().render_sutta_content()

    def _decrease_text_size(self):
        font_size = self._app_data.app_settings.get('sutta_font_size', 22)
        if font_size < 5:
            return
        self._app_data.app_settings['sutta_font_size'] = font_size - 2
        self._app_data._save_app_settings()
        self.s._get_active_tab().render_sutta_content()

    def _increase_text_margins(self):
        # increase margins = smaller max with
        max_width = self._app_data.app_settings.get('sutta_max_width', 75)
        if max_width < 10:
            return
        self._app_data.app_settings['sutta_max_width'] = max_width - 2
        self._app_data._save_app_settings()
        self.s._get_active_tab().render_sutta_content()

    def _decrease_text_margins(self):
        # decrease margins = greater max with
        max_width = self._app_data.app_settings.get('sutta_max_width', 75)
        self._app_data.app_settings['sutta_max_width'] = max_width + 2
        self._app_data._save_app_settings()
        self.s._get_active_tab().render_sutta_content()

    def _show_search_result_sizes_dialog(self):
        from simsapa.layouts.search_result_sizes_dialog import SearchResultSizesDialog
        d = SearchResultSizesDialog(self._app_data, self)
        if d.exec() and self.s.enable_sidebar:
            self._update_sidebar_fulltext(self.query_hits())

    # def _show_import_suttas_dialog(self):
    #     from simsapa.layouts.import_suttas_dialog import ImportSuttasWithSpreadsheetDialog
    #     d = ImportSuttasWithSpreadsheetDialog(self._app_data, self)
    #     if d.exec():
    #         logger.info("Finished importing suttas")
    #         self._show_quit_and_restart()

    # def _remove_imported_suttas(self):
    #     msg = """
    #     <p>Remove all imported suttas from the user database?</p>
    #     <p>(Suttas stored in the default application database will remain.)</p>
    #     """

    #     reply = QMessageBox.question(self,
    #                                  "Remove Imported Suttas",
    #                                  msg,
    #                                  QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
    #                                  QMessageBox.StandardButton.No)

    #     if reply == QMessageBox.StandardButton.Yes:
    #         suttas = self._app_data.db_session.query(Um.Sutta).all()

    #         uids = list(map(lambda x: x.uid, suttas))

    #         for db_item in suttas:
    #             self._app_data.db_session.delete(db_item)

    #         self._app_data.db_session.commit()

    #         w = self._queries.search_indexes.suttas_index.writer()
    #         for x in uids:
    #             w.delete_by_term('uid', x)

    #         w.commit()

    #         self._show_quit_and_restart()

    def _show_quit_and_restart(self):
        msg = """
        <p>Restart Simsapa for the changes to take effect.</p>
        <p>Quit now?</p>
        """

        reply = QMessageBox.question(self,
                                     "Restart Simsapa",
                                     msg,
                                     QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
                                     QMessageBox.StandardButton.No)

        if reply == QMessageBox.StandardButton.Yes:
            self.action_Quit.activate(QAction.ActionEvent.Trigger)

    def _toggle_sidebar(self):
        is_on = self.action_Show_Sidebar.isChecked()
        if is_on:
            self.splitter.setSizes([2000, 2000])
        else:
            self.splitter.setSizes([2000, 0])

    def _show_sutta_languages(self):
        from simsapa.layouts.sutta_languages import SuttaLanguagesWindow
        w = SuttaLanguagesWindow(self._app_data, parent = self)
        w.show()

    def _show_send_to_kindle(self):
        tab = self.s._get_active_tab()

        from simsapa.layouts.send_to_kindle import SendToKindleWindow

        def _open_send(html: str):
            w = SendToKindleWindow(self._app_data,
                                   tab_sutta = tab.sutta,
                                   tab_html = html,
                                   parent = self)
            w.show()

        tab.qwe.page().toHtml(_open_send)

    def _show_send_to_remarkable(self):
        tab = self.s._get_active_tab()

        from simsapa.layouts.send_to_remarkable import SendToRemarkableWindow

        def _open_send(html: str):
            w = SendToRemarkableWindow(self._app_data,
                                       tab_sutta = tab.sutta,
                                       tab_html = html,
                                       parent = self)
            w.show()

        tab.qwe.page().toHtml(_open_send)

    def _show_export_as(self):
        from simsapa.layouts.sutta_export_dialog import SuttaExportDialog

        tab = self.s._get_active_tab()
        if tab.sutta is None:
            return

        d = SuttaExportDialog(self._app_data, tab.sutta)
        d.exec()

    def connect_preview_window_signals(self, preview_window: PreviewWindow):
        self.graph_link_mouseover.connect(partial(preview_window.graph_link_mouseover))
        self.s.connect_preview_window_signals(preview_window)

    def _handle_close(self):
        self.close()

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self._handle_close))

        self.action_Copy \
            .triggered.connect(partial(self.s._handle_copy))

        self.action_Paste \
            .triggered.connect(partial(self.s._handle_paste))

        self.action_Search_Query_Terms \
            .triggered.connect(partial(show_search_info))

        self.action_Show_Related_Suttas \
            .triggered.connect(partial(self.s._handle_show_related_suttas))

        self.action_Lookup_Selection_in_Dictionary \
            .triggered.connect(partial(self.s._lookup_selection_in_dictionary))

        self.action_Lookup_Clipboard_in_Suttas \
            .triggered.connect(partial(self._lookup_clipboard_in_suttas))

        self.action_Lookup_Clipboard_in_Dictionary \
            .triggered.connect(partial(self._lookup_clipboard_in_dictionary))

        self.action_Previous_Result \
            .triggered.connect(partial(self._select_prev_result))

        self.action_Next_Result \
            .triggered.connect(partial(self._select_next_result))

        self.action_Previous_Tab \
            .triggered.connect(partial(self.s._select_prev_tab))

        self.action_Next_Tab \
            .triggered.connect(partial(self.s._select_next_tab))

        self.recent_list.itemSelectionChanged.connect(partial(self._handle_recent_select))

        self.add_memo_button \
            .clicked.connect(partial(self.add_memo_for_sutta))

        self.action_Reload_Page \
            .triggered.connect(partial(self.s.reload_page))

        self.action_Increase_Text_Size \
            .triggered.connect(partial(self._increase_text_size))

        self.action_Decrease_Text_Size \
            .triggered.connect(partial(self._decrease_text_size))

        self.action_Increase_Text_Margins \
            .triggered.connect(partial(self._increase_text_margins))

        self.action_Decrease_Text_Margins \
            .triggered.connect(partial(self._decrease_text_margins))

        # self.action_Import_Suttas_with_Spreadsheet \
        #     .triggered.connect(partial(self._show_import_suttas_dialog))

        # self.action_Remove_Imported_Suttas \
        #     .triggered.connect(partial(self._remove_imported_suttas))

        self.action_Search_Result_Sizes \
            .triggered.connect(partial(self._show_search_result_sizes_dialog))

        self.action_Show_Sidebar \
            .triggered.connect(partial(self._toggle_sidebar))

        self.action_Sutta_Languages \
            .triggered.connect(partial(self._show_sutta_languages))

        self.action_Send_to_Kindle \
            .triggered.connect(partial(self._show_send_to_kindle))

        self.action_Send_to_reMarkable \
            .triggered.connect(partial(self._show_send_to_remarkable))

        self.action_Export_As \
            .triggered.connect(partial(self._show_export_as))

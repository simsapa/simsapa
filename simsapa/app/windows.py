import os
from functools import partial
from typing import List

from PyQt5.QtWidgets import (QApplication, QMainWindow, QFileDialog)

from .types import AppData

from ..layouts.sutta_search import SuttaSearchWindow
from ..layouts.dictionary_search import DictionarySearchWindow
# from ..layouts.dictionaries_manager import DictionariesManagerWindow
from ..layouts.document_reader import DocumentReaderWindow
from ..layouts.library_browser import LibraryBrowserWindow
from ..layouts.memos_browser import MemosBrowserWindow
from ..layouts.links_browser import LinksBrowserWindow


class AppWindows:
    def __init__(self, app: QApplication, app_data: AppData):
        self._app = app
        self._app_data = app_data
        self._windows: List[QMainWindow] = []

    def _new_sutta_search_window(self):
        view = SuttaSearchWindow(self._app_data)
        self._connect_signals(view)
        view.show()
        if self._app_data.sutta_to_open:
            view._show_sutta(self._app_data.sutta_to_open)
            self._app_data.sutta_to_open = None
        self._windows.append(view)

    def _new_dictionary_search_window(self):
        view = DictionarySearchWindow(self._app_data)
        self._connect_signals(view)
        view.show()
        if self._app_data.dict_word_to_open:
            view._show_word(self._app_data.dict_word_to_open)
            self._app_data.dict_word_to_open = None
        self._windows.append(view)

#    def _new_dictionaries_manager_window(self):
#        view = DictionariesManagerWindow(self._app_data)
#        self._connect_signals(view)
#        view.show()
#        self._windows.append(view)

    def _new_library_browser_window(self):
        view = LibraryBrowserWindow(self._app_data)
        self._connect_signals(view)
        view.show()
        self._windows.append(view)

    def _new_memos_browser_window(self):
        view = MemosBrowserWindow(self._app_data)
        self._connect_signals(view)
        view.show()
        self._windows.append(view)

    def _new_links_browser_window(self):
        view = LinksBrowserWindow(self._app_data)
        self._connect_signals(view)
        view.show()
        self._windows.append(view)

    def _new_document_reader_window(self, file_path=None):
        view = DocumentReaderWindow(self._app_data)
        self._connect_signals(view)
        view.show()

        if file_path is not None and file_path is not False and len(file_path) > 0:
            view.open_doc(file_path)

        self._windows.append(view)

    def _open_selected_document(self, view: QMainWindow):
        doc = view.get_selected_document()
        if doc:
            self._new_document_reader_window(doc.filepath)

    def _open_file_dialog(self, view: QMainWindow):
        try:
            view.open_file_dialog()
        except AttributeError:
            file_path, _ = QFileDialog.getOpenFileName(
                None,
                "Open File...",
                "",
                "PDF or Epub Files (*.pdf *.epub)")

            if len(file_path) != 0:
                self._new_document_reader_window(file_path)

    def _close_all_windows(self):
        for w in self._windows:
            w.close()

    def _quit_app(self):
        self._close_all_windows()
        self._app.quit()

    def _connect_signals(self, view: QMainWindow):
        view.action_Open \
            .triggered.connect(partial(self._open_file_dialog, view))

        try:
            view.action_Open_Selected \
                .triggered.connect(partial(self._open_selected_document, view))
        except Exception:
            # FIXME silent exception
            pass

        view.action_Quit \
            .triggered.connect(partial(self._quit_app))
        view.action_Sutta_Search \
            .triggered.connect(partial(self._new_sutta_search_window))
        view.action_Dictionary_Search \
            .triggered.connect(partial(self._new_dictionary_search_window))
        view.action_Memos \
            .triggered.connect(partial(self._new_memos_browser_window))
        view.action_Links \
            .triggered.connect(partial(self._new_links_browser_window))

        s = os.getenv('ENABLE_WIP_FEATURES')
        if s is not None and s.lower() == 'true':
            # view.action_Dictionaries_Manager \
            #     .triggered.connect(partial(self._new_dictionaries_manager_window))
            view.action_Document_Reader \
                .triggered.connect(partial(self._new_document_reader_window))
            view.action_Library \
                .triggered.connect(partial(self._new_library_browser_window))
        else:
            if hasattr(view,'toolBar'):
                view.toolBar.setVisible(False)

            view.action_Dictionaries_Manager.setVisible(False)
            view.action_Document_Reader.setVisible(False)
            view.action_Library.setVisible(False)

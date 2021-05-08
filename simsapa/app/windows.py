from functools import partial
from typing import List

from PyQt5.QtWidgets import QMainWindow  # type: ignore

from .types import AppData  # type: ignore

from ..layouts.sutta_search import SuttaSearchWindow, SuttaSearchCtrl  # type: ignore
from ..layouts.dictionary_search import DictionarySearchWindow, DictionarySearchCtrl  # type: ignore


class AppWindows:
    def __init__(self, app_data: AppData):
        self._app_data = app_data
        self._windows: List[QMainWindow] = []

    def _new_sutta_search_window(self):
        view = SuttaSearchWindow(self._app_data)
        self._connect_signals(view)
        view.show()
        SuttaSearchCtrl(view)
        self._windows.append(view)

    def _new_dictionary_search_window(self):
        view = DictionarySearchWindow(self._app_data)
        self._connect_signals(view)
        view.show()
        DictionarySearchCtrl(view)
        self._windows.append(view)

    def _close_all_windows(self):
        for w in self._windows:
            w.close()

    def _connect_signals(self, view: QMainWindow):
        view.action_Quit \
            .triggered.connect(partial(self._close_all_windows))
        view.action_Sutta_Search \
            .triggered.connect(partial(self._new_sutta_search_window))
        view.action_Dictionary_Search \
            .triggered.connect(partial(self._new_dictionary_search_window))



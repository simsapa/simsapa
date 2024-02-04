from typing import Optional

from PyQt6.QtGui import QHideEvent, QShowEvent
from simsapa.app.app_data import AppData
from simsapa.layouts.gui_helpers import is_dictionary_search_window, is_ebook_reader_window, is_sutta_search_window, is_sutta_study_window

from simsapa.layouts.gui_types import AppWindowInterface, WindowPosSize


class HasRestoreSizePos(AppWindowInterface):
    _app_data: AppData
    _window_size_pos: Optional[WindowPosSize] = None

    def save_size_pos(self):
        qr = self.frameGeometry()
        self._window_size_pos = WindowPosSize(
            x = qr.x(),
            y = qr.y(),
            width = qr.width(),
            height = qr.height(),
        )

        p = self._window_size_pos

        if is_sutta_search_window(self):
            self._app_data.app_settings['sutta_search_pos'] = p

        elif is_dictionary_search_window(self):
            self._app_data.app_settings['dictionary_search_pos'] = p

        elif is_sutta_study_window(self):
            self._app_data.app_settings['sutta_study_pos'] = p

        elif is_ebook_reader_window(self):
            self._app_data.app_settings['ebook_reader_pos'] = p

        self._app_data._save_app_settings()

    def restore_size_pos(self):
        p = self._window_size_pos
        if p is None:
            return

        self.resize(p['width'], p['height'])
        self.move(p['x'], p['y'])

    def showEvent(self, event: QShowEvent) -> None:
        super().showEvent(event)
        self.restore_size_pos()

    def hideEvent(self, event: QHideEvent) -> None:
        self.save_size_pos()
        return super().hideEvent(event)

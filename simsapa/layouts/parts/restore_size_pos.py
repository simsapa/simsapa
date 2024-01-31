from typing import Optional

from PyQt6.QtGui import QHideEvent, QShowEvent
from PyQt6.QtWidgets import QMainWindow

from simsapa.layouts.gui_types import WindowPosSize


class HasRestoreSizePos(QMainWindow):

    _window_size_pos: Optional[WindowPosSize] = None

    def save_size_pos(self):
        qr = self.frameGeometry()
        self._window_size_pos = WindowPosSize(
            x = qr.x(),
            y = qr.y(),
            width = qr.width(),
            height = qr.height(),
        )

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

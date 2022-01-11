import logging as _logging
from typing import Callable, Optional

from PyQt5.QtCore import QAbstractNativeEventFilter, QAbstractEventDispatcher
from PyQt5.QtCore import QObject, pyqtSignal
from PyQt5.QtWidgets import QMainWindow

from pyqtkeybind.x11 import X11KeyBinder
from pyqtkeybind import keybinder

from simsapa.app.hotkeys_manager_interface import HotkeysManagerInterface

logger = _logging.getLogger(__name__)

class WinEventFilter(QAbstractNativeEventFilter):
    def __init__(self, keybinder):
        self.keybinder = keybinder
        super().__init__()

    def nativeEventFilter(self, eventType, message):
        ret = self.keybinder.handler(eventType, message)
        return ret, 0

class HotkeysManagerLinux(QObject, HotkeysManagerInterface):
    lookup_clipboard_in_suttas_signal = pyqtSignal()
    lookup_clipboard_in_dictionary_signal = pyqtSignal()

    def __init__(self):
        super().__init__()

        self.keybinder: X11KeyBinder = keybinder
        keybinder.init()

        self.win_ids = []

        self.win_event_filter = WinEventFilter(self.keybinder)
        self.event_dispatcher = QAbstractEventDispatcher.instance()
        self.event_dispatcher.installNativeEventFilter(self.win_event_filter)

    def setup_window(self, window: QMainWindow,
                     sutta_lookup_fn: Optional[Callable] = None,
                     dict_lookup_fn: Optional[Callable] = None):

        if sutta_lookup_fn is not None:
            self.lookup_clipboard_in_suttas_signal.connect(sutta_lookup_fn)
        if dict_lookup_fn is not None:
            self.lookup_clipboard_in_dictionary_signal.connect(dict_lookup_fn)

        win_id = window.winId()
        self.win_ids.append(win_id)

        self.keybinder.register_hotkey(win_id, "ctrl+shift+s", self.lookup_clipboard_in_suttas_signal.emit)
        self.keybinder.register_hotkey(win_id, "ctrl+shift+d", self.lookup_clipboard_in_dictionary_signal.emit)

    def unregister_all_hotkeys(self):
        for i in self.win_ids:
            self.keybinder.unregister_hotkey(i,  "ctrl+shift+s")
            self.keybinder.unregister_hotkey(i,  "ctrl+shift+d")

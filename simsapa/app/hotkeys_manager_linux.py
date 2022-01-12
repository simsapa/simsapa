import logging as _logging

from PyQt5.QtCore import QAbstractNativeEventFilter, QAbstractEventDispatcher
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

class HotkeysManagerLinux(HotkeysManagerInterface):
    def __init__(self, api_port: int):
        super().__init__(api_port)

        self.keybinder: X11KeyBinder = keybinder
        keybinder.init()

        self.win_ids = []

        self.win_event_filter = WinEventFilter(self.keybinder)
        self.event_dispatcher = QAbstractEventDispatcher.instance()
        self.event_dispatcher.installNativeEventFilter(self.win_event_filter)

    def setup_window(self, window: QMainWindow):
        win_id = window.winId()
        self.win_ids.append(win_id)

        self.keybinder.register_hotkey(win_id, "ctrl+shift+s", self.lookup_clipboard_in_suttas)
        self.keybinder.register_hotkey(win_id, "ctrl+shift+d", self.lookup_clipboard_in_dictionary)

    def unregister_all_hotkeys(self):
        for i in self.win_ids:
            self.keybinder.unregister_hotkey(i, "ctrl+shift+s")
            self.keybinder.unregister_hotkey(i, "ctrl+shift+d")


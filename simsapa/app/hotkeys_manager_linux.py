import logging as _logging

from PyQt5.QtCore import QAbstractNativeEventFilter, QAbstractEventDispatcher
from PyQt5.QtWidgets import QMainWindow

from pyqtkeybind.x11 import X11KeyBinder
from pyqtkeybind import keybinder

from simsapa.app.helpers import write_log
from simsapa.app.hotkeys_manager_interface import HotkeysManagerInterface
from simsapa.app.actions_manager import ActionsManager

logger = _logging.getLogger(__name__)

class WinEventFilter(QAbstractNativeEventFilter):
    def __init__(self, keybinder):
        self.keybinder = keybinder
        super().__init__()

    def nativeEventFilter(self, eventType, message):
        ret = self.keybinder.handler(eventType, message)
        return ret, 0

class HotkeysManagerLinux(HotkeysManagerInterface):
    def __init__(self, actions_manager: ActionsManager):
        super().__init__(actions_manager)

        self.keybinder: X11KeyBinder = keybinder
        keybinder.init()

        self.win_ids = []

        self.win_event_filter = WinEventFilter(self.keybinder)
        self.event_dispatcher = QAbstractEventDispatcher.instance()
        self.event_dispatcher.installNativeEventFilter(self.win_event_filter)

    def setup_window(self, window: QMainWindow):
        write_log("setup_window()")
        win_id = window.winId()
        self.win_ids.append(win_id)

        try:
            self.keybinder.register_hotkey(win_id, "ctrl+shift+s", self.lookup_clipboard_in_suttas)
            self.keybinder.register_hotkey(win_id, "ctrl+shift+g", self.lookup_clipboard_in_dictionary)
        except Exception as e:
            write_log(f"ERROR: {e}")

    def unregister_all_hotkeys(self):
        write_log("unregister_all_hotkeys()")
        try:
            for i in self.win_ids:
                self.keybinder.unregister_hotkey(i, "ctrl+shift+s")
                self.keybinder.unregister_hotkey(i, "ctrl+shift+g")
        except Exception as e:
            write_log(f"ERROR: {e}")

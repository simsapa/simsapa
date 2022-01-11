import logging as _logging
from typing import Callable, Optional

from PyQt5.QtCore import QObject, pyqtSignal
from PyQt5.QtWidgets import QMainWindow

import keyboard

from simsapa.app.hotkeys_manager_interface import HotkeysManagerInterface

logger = _logging.getLogger(__name__)

class HotkeysManagerWindowsMac(QObject, HotkeysManagerInterface):
    lookup_clipboard_in_suttas_signal = pyqtSignal()
    lookup_clipboard_in_dictionary_signal = pyqtSignal()

    def __init__(self):
        super().__init__()

    def setup_window(self, window: QMainWindow,
                     sutta_lookup_fn: Optional[Callable] = None,
                     dict_lookup_fn: Optional[Callable] = None):

        if sutta_lookup_fn is not None:
            self.lookup_clipboard_in_suttas_signal.connect(sutta_lookup_fn)
        if dict_lookup_fn is not None:
            self.lookup_clipboard_in_dictionary_signal.connect(dict_lookup_fn)

        try:
            keyboard.add_hotkey("ctrl+shift+s", self.lookup_clipboard_in_suttas_signal.emit, suppress=True)
            keyboard.add_hotkey("ctrl+shift+d", self.lookup_clipboard_in_dictionary_signal.emit, suppress=True)
        except Exception as e:
            logger.error("Can't init hotkeys.")
            print(e)

    def unregister_all_hotkeys(self):
        keyboard.unhook_all_hotkeys()

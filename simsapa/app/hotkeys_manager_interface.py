from typing import Callable, Optional
from PyQt5.QtCore import pyqtSignal
from PyQt5.QtWidgets import QMainWindow

class HotkeysManagerInterface:
    lookup_clipboard_in_suttas_signal: pyqtSignal
    lookup_clipboard_in_dictionary_signal: pyqtSignal

    def setup_window(self,
                     window: QMainWindow,
                     sutta_lookup_fn: Optional[Callable] = None,
                     dict_lookup_fn: Optional[Callable] = None):
        pass

    def unregister_all_hotkeys(self):
        pass

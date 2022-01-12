import logging as _logging

from PyQt5.QtWidgets import QMainWindow

import keyboard

from simsapa.app.hotkeys_manager_interface import HotkeysManagerInterface

logger = _logging.getLogger(__name__)

class HotkeysManagerWindowsMac(HotkeysManagerInterface):
    def __init__(self, api_port: int):
        super().__init__(api_port)

        try:
            keyboard.add_hotkey("ctrl+shift+s", self.lookup_clipboard_in_suttas, suppress=True)
            keyboard.add_hotkey("ctrl+shift+d", self.lookup_clipboard_in_dictionary, suppress=True)
        except Exception as e:
            logger.error("Can't init hotkeys.")
            print(e)

    def setup_window(self, window: QMainWindow):
        pass

    def unregister_all_hotkeys(self):
        keyboard.unhook_all_hotkeys()

import logging as _logging

from PyQt5.QtWidgets import QMainWindow

from simsapa.keyboard import keyboard

from simsapa.app.helpers import write_log
from simsapa.app.actions_manager import ActionsManager
from simsapa.app.hotkeys_manager_interface import HotkeysManagerInterface

logger = _logging.getLogger(__name__)

class HotkeysManagerWindowsMac(HotkeysManagerInterface):
    def __init__(self, actions_manager: ActionsManager):
        super().__init__(actions_manager)

        try:
            keyboard.add_hotkey("ctrl+shift+s", self.lookup_clipboard_in_suttas, suppress=True)
            keyboard.add_hotkey("ctrl+shift+g", self.lookup_clipboard_in_dictionary, suppress=True)
        except Exception as e:
            logger.error("Can't init hotkeys.")
            print(e)

    def setup_window(self, window: QMainWindow):
        pass

    def unregister_all_hotkeys(self):
        write_log("unregister_all_hotkeys()")
        try:
            keyboard.unhook_all_hotkeys()
        except Exception as e:
            write_log(f"ERROR: {e}")

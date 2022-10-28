from PyQt6.QtWidgets import QMainWindow

from simsapa.keyboard import keyboard

from simsapa import logger
from simsapa.app.actions_manager import ActionsManager
from simsapa.app.hotkeys_manager_interface import HotkeysManagerInterface

class HotkeysManagerWindowsMac(HotkeysManagerInterface):
    def __init__(self, actions_manager: ActionsManager):
        super().__init__(actions_manager)

        try:
            keyboard.add_hotkey("ctrl+shift+f6", self.show_word_scan_popup, suppress=True)
            keyboard.add_hotkey("ctrl+shift+s", self.lookup_clipboard_in_suttas, suppress=True)
            keyboard.add_hotkey("ctrl+shift+g", self.lookup_clipboard_in_dictionary, suppress=True)
        except Exception as e:
            logger.error(f"Can't init hotkeys: {e}")

    def setup_window(self, window: QMainWindow):
        pass

    def unregister_all_hotkeys(self):
        logger.info("unregister_all_hotkeys()")
        try:
            keyboard.unhook_all_hotkeys()
        except Exception as e:
            logger.error(e)

from PyQt5.QtWidgets import QMainWindow

from simsapa.app.actions_manager import ActionsManager

class HotkeysManagerInterface:
    api_url: str

    def __init__(self, actions_manager: ActionsManager):
        self.actions_manager = actions_manager

    def setup_window(self, window: QMainWindow):
        raise NotImplementedError

    def unregister_all_hotkeys(self):
        raise NotImplementedError

    def show_word_scan_popup(self):
        self.actions_manager.show_word_scan_popup()

    def lookup_clipboard_in_suttas(self):
        self.actions_manager.lookup_clipboard_in_suttas()

    def lookup_clipboard_in_dictionary(self):
        self.actions_manager.lookup_clipboard_in_dictionary()

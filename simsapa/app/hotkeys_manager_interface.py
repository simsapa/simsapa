from PyQt5.QtWidgets import QMainWindow
import requests

class HotkeysManagerInterface:
    api_url: str

    def __init__(self, api_port: int):
        self.api_url = f'http://localhost:{api_port}'

    def setup_window(self, window: QMainWindow):
        raise NotImplementedError

    def unregister_all_hotkeys(self):
        raise NotImplementedError

    def lookup_clipboard_in_suttas(self):
        data = {'action': 'lookup_clipboard_in_suttas'}
        self._send_to_all(data)

    def lookup_clipboard_in_dictionary(self):
        data = {'action': 'lookup_clipboard_in_dictionary'}
        self._send_to_all(data)

    def _send_to_all(self, data):
        url = f"{self.api_url}/queues/all"
        r = requests.post(url=url, json=data)
        if r.status_code != 200:
            print(f"ERROR: {r}")

import json
from typing import List
import requests

from simsapa import logger
from simsapa import ApiAction, ApiMessage

class ActionsManager:
    api_url: str

    def __init__(self, api_port: int):
        self.api_url = f'http://localhost:{api_port}'

    def show_word_scan_popup(self):
        msg = ApiMessage(action = ApiAction.show_word_scan_popup, data = '')
        self._send_to_all(msg)

    def lookup_clipboard_in_suttas(self):
        msg = ApiMessage(action = ApiAction.lookup_clipboard_in_suttas, data = '')
        self._send_to_all(msg)

    def lookup_clipboard_in_dictionary(self):
        msg = ApiMessage(action = ApiAction.lookup_clipboard_in_dictionary, data = '')
        self._send_to_all(msg)

    def lookup_in_suttas(self, query: str):
        msg = ApiMessage(action = ApiAction.lookup_in_suttas,
                         data = query)
        self._send_to_all(msg)

    def lookup_in_dictionary(self, query: str):
        msg = ApiMessage(action = ApiAction.lookup_in_dictionary,
                         data = query)
        self._send_to_all(msg)

    def open_sutta_new(self, uid: str):
        msg = ApiMessage(action = ApiAction.open_sutta_new,
                         data = uid)
        self._send_to_all(msg)

    def open_words_new(self, schemas_ids: List[tuple[str, int]]):
        msg = ApiMessage(action = ApiAction.open_words_new,
                         data = json.dumps(schemas_ids))
        self._send_to_all(msg)

    def _send_to_all(self, msg: ApiMessage):
        url = f"{self.api_url}/queues/all"
        logger.info(f"_send_to_all(): {url}, {msg}")
        try:
            r = requests.post(url=url, json=msg)
            if r.status_code != 200:
                logger.error(f"{r}")
        except Exception as e:
            logger.error(e)

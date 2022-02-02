import requests

from simsapa import logger

class ActionsManager:
    api_url: str

    def __init__(self, api_port: int):
        self.api_url = f'http://localhost:{api_port}'

    def lookup_clipboard_in_suttas(self):
        data = {'action': 'lookup_clipboard_in_suttas'}
        self._send_to_all(data)

    def lookup_clipboard_in_dictionary(self):
        data = {'action': 'lookup_clipboard_in_dictionary'}
        self._send_to_all(data)

    def lookup_in_suttas(self, query: str):
        data = {
            'action': 'lookup_in_suttas',
            'query': query,
        }
        self._send_to_all(data)

    def lookup_in_dictionary(self, query: str):
        data = {
            'action': 'lookup_in_dictionary',
            'query': query,
        }
        self._send_to_all(data)

    def _send_to_all(self, data):
        url = f"{self.api_url}/queues/all"
        logger.info(f"_send_to_all(): {url}, {data}")
        try:
            r = requests.post(url=url, json=data)
            if r.status_code != 200:
                logger.error(f"{r}")
        except Exception as e:
            logger.error(e)

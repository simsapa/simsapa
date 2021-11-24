import logging as _logging
import re
import cgi
import socket
from http.server import HTTPServer, BaseHTTPRequestHandler

from simsapa import APP_QUEUES

logger = _logging.getLogger(__name__)


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(404, 'Not Found')
        self._send_cors_headers()
        self.end_headers()

        # self.send_response(200)
        # self._send_cors_headers()
        # self.send_header('Content-type', 'text/html')
        # self.end_headers()
        # res = "<h1>It's over 9000!</h1>"
        # self.wfile.write(bytes(res, 'utf-8'))

    def do_OPTIONS(self):
        self.send_response(200)
        self._send_cors_headers()
        self.end_headers()

    def do_POST(self):
        path = self.path.rstrip('/')

        if re.search('/queues/*', path):

            ctype, pdict = cgi.parse_header(self.headers.get('content-type'))
            if ctype == 'application/json':
                length = int(self.headers.get('content-length'))
                data: str = self.rfile.read(length).decode('utf8')

            else:
                self.send_response(400, "Bad Request")
                self._send_cors_headers()
                self.end_headers()
                return

            queue_id = path.split('/')[-1]

            if queue_id not in APP_QUEUES.keys():
                self.send_response(403, 'Forbidden')
                self._send_cors_headers()
                self.end_headers()
                return

            APP_QUEUES[queue_id].put_nowait(data)

            self.send_response(200)
            self._send_cors_headers()
            self.end_headers()

        else:
            self.send_response(404, 'Not Found')
            self._send_cors_headers()
            self.end_headers()

    def _send_cors_headers(self):
        """ Sets headers required for CORS """
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET,POST,OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "x-api-key,Content-Type")


def start_server(port=8000):
    logger.info(f'Starting server on port {port}')
    httpd = HTTPServer(('127.0.0.1', port), Handler)
    httpd.serve_forever()


def find_available_port() -> int:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(('', 0))
    _, port = sock.getsockname()
    return port

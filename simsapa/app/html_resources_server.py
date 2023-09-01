from pathlib import Path
import socket, os, sys, threading
from flask import Flask
from flask_cors import CORS
import logging

from simsapa import logger

class HtmlResourcesServer:
    def __init__(self, resources_base_dir: Path):
        if not resources_base_dir.exists():
            logger.error(f"(HtmlResourcesServer) Path doesn't exist: {resources_base_dir}")

        self.resources_base_dir = resources_base_dir

        self.port = find_available_port_html_resources()

        self.app = Flask(__name__, static_url_path='', static_folder=resources_base_dir)

        self.app.config['ENV'] = 'development'
        CORS(self.app)
        logging.getLogger("werkzeug").disabled = True

        self.app.register_error_handler(400, self.resp_bad_request)
        self.app.register_error_handler(403, self.resp_forbidden)
        self.app.register_error_handler(404, self.resp_not_found)

    def resp_bad_request(self, e):
        msg = f"(HtmlResourcesServer) Bad Request: {e}"
        logger.error(msg)
        return msg, 400

    def resp_not_found(self, e):
        msg = f"(HtmlResourcesServer) Not Found: {e}"
        logger.error(msg)
        return msg, 404

    def resp_forbidden(self, e):
        msg = f"Forbidden: {e}"
        logger.error(msg)
        return msg, 403

    def start_server(self):
        self.server_daemon = threading.Thread(name='daemon_server_sutta_index',
                                              target=self._start_server_html_resources)
        self.server_daemon.setDaemon(True)
        self.server_daemon.start()

    def _start_server_html_resources(self):
        logger.info(f'(HtmlResourcesServer) Starting server on port {self.port}')
        os.environ["FLASK_ENV"] = "development"

        # Error in click.utils.echo() when console is unavailable
        # https://github.com/pallets/click/issues/2415
        if getattr(sys, 'frozen', False):
            f = open(os.devnull, 'w')
            sys.stdin = f
            sys.stdout = f

        self.app.run(host='127.0.0.1', port=self.port, debug=False, load_dotenv=False)

def find_available_port_html_resources() -> int:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(('', 0))
    _, port = sock.getsockname()
    return port

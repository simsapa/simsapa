import json
import logging as _logging
import re
import socket
from typing import Optional
import falcon
from wsgiref.simple_server import make_server

from simsapa import APP_QUEUES
from simsapa.app.helpers import write_log

logger = _logging.getLogger(__name__)

class QueueResource:
    def on_get(self, req: falcon.Request, resp: falcon.Response):
        write_log("Resp: 404 Not Found")
        resp.status = falcon.HTTP_404

    def on_post(self, req: falcon.Request, resp: falcon.Response, queue_id: Optional[str]):
        if req.content_type == 'application/json':
            m = req.get_media(default_when_empty="{}")
            data = json.dumps(m)
            write_log(data)

        else:
            write_log("Resp: 400 Bad Request")
            resp.status = falcon.HTTP_400
            return

        if queue_id is None:
            resp.status = falcon.HTTP_404
            return

        if queue_id != 'all' and queue_id not in APP_QUEUES.keys():
            write_log("Resp: 403 Forbidden")
            resp.status = falcon.HTTP_403
            return

        if queue_id == 'all':
            for i in APP_QUEUES.keys():
                APP_QUEUES[i].put_nowait(data)
        else:
            APP_QUEUES[queue_id].put_nowait(data)

        write_log("Resp: 200 OK")
        resp.status = falcon.HTTP_200

def start_server(port=8000):
    write_log("start_server()")

    app = falcon.App(cors_enable=True)
    queues = QueueResource()
    app.add_route('/queues/{queue_id}', queues)

    with make_server('127.0.0.1', port, app) as httpd:
        write_log(f'Starting server on port {port}')
        httpd.serve_forever()

def find_available_port() -> int:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(('', 0))
    _, port = sock.getsockname()
    return port

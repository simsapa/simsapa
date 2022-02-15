import json
import socket
from typing import Optional
import falcon
from wsgiref.simple_server import make_server

from simsapa import APP_QUEUES, PACKAGE_ASSETS_DIR
from simsapa import logger

class QueueResource:
    # def on_get(self, req: falcon.Request, resp: falcon.Response):
    #     logger.error("Resp: 404 Not Found")
    #     resp.status = falcon.HTTP_404

    def on_post(self, req: falcon.Request, resp: falcon.Response, queue_id: Optional[str]):
        if req.content_type == 'application/json':
            m = req.get_media(default_when_empty="{}")
            data = json.dumps(m)
            logger.info("QueueResource.on_post() data: %s" % data)

        else:
            logger.error("Resp: 400 Bad Request")
            resp.status = falcon.HTTP_400
            return

        if queue_id is None:
            logger.error("Resp: 404 Not Found")
            resp.status = falcon.HTTP_404
            return

        if queue_id != 'all' and queue_id not in APP_QUEUES.keys():
            logger.error("Resp: 403 Forbidden")
            resp.status = falcon.HTTP_403
            return

        if queue_id == 'all':
            for i in APP_QUEUES.keys():
                APP_QUEUES[i].put_nowait(data)
        else:
            APP_QUEUES[queue_id].put_nowait(data)

        logger.info("Resp: 200 OK")
        resp.status = falcon.HTTP_200

def start_server(port=8000):
    logger.info("start_server()")

    app = falcon.App(cors_enable=True)
    queues = QueueResource()
    app.add_route('/queues/{queue_id}', queues)

    app.add_static_route('/assets', PACKAGE_ASSETS_DIR)

    with make_server('127.0.0.1', port, app) as httpd:
        logger.info(f'Starting server on port {port}')
        httpd.serve_forever()

def find_available_port() -> int:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(('', 0))
    _, port = sock.getsockname()
    return port

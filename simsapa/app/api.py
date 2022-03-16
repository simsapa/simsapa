import socket
import os
from flask import Flask, send_from_directory, abort, request
from flask.wrappers import Response
from flask_cors import CORS

from simsapa import APP_QUEUES, PACKAGE_ASSETS_DIR
from simsapa import logger

app = Flask(__name__)
app.config['ENV'] = 'development'
cors = CORS(app)

@app.route('/queues/<string:queue_id>', methods=['POST'])
def queues(queue_id):
    if request.content_type == 'application/json':
        try:
            # NOTE get the byte string, we want a string as the queue message,
            # and we'll deserialize somewhere else, instead of doing so with
            # .get_json() here.
            data = request.get_data(as_text=True, cache=False)
        except Exception as e:
            abort(Response(f"{e}", 403))

        logger.info(f"QueueResource.on_post() data: {data}")

    else:
        abort(400)

    if queue_id is None:
        abort(404)

    if queue_id != 'all' and queue_id not in APP_QUEUES.keys():
        abort(403)

    if queue_id == 'all':
        for i in APP_QUEUES.keys():
            APP_QUEUES[i].put_nowait(data)
    else:
        APP_QUEUES[queue_id].put_nowait(data)

    return 'OK', 200

@app.route('/assets/<path:path>', methods=['GET'])
def assets(path):
    if not os.path.isfile(os.path.join(PACKAGE_ASSETS_DIR, path)):
        abort(404)

    return send_from_directory(PACKAGE_ASSETS_DIR, path)

@app.errorhandler(400)
def resp_bad_request(e):
    msg = f"Bad Request: {e}"
    logger.error(msg)
    return msg, 400

@app.errorhandler(404)
def reps_not_found(e):
    msg = f"Not Found: {e}"
    logger.error(msg)
    return msg, 404

@app.errorhandler(403)
def resp_forbidden(e):
    msg = f"Forbidden: {e}"
    logger.error(msg)
    return msg, 403

def start_server(port=8000):
    logger.info(f'Starting server on port {port}')
    os.environ["FLASK_ENV"] = "development"
    app.run(host='127.0.0.1', port=port, debug=False, load_dotenv=False)

def find_available_port() -> int:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(('', 0))
    _, port = sock.getsockname()
    return port

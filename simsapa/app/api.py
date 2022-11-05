from pathlib import Path
import socket
import os
from typing import List, Optional
from flask import Flask, jsonify, send_from_directory, abort, request
from flask.wrappers import Response
from flask_cors import CORS

from simsapa import APP_QUEUES, PACKAGE_ASSETS_DIR, USER_DB_PATH, DbSchemaName
from simsapa import logger
from simsapa.app.db_helpers import find_or_create_db, get_db_engine_connection_session

from .types import GraphRequest, USutta, UDictWord

from .db import appdata_models as Am
from .db import userdata_models as Um

from .graph import (all_nodes_and_edges, generate_graph, sutta_nodes_and_edges,
                    dict_word_nodes_and_edges, sutta_graph_id)

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


def _get_sutta_by_uid(uid: str) -> Optional[USutta]:
    _, _, db_session = get_db_engine_connection_session()

    results: List[USutta] = []

    res = db_session \
        .query(Am.Sutta) \
        .filter(Am.Sutta.uid == uid) \
        .all()
    results.extend(res)

    res = db_session \
        .query(Um.Sutta) \
        .filter(Um.Sutta.uid == uid) \
        .all()
    results.extend(res)

    if len(results) == 0:
        logger.warn("No Sutta found with uid: %s" % uid)
        return None

    return results[0]

def _get_word_by_uid(uid: str) -> Optional[UDictWord]:
    _, _, db_session = get_db_engine_connection_session()

    results: List[UDictWord] = []

    res = db_session \
        .query(Am.DictWord) \
        .filter(Am.DictWord.uid == uid) \
        .all()
    results.extend(res)

    res = db_session \
        .query(Um.DictWord) \
        .filter(Um.DictWord.uid == uid) \
        .all()
    results.extend(res)

    if len(results) == 0:
        logger.warn("No DictWord found with uid: %s" % uid)
        return None

    return results[0]

@app.route('/generate_graph', methods=['POST'])
def api_generate_graph():
    try:
        if request.json is None:
            return "Bad Request", 400

        p: GraphRequest = request.json

        if p['sutta_uid'] is not None:
            sutta = _get_sutta_by_uid(p['sutta_uid'])
        else:
            sutta = None

        if p['dict_word_uid'] is not None:
            dict_word = _get_word_by_uid(p['dict_word_uid'])
        else:
            dict_word = None

        if sutta is not None:
            (nodes, edges) = sutta_nodes_and_edges(sutta, distance=p['distance'])

            selected = []
            for idx, n in enumerate(nodes):
                if n[0] == sutta_graph_id(sutta):
                    selected.append(idx)

        elif dict_word is not None:
            (nodes, edges) = dict_word_nodes_and_edges(dict_word, distance=p['distance'])

            selected = []
            for idx, n in enumerate(nodes):
                if n[0] == sutta_graph_id(dict_word):
                    selected.append(idx)

        else:
            (nodes, edges) = all_nodes_and_edges()
            selected = []

        generate_graph(nodes,
                       edges,
                       selected,
                       p['queue_id'],
                       Path(p['graph_path']),
                       p['messages_url'],
                       p['labels'],
                       p['min_links'],
                       (p['width'], p['height']))

        hits = len(nodes) - 1

        result = (p['graph_gen_timestamp'], hits, str(p['graph_path']))

        return jsonify(result), 200

    except Exception as e:
        msg = "%s" % e
        logger.error(msg)

        return msg, 503

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

    # Run this here, not in AppData.__init__(), so that db_helpers module loads
    # in this thread and doesn't block the main thread opening the first window.
    find_or_create_db(USER_DB_PATH, DbSchemaName.UserData.value)

    app.run(host='127.0.0.1', port=port, debug=False, load_dotenv=False)

def find_available_port() -> int:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(('', 0))
    _, port = sock.getsockname()
    return port

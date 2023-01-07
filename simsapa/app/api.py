import queue
from pathlib import Path
import json
import socket
import os
import sys
from typing import Dict, List, Optional
from flask import Flask, jsonify, send_from_directory, abort, request
from flask.wrappers import Response
from flask_cors import CORS
import logging

from simsapa import PACKAGE_ASSETS_DIR, USER_DB_PATH, DbSchemaName
from simsapa import logger
from simsapa.app.db_helpers import find_or_create_db, get_db_engine_connection_session

from .types import GraphRequest, UBookmark, USutta, UDictWord

from sqlalchemy.sql.elements import and_, or_

from .db import appdata_models as Am
from .db import userdata_models as Um

from .graph import (all_nodes_and_edges, generate_graph, sutta_nodes_and_edges,
                    dict_word_nodes_and_edges, sutta_graph_id)

app = Flask(__name__)
app.config['ENV'] = 'development'
cors = CORS(app)
logging.getLogger("werkzeug").disabled = True

global server_queue
server_queue: Optional[queue.Queue] = None

@app.route('/queues/<string:queue_id>', methods=['POST'])
def queues(queue_id):
    if request.content_type == 'application/json':
        try:
            msg = request.get_json(cache=False)
            if msg is None:
                logger.error("can't deserialize message")
                abort(400)

            msg['queue_id'] = queue_id

        except Exception as e:
            abort(Response(f"{e}", 403))

        logger.info(f"QueueResource.on_post() msg: {msg}")

    else:
        abort(400)

    if queue_id is None or queue_id == '':
        logger.error("queue_id is missing")
        abort(404)

    if server_queue is None:
        logger.error("server_queue is None")
        abort(404)

    server_queue.put_nowait(json.dumps(msg))

    return 'OK', 200

@app.route('/assets/<path:path>', methods=['GET'])
def assets(path):
    if not os.path.isfile(os.path.join(PACKAGE_ASSETS_DIR, path)): # type: ignore
        abort(404)

    return send_from_directory(PACKAGE_ASSETS_DIR, path) # type: ignore


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

    db_session.close()

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

    db_session.close()

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


def _bm_to_res(x: UBookmark) -> Dict[str, str]:
    return {
        'quote': str(x.quote) if x.quote is not None else '',
        'selection_range': str(x.selection_range) if x.selection_range is not None else '',
        'comment_text': str(x.comment_text) if x.comment_text is not None else '',
        'comment_attr_json': str(x.comment_attr_json) if x.comment_attr_json is not None else '',
        'bookmark_schema_id': f"{x.metadata.schema}-{x.id}",
    }


@app.route('/get_bookmarks_with_range_for_sutta', methods=['POST'])
def get_bookmarks_with_range_for_sutta():
    data = request.get_json()
    if not data or 'sutta_uid' not in data.keys():
        return "Missing sutta_uid", 400

    sutta_uid = data['sutta_uid']
    result = list(map(_bm_to_res, _get_bookmarks_with_range_for_sutta(sutta_uid)))
    return jsonify(result), 200


@app.route('/get_bookmarks_with_quote_only_for_sutta', methods=['POST'])
def get_bookmarks_with_quote_only_for_sutta():
    data = request.get_json()
    if not data or 'sutta_uid' not in data.keys():
        return "Missing sutta_uid", 400

    sutta_uid = data['sutta_uid']
    result = list(map(_bm_to_res, _get_bookmarks_with_quote_only_for_sutta(sutta_uid)))
    return jsonify(result), 200


def _get_bookmarks_with_quote_only_for_sutta(sutta_uid: str, except_quote: str = "") -> List[UBookmark]:
    _, _, db_session = get_db_engine_connection_session()
    res = []

    r = db_session \
        .query(Am.Bookmark) \
        .filter(and_(
            Am.Bookmark.sutta_uid == sutta_uid,
            or_(Am.Bookmark.selection_range.is_(None),
                Am.Bookmark.selection_range == ""),
            Am.Bookmark.quote.is_not(None),
            Am.Bookmark.quote != "",
            Am.Bookmark.quote != except_quote,
        )) \
        .all()
    res.extend(r)

    r = db_session \
        .query(Um.Bookmark) \
        .filter(and_(
            Um.Bookmark.sutta_uid == sutta_uid,
            or_(Um.Bookmark.selection_range.is_(None),
                Um.Bookmark.selection_range == ""),
            Um.Bookmark.quote.is_not(None),
            Um.Bookmark.quote != "",
            Um.Bookmark.quote != except_quote,
        )) \
        .all()
    res.extend(r)

    db_session.close()

    return res


def _get_bookmarks_with_range_for_sutta(sutta_uid: str, except_quote = "") -> List[UBookmark]:
    _, _, db_session = get_db_engine_connection_session()
    res = []

    r = db_session \
        .query(Am.Bookmark) \
        .filter(and_(
            Am.Bookmark.sutta_uid == sutta_uid,
            Am.Bookmark.selection_range.is_not(None),
            Am.Bookmark.selection_range != "",
            Am.Bookmark.quote.is_not(None),
            Am.Bookmark.quote != "",
            Am.Bookmark.quote != except_quote,
        )) \
        .all()
    res.extend(r)

    r = db_session \
        .query(Um.Bookmark) \
        .filter(and_(
            Um.Bookmark.sutta_uid == sutta_uid,
            Um.Bookmark.selection_range.is_not(None),
            Um.Bookmark.selection_range != "",
            Um.Bookmark.quote.is_not(None),
            Um.Bookmark.quote != "",
            Um.Bookmark.quote != except_quote,
        )) \
        .all()
    res.extend(r)

    db_session.close()

    return res

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

def start_server(port: int, q: queue.Queue):
    logger.info(f'Starting server on port {port}')
    os.environ["FLASK_ENV"] = "development"

    global server_queue
    server_queue = q

    # Run this here, not in AppData.__init__(), so that db_helpers module loads
    # in this thread and doesn't block the main thread opening the first window.
    find_or_create_db(USER_DB_PATH, DbSchemaName.UserData.value)

    # Error in click.utils.echo() when console is unavailable
    # https://github.com/pallets/click/issues/2415
    if getattr(sys, 'frozen', False):
        f = open(os.devnull, 'w')
        sys.stdin = f
        sys.stdout = f

    app.run(host='127.0.0.1', port=port, debug=False, load_dotenv=False)

def find_available_port() -> int:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(('', 0))
    _, port = sock.getsockname()
    return port

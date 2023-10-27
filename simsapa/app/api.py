from pathlib import Path
import queue, json, os
from typing import Callable, Dict, List, Optional
from flask import Flask, jsonify, send_from_directory, abort, request
from flask.wrappers import Response
from flask_cors import CORS
import logging

from simsapa import PACKAGE_ASSETS_DIR
from simsapa import logger
from simsapa.app.db_session import get_db_engine_connection_session

from simsapa.app.types import GraphRequest, UBookmark, USutta, UDictWord

from sqlalchemy import and_, or_

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

app = Flask(__name__)
app.config['ENV'] = 'development'
cors = CORS(app)
logging.getLogger("werkzeug").disabled = True

global server_queue
server_queue: Optional[queue.Queue] = None

class AppCallbacks:
    open_window: Callable[[str], None]
    run_lookup_query: Callable[[str], None]
    def __init__(self):
        pass

global app_callbacks
app_callbacks = AppCallbacks()

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

@app.route('/assets/<path:filename>', methods=['GET'])
def assets(filename):
    if not os.path.isfile(os.path.join(PACKAGE_ASSETS_DIR, filename)): # type: ignore
        logger.error(f"api::assets(): File Not Found: {filename}")
        abort(404)

    return send_from_directory(PACKAGE_ASSETS_DIR, filename) # type: ignore


def _get_sutta_by_uid(uid: str) -> Optional[USutta]:
    db_eng, db_conn, db_session = get_db_engine_connection_session()

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

    db_conn.close()
    db_session.close()
    db_eng.dispose()

    if len(results) == 0:
        logger.warn("No Sutta found with uid: %s" % uid)
        return None

    return results[0]

def _get_word_by_uid(uid: str) -> Optional[UDictWord]:
    db_eng, db_conn, db_session = get_db_engine_connection_session()

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

    db_conn.close()
    db_session.close()
    db_eng.dispose()

    if len(results) == 0:
        logger.warn("No DictWord found with uid: %s" % uid)
        return None

    return results[0]

@app.route('/generate_graph', methods=['POST'])
def api_generate_graph():
    from simsapa.app.graph import (all_nodes_and_edges, generate_graph, sutta_nodes_and_edges,
                                   dict_word_nodes_and_edges, sutta_graph_id)

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

        # Append extra JS here, no idea how to include it in the bokeh template.
        with open(p['graph_path'], 'r', encoding='utf-8') as f:
            html = f.read()

        extra_js = """
<script src='qrc:///qtwebchannel/qwebchannel.js'></script>
<script>
document.addEventListener("DOMContentLoaded", function(event) {
    new QWebChannel(qt.webChannelTransport, function (channel) {
        document.qt_channel = channel;
    });
});
</script>
        """.strip()

        html = html.replace("</head>", f"{extra_js}</head>")

        with open(p['graph_path'], 'w', encoding='utf-8') as f:
            f.write(html)

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

@app.route('/lookup_window_query/<query_text>', methods=['GET'])
def lookup_window_query(query_text: str = ''):
    if len(query_text) == 0:
        return "OK", 200
    app_callbacks.run_lookup_query(query_text)
    return "OK", 200

@app.route('/open_window', defaults={'window_type': ''})
@app.route('/open_window/<string:window_type>', methods=['GET'])
def open_window(window_type: str = ''):
    app_callbacks.open_window(window_type)
    return "OK", 200

@app.route('/get_bookmarks_with_quote_only_for_sutta', methods=['POST'])
def get_bookmarks_with_quote_only_for_sutta():
    data = request.get_json()
    if not data or 'sutta_uid' not in data.keys():
        return "Missing sutta_uid", 400

    sutta_uid = data['sutta_uid']
    result = list(map(_bm_to_res, _get_bookmarks_with_quote_only_for_sutta(sutta_uid)))
    return jsonify(result), 200


def _get_bookmarks_with_quote_only_for_sutta(sutta_uid: str, except_quote: str = "") -> List[UBookmark]:
    db_eng, db_conn, db_session = get_db_engine_connection_session()
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

    db_conn.close()
    db_session.close()
    db_eng.dispose()

    return res


def _get_bookmarks_with_range_for_sutta(sutta_uid: str, except_quote = "") -> List[UBookmark]:
    db_eng, db_conn, db_session = get_db_engine_connection_session()
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

    db_conn.close()
    db_session.close()
    db_eng.dispose()

    return res

@app.errorhandler(400)
def resp_bad_request(e):
    msg = f"Bad Request: {e}"
    logger.error(msg)
    return msg, 400

@app.errorhandler(404)
def resp_not_found(e):
    msg = f"Not Found: {e}"
    logger.error(msg)
    return msg, 404

@app.errorhandler(403)
def resp_forbidden(e):
    msg = f"Forbidden: {e}"
    logger.error(msg)
    return msg, 403

def start_server(port: int,
                 q: queue.Queue,
                 open_window_fn: Callable[[str], None],
                 run_lookup_query_fn: Callable[[str], None]):
    logger.info(f'Starting server on port {port}')

    global server_queue
    server_queue = q

    global app_callbacks
    app_callbacks.open_window = open_window_fn
    app_callbacks.run_lookup_query = run_lookup_query_fn

    app.run(host='127.0.0.1', port=port, debug=False, load_dotenv=False)

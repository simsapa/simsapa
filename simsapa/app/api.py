from pathlib import Path
import queue, json, os
from typing import Callable, Dict, List, Optional, TypedDict
from flask import Flask, jsonify, send_from_directory, abort, request
from flask.wrappers import Response
from flask_cors import CORS
import logging

from simsapa import PACKAGE_ASSETS_DIR, SERVER_QUEUE, ApiAction, ApiMessage, DbSchemaName
from simsapa import logger, SearchResult
from simsapa.app.completion_lists import get_and_save_completions
from simsapa.app.db_session import get_db_engine_connection_session

from simsapa.app.types import GraphRequest, SearchArea, SearchMode, SearchParams, UBookmark, USutta, UDictWord

from sqlalchemy import and_, or_

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

from simsapa.app.db import dpd_models as Dpd
from simsapa.dpd_db.tools.pali_sort_key import pali_sort_key

app = Flask(__name__)
app.config['ENV'] = 'development'
cors = CORS(app)
logging.getLogger("werkzeug").disabled = True

global server_queue
server_queue: Optional[queue.Queue] = None

class ApiSearchResult(TypedDict):
    hits: Optional[int]
    results: List[SearchResult]
    deconstructor: List[str]

class AppCallbacks:
    open_window: Callable[[str], None]
    run_lookup_query: Callable[[str], None]
    run_suttas_fulltext_search: Callable[[str, SearchParams, int], ApiSearchResult]
    run_dict_combined_search: Callable[[str, SearchParams, int], ApiSearchResult]

    def __init__(self):
        pass

global app_callbacks
app_callbacks = AppCallbacks()

@app.route('/', methods=['GET'])
def route_index():
    return 'OK', 200

@app.route('/queues/<string:queue_id>', methods=['POST'])
def route_queues(queue_id):
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
def route_assets(filename):
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

    res = db_session \
        .query(Dpd.PaliWord) \
        .filter(Dpd.PaliWord.uid == uid) \
        .all()
    results.extend(res)

    res = db_session \
        .query(Dpd.PaliRoot) \
        .filter(Dpd.PaliRoot.uid == uid) \
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
def route_api_generate_graph():
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

@app.route('/suttas_fulltext_search', methods=['POST'])
def route_suttas_fulltext_search():
    data = request.get_json()
    if not data or 'query_text' not in data.keys():
        return "Missing query_text", 400

    if 'page_num' not in data.keys():
        page_num = 0
    else:
        page_num = int(data['page_num'])

    if 'suttas_lang' not in data.keys():
        lang = 'Languages'
    else:
        lang = data['suttas_lang']

    if 'suttas_lang_include' not in data.keys():
        lang_include = True
    else:
        lang_include = data['suttas_lang_include']

    params = SearchParams(
        mode = SearchMode.FulltextMatch,
        page_len = 20,
        lang = lang,
        lang_include = lang_include,
        source = None,
        source_include = True,
        enable_regex = False,
        fuzzy_distance = 0,
    )

    res = app_callbacks.run_suttas_fulltext_search(data['query_text'].strip(), params, page_num)

    return jsonify(res), 200

@app.route('/dict_combined_search', methods=['POST'])
def route_dict_combined_search():
    data = request.get_json()
    if not data or 'query_text' not in data.keys():
        return "Missing query_text", 400

    if 'page_num' not in data.keys():
        page_num = 0
    else:
        page_num = int(data['page_num'])

    if 'dict_lang' not in data.keys():
        lang = 'Languages'
    else:
        lang = data['dict_lang']

    if 'dict_lang_include' not in data.keys():
        lang_include = True
    else:
        lang_include = data['dict_lang_include']

    if 'dict_dict' not in data.keys():
        dict = 'Dictionaries'
    else:
        dict = data['dict_dict']

    if 'dict_dict_include' not in data.keys():
        dict_include = True
    else:
        dict_include = data['dict_dict_include']

    params = SearchParams(
        mode = SearchMode.Combined,
        page_len = 20,
        lang = lang,
        lang_include = lang_include,
        source = dict,
        source_include = dict_include,
        enable_regex = False,
        fuzzy_distance = 0,
    )

    res = app_callbacks.run_dict_combined_search(data['query_text'].strip(), params, page_num)

    return jsonify(res), 200

@app.route('/dict_words_flat_completion_list', methods=['GET'])
def route_dict_words_flat_completion_list():
    logger.info('/dict_words_flat_completion_list')
    db_eng, db_conn, db_session = get_db_engine_connection_session()

    # NOTE: This gives a very long list, a 31 MB json response.
    #
    # r = get_and_save_completions(db_session, SearchArea.DictWords)
    # # Flatten the lists into a single list of strings
    # results = [item for sublist in r.values() for item in sublist]
    #
    # Instead, load the words and roots from the DPD, which yields a 1.6 MB list.

    res = db_session.query(Dpd.PaliWord.pali_1).all()
    res.extend(db_session.query(Dpd.PaliRoot.root_no_sign).all())
    results: List[str] = list(map(lambda x: x[0].strip() or 'none', res))

    results = sorted(results, key=lambda x: pali_sort_key(x))

    db_conn.close()
    db_session.close()
    db_eng.dispose()

    return jsonify(results), 200

@app.route('/sutta_titles_flat_completion_list', methods=['GET'])
def route_sutta_titles_flat_completion_list():
    logger.info('/sutta_titles_flat_completion_list')
    db_eng, db_conn, db_session = get_db_engine_connection_session()

    r = get_and_save_completions(db_session, SearchArea.Suttas)
    results = [item for sublist in r.values() for item in sublist]

    results = sorted(results, key=lambda x: pali_sort_key(x))

    db_conn.close()
    db_session.close()
    db_eng.dispose()

    return jsonify(results), 200

@app.route('/get_bookmarks_with_range_for_sutta', methods=['POST'])
def route_get_bookmarks_with_range_for_sutta():
    data = request.get_json()
    if not data or 'sutta_uid' not in data.keys():
        return "Missing sutta_uid", 400

    sutta_uid = data['sutta_uid']
    result = list(map(_bm_to_res, _get_bookmarks_with_range_for_sutta(sutta_uid)))
    return jsonify(result), 200

@app.route('/lookup_window_query/<string:word>', methods=['GET'])
@app.route('/lookup_window_query/<string:word>/<string:dict_label>', methods=['GET'])
def route_lookup_window_query_get(word: str = '', dict_label = ''):
    logger.info(f"route_lookup_window_query_get() {word} {dict_label}")
    if len(word) == 0:
        return "OK", 200

    uid = "/".join([i for i in [word, dict_label] if i != ""])

    app_callbacks.run_lookup_query(uid)
    return "OK", 200

@app.route('/lookup_window_query', methods=['POST'])
def route_lookup_window_query_post():
    data = request.get_json()
    if not data:
        return "Missing data", 400

    if 'query_text' not in data.keys():
        return "Missing query_text", 400

    query_text = str(data['query_text'])
    if len(query_text) == 0:
        return "OK", 200

    app_callbacks.run_lookup_query(query_text)

    return "OK", 200

@app.route('/suttas/<string:sutta_ref>', methods=['GET'])
@app.route('/suttas/<string:sutta_ref>/<string:lang>', methods=['GET'])
@app.route('/suttas/<string:sutta_ref>/<string:lang>/<string:source_uid>', methods=['GET'])
def route_suttas(sutta_ref = '', lang = '', source_uid = ''):
    logger.info(f"route_suttas() {sutta_ref} {lang} {source_uid}")

    api_msg = ApiMessage(
        queue_id='app_windows',
        action = ApiAction.show_sutta_by_url,
        data = request.url
    )
    SERVER_QUEUE.put_nowait(json.dumps(api_msg))

    uid = "/".join([i for i in [sutta_ref, lang, source_uid] if i != ""])

    text_msg = f"The Simsapa window should appear with '{uid}'. You can close this tab."

    return text_msg, 200

@app.route('/words/<string:word>', methods=['GET'])
@app.route('/words/<string:word>/<string:dict_label>', methods=['GET'])
def route_words(word = '', dict_label = ''):
    logger.info(f"route_words() {word} {dict_label}")

    api_msg = ApiMessage(
        queue_id='app_windows',
        action = ApiAction.show_word_by_url,
        data = request.url
    )
    SERVER_QUEUE.put_nowait(json.dumps(api_msg))

    uid = "/".join([i for i in [word, dict_label] if i != ""])

    text_msg = f"The Simsapa window should appear with '{uid}'. You can close this tab."

    return text_msg, 200

@app.route('/words/<string:word>.json', methods=['GET'])
@app.route('/words/<string:word>/<string:dict_label>.json', methods=['GET'])
def route_words_json(word = '', dict_label = ''):
    logger.info(f"route_words_json() {word} {dict_label}")

    if dict_label == '':
        source = None
    else:
        source = dict_label

    params = SearchParams(
        mode = SearchMode.Combined,
        page_len = 20,
        lang = None,
        lang_include = True,
        source = source,
        source_include = True,
        enable_regex = False,
        fuzzy_distance = 0,
    )

    res = app_callbacks.run_dict_combined_search(word, params, 0)

    if len(res['results']) == 0:
        return jsonify([]), 200

    db_eng, db_conn, db_session = get_db_engine_connection_session()

    res_dicts: List[dict] = []

    for i in res['results']:
        r: Optional[UDictWord] = None

        if i['schema_name'] == DbSchemaName.AppData.value:
            r = db_session.query(Am.DictWord) \
                          .filter(Am.DictWord.uid == i['uid']).first()

        elif i['schema_name'] == DbSchemaName.UserData.value:
            r = db_session.query(Um.DictWord) \
                          .filter(Um.DictWord.uid == i['uid']).first()

        elif i['schema_name'] == DbSchemaName.Dpd.value:
            if i['table_name'] == "pali_words":
                r = db_session.query(Dpd.PaliWord) \
                            .filter(Dpd.PaliWord.uid == i['uid']).first()

            elif i['table_name'] == "pali_roots":
                r = db_session.query(Dpd.PaliRoot) \
                            .filter(Dpd.PaliRoot.uid == i['uid']).first()

            else:
                continue

        else:
            continue

        if r is None:
            continue

        res_dicts.append(r.as_dict)

    db_conn.close()
    db_session.close()
    db_eng.dispose()

    return jsonify(res_dicts), 200

@app.route('/open_window', defaults={'window_type': ''})
@app.route('/open_window/<string:window_type>', methods=['GET'])
def route_open_window(window_type: str = ''):
    app_callbacks.open_window(window_type)
    return "OK", 200

@app.route('/get_bookmarks_with_quote_only_for_sutta', methods=['POST'])
def route_get_bookmarks_with_quote_only_for_sutta():
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
                 run_lookup_query_fn: Callable[[str], None],
                 run_suttas_fulltext_search_fn: Callable[[str, SearchParams, int], ApiSearchResult],
                 run_dict_combined_search_fn: Callable[[str, SearchParams, int], ApiSearchResult]):
    logger.info(f'Starting server on port {port}')

    global server_queue
    server_queue = q

    global app_callbacks
    app_callbacks.open_window = open_window_fn
    app_callbacks.run_lookup_query = run_lookup_query_fn
    app_callbacks.run_suttas_fulltext_search = run_suttas_fulltext_search_fn
    app_callbacks.run_dict_combined_search = run_dict_combined_search_fn

    app.run(host='127.0.0.1', port=port, debug=False, load_dotenv=False)

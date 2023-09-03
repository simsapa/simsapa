import re
from typing import List, Tuple, TypedDict, Optional
from pathlib import Path
from itertools import chain
from bokeh.document.locking import UnlockedDocumentProxy
from bokeh.models.renderers import GraphRenderer, GlyphRenderer

import networkx as nx
from bokeh.io import output_file, save, curdoc
from bokeh.document import Document
from bokeh.models import (Div, Circle, MultiLine, Range1d, ColumnDataSource,
                          LabelSet, HoverTool, NodesAndLinkedEdges,
                          EdgesAndLinkedNodes)
from bokeh.palettes import Spectral8
from bokeh.plotting import figure, from_networkx, Figure
from bokeh.layouts import column
from bokeh.models import CustomJS

import networkx
from bokeh.transform import linear_cmap

from sqlalchemy.orm.session import Session
from sqlalchemy import and_

from simsapa import ShowLabels
from simsapa.app.db_session import get_db_engine_connection_session

from .db import appdata_models as Am
from .db import userdata_models as Um

from .types import UDictWord, USutta, UDocument, ULink


class NodeData(TypedDict):
    label: str
    title: str
    description: str
    table: str
    id: int
    fill_color: str


GraphNode = Tuple[str, NodeData]

GraphEdge = Tuple[str, str]


class DocumentPage(TypedDict):
    doc: UDocument
    page_number: Optional[int]


def sutta_graph_id(x: USutta) -> str:
    schema = x.metadata.schema
    return f"{schema}.suttas.{x.id}"


def dict_word_graph_id(x: UDictWord) -> str:
    schema = x.metadata.schema
    return f"{schema}.dict_words.{x.id}"


def document_graph_id(x: DocumentPage) -> str:
    doc = x['doc']
    page = x['page_number']
    schema = doc.metadata.schema
    if page is None:
        return f"{schema}.documents.{doc.id}"
    else:
        return f"{schema}.documents.{doc.id}.{page}"


def sutta_to_node(x: USutta) -> GraphNode:
    schema = x.metadata.schema
    return (
        sutta_graph_id(x),
        NodeData(
            label = str(x.sutta_ref),
            title = str(x.title),
            description = '',
            table = f'{schema}.suttas',
            id = int(str(x.id)),
            fill_color = Spectral8[0],
        )
    )


def dict_word_to_node(x: UDictWord) -> GraphNode:
    schema = x.metadata.schema
    return (
        dict_word_graph_id(x),
        NodeData(
            label = str(x.word),
            title = str(x.word),
            description = '',
            table = f'{schema}.dict_words',
            id = int(str(x.id)),
            fill_color = Spectral8[1],
        )
    )


def document_page_to_node(x: DocumentPage) -> GraphNode:
    doc = x['doc']
    page_number = x['page_number']
    schema = doc.metadata.schema
    if page_number is None:
        label = doc.title
    else:
        label = f"{doc.title} (p.{page_number})"
    return (
        document_graph_id(x),
        NodeData(
            label = str(label),
            title = str(doc.title),
            description = '',
            table = f'{schema}.documents',
            id = int(str(doc.id)),
            fill_color = Spectral8[4],
        )
    )


def unique_nodes(nodes: List[GraphNode]) -> List[GraphNode]:
    listed = []
    unique_nodes = []
    for i in nodes:
        e = i[0]
        if e not in listed:
            listed.append(e)
            unique_nodes.append(i)

    unique_nodes.sort(key=lambda x: x[0])

    return unique_nodes


def unique_edges(edges: List[GraphEdge]) -> List[GraphEdge]:
    listed = []
    unique_edges = []
    for i in edges:
        e = f"{i[0]},{i[1]}"
        if e not in listed:
            listed.append(e)
            unique_edges.append(i)

    unique_edges.sort(key=lambda x: f"{x[0]},{x[1]}")

    return unique_edges


def get_related_suttas(db_session: Session, sutta: USutta) -> List[USutta]:
    uid_ref = re.sub('^([^/]+)/.*', r'\1', str(sutta.uid))

    res: List[USutta] = []
    r = db_session \
        .query(Am.Sutta) \
        .filter(and_(
            Am.Sutta.uid != sutta.uid,
            Am.Sutta.uid.like(f"{uid_ref}/%"),
        )) \
        .all()
    res.extend(r)

    r = db_session \
        .query(Um.Sutta) \
        .filter(and_(
            Um.Sutta.uid != sutta.uid,
            Um.Sutta.uid.like(f"{uid_ref}/%"),
        )) \
        .all()
    res.extend(r)

    return res


def sutta_nodes_and_edges(sutta: USutta, distance: int = 1):
    # NOTE: Don't pass Db Session in the argument. Db Session must remain within the same thread, otherwise causes exception:
    # Exception closing connection <sqlite3.Connection object at 0x7f7b83c6a4e0>
    db_eng, db_conn, db_session = get_db_engine_connection_session()

    links = {'appdata.suttas': [], 'userdata.suttas': []}
    sutta_ids = {'appdata.suttas': [], 'userdata.suttas': []}

    related_suttas = get_related_suttas(db_session, sutta)
    related_suttas.append(sutta)

    related_ids = list(map(lambda x: x.id, related_suttas))

    for s in related_suttas:
        for DbLink in [Am.Link, Um.Link]:
            for table in ['appdata.suttas', 'userdata.suttas']:

                r = db_session \
                    .query(DbLink.to_id) \
                    .filter(DbLink.from_table == table) \
                    .filter(DbLink.to_table == table) \
                    .filter(DbLink.from_id == s.id) \
                    .all()

                links[table].extend(r)

                r = db_session \
                    .query(DbLink.from_id) \
                    .filter(DbLink.from_table == table) \
                    .filter(DbLink.to_table == table) \
                    .filter(DbLink.to_id == s.id) \
                    .all()

                links[table].extend(r)

    for table in ['appdata.suttas', 'userdata.suttas']:
        # results IDs, exluding the the current sutta ID (i.e. suttas connecting to it)
        ids = filter(lambda x: x not in related_ids, map(lambda x: x[0], links[table]))
        # set() will contain unique items
        sutta_ids[table] = list(set(ids))

    suttas = []

    r = db_session \
                .query(Am.Sutta) \
                .filter(Am.Sutta.id.in_(sutta_ids['appdata.suttas'])) \
                .all()
    suttas.extend(r)

    r = db_session \
                .query(Um.Sutta) \
                .filter(Um.Sutta.id.in_(sutta_ids['userdata.suttas'])) \
                .all()
    suttas.extend(r)

    nodes = list(map(sutta_to_node, suttas))

    def to_edge(x: USutta):
        from_id = sutta_graph_id(sutta)
        to_id = sutta_graph_id(x)
        if to_id < from_id:
            return (to_id, from_id)
        else:
            return (from_id, to_id)

    edges = list(map(to_edge, suttas))

    # Close the session here, so that it's not the context manager in another
    # thread that has to close it.
    db_conn.close()
    db_session.close()
    db_eng.dispose()

    # Collect links from other nodes

    if distance > 1:
        for i in suttas:
            (n, e) = sutta_nodes_and_edges(sutta=i, distance=distance - 1)
            nodes.extend(n)
            edges.extend(e)

    # Append the current sutta as a node
    nodes.append(sutta_to_node(sutta))

    return (unique_nodes(nodes), unique_edges(edges))


def dict_word_nodes_and_edges(dict_word: UDictWord, distance: int = 1):
    db_eng, db_conn, db_session = get_db_engine_connection_session()

    schema = dict_word.metadata.schema

    links = []

    r = db_session \
        .query(Um.Link.to_id) \
        .filter(Um.Link.from_table == f"{schema}.dict_words") \
        .filter(Um.Link.to_table == "appdata.suttas") \
        .filter(Um.Link.from_id == dict_word.id) \
        .all()

    links.extend(r)

    r = db_session \
        .query(Um.Link.from_id) \
        .filter(Um.Link.from_table == f"{schema}.dict_words") \
        .filter(Um.Link.to_table == "appdata.suttas") \
        .filter(Um.Link.to_id == dict_word.id) \
        .all()

    links.extend(r)

    ids = map(lambda x: x[0], links)
    # set() will contain unique items
    sutta_ids = list(set(ids))

    suttas = db_session \
        .query(Am.Sutta) \
        .filter(Am.Sutta.id.in_(sutta_ids)) \
        .all()

    nodes = list(map(sutta_to_node, suttas))

    def to_edge(x: USutta):
        from_id = dict_word_graph_id(dict_word)
        to_id = sutta_graph_id(x)
        return (to_id, from_id)

    edges = list(map(to_edge, suttas))

    db_conn.close()
    db_session.close()
    db_eng.dispose()

    # Collect links from other nodes

    if distance > 1:
        for i in suttas:
            (n, e) = sutta_nodes_and_edges(sutta=i, distance=distance - 1)
            nodes.extend(n)
            edges.extend(e)

    # Append the current doument as a node

    nodes.append(dict_word_to_node(dict_word))

    return (unique_nodes(nodes), unique_edges(edges))


def document_page_nodes_and_edges(db_doc: Am.Document, page_number: int, distance: int = 1):
    db_eng, db_conn, db_session = get_db_engine_connection_session()

    links = db_session \
        .query(Um.Link.to_id) \
        .filter(Um.Link.from_table == "userdata.documents") \
        .filter(Um.Link.from_page_number == page_number) \
        .filter(Um.Link.to_table == "appdata.suttas") \
        .filter(Um.Link.from_id == db_doc.id) \
        .all()

    ids = map(lambda x: x[0], links)
    # set() will contain unique items
    sutta_ids = list(set(ids))

    suttas = db_session \
        .query(Am.Sutta) \
        .filter(Am.Sutta.id.in_(sutta_ids)) \
        .all()

    nodes = list(map(sutta_to_node, suttas))

    def to_edge(x: USutta):
        d = DocumentPage(doc=db_doc, page_number=page_number)
        from_id = document_graph_id(d)
        to_id = sutta_graph_id(x)
        return (to_id, from_id)

    edges = list(map(to_edge, suttas))

    db_conn.close()
    db_session.close()
    db_eng.dispose()

    # Collect links from other nodes

    if distance > 1:
        for i in suttas:
            (n, e) = sutta_nodes_and_edges(sutta=i, distance=distance - 1)
            nodes.extend(n)
            edges.extend(e)

    # Append the current doument as a node

    d = DocumentPage(doc=db_doc, page_number=page_number)
    nodes.append(document_page_to_node(d))

    return (unique_nodes(nodes), unique_edges(edges))


def _suttas_from_links(db_session: Session, links: List[Um.Link]) -> List[USutta]:
    results: List[USutta] = []

    def to_appdata_sutta_ids(x: Um.Link):
        ids = []
        if x.from_table == 'appdata.suttas':
            ids.append(x.from_id)
        if x.to_table == 'appdata.suttas':
            ids.append(x.to_id)
        return ids

    a = list(map(to_appdata_sutta_ids, links))
    db_ids = list(set(chain.from_iterable(a)))

    r = db_session \
        .query(Am.Sutta) \
        .filter(Am.Sutta.id.in_(db_ids)) \
        .all()

    results.extend(r)

    return results


def _dict_words_from_links(db_session: Session, links: List[Um.Link]) -> List[UDictWord]:
    results: List[UDictWord] = []

    def to_appdata_word_ids(x: Um.Link):
        ids = []
        if x.from_table == 'appdata.dict_words':
            ids.append(x.from_id)
        if x.to_table == 'appdata.dict_words':
            ids.append(x.to_id)
        return ids

    def to_userdata_word_ids(x: Um.Link):
        ids = []
        if x.from_table == 'userdata.dict_words':
            ids.append(x.from_id)
        if x.to_table == 'userdata.dict_words':
            ids.append(x.to_id)
        return ids

    a = list(map(to_appdata_word_ids, links))
    appdata_db_ids = list(set(chain.from_iterable(a)))

    a = list(map(to_userdata_word_ids, links))
    userdata_db_ids = list(set(chain.from_iterable(a)))

    r = db_session \
        .query(Am.DictWord) \
        .filter(Am.DictWord.id.in_(appdata_db_ids)) \
        .all()

    results.extend(r)

    r = db_session \
        .query(Um.DictWord) \
        .filter(Um.DictWord.id.in_(userdata_db_ids)) \
        .all()

    results.extend(r)

    return results


def _documents_and_pages_from_links(db_session: Session, links: List[Um.Link]) -> List[DocumentPage]:
    results: List[DocumentPage] = []

    def to_appdata_items(x: Um.Link):
        items = []
        if x.from_table == 'appdata.documents':
            items.append((x.from_id, x.from_page_number))
        if x.to_table == 'appdata.documents':
            items.append((x.to_id, x.to_page_number))
        return items

    def to_userdata_items(x: Um.Link):
        items = []
        if x.from_table == 'userdata.documents':
            items.append((x.from_id, x.from_page_number))
        if x.to_table == 'userdata.documents':
            items.append((x.to_id, x.to_page_number))
        return items

    a = list(map(to_appdata_items, links))
    appdata_items = list(set(chain.from_iterable(a)))
    appdata_ids = list(map(lambda x: x[0], appdata_items))

    a = list(map(to_userdata_items, links))
    userdata_items = list(set(chain.from_iterable(a)))
    userdata_ids = list(map(lambda x: x[0], userdata_items))

    appdata_db_res = db_session \
        .query(Am.Document) \
        .filter(Am.Document.id.in_(appdata_ids)) \
        .all()

    userdata_db_res = db_session \
        .query(Um.Document) \
        .filter(Um.Document.id.in_(userdata_ids)) \
        .all()

    def id_to_doc(x, db_res):
        id = x[0]
        page = x[1]
        doc = list(filter(lambda i: i.id == id, db_res))
        return DocumentPage(doc=doc[0], page_number=page)

    appdata_docpages = list(map(lambda x: id_to_doc(x, appdata_db_res), appdata_items))
    results.extend(appdata_docpages)

    userdata_docpages = list(map(lambda x: id_to_doc(x, userdata_db_res), userdata_items))
    results.extend(userdata_docpages)

    return results


def all_nodes_and_edges():
    db_eng, db_conn, db_session = get_db_engine_connection_session()

    links = []
    r = db_session.query(Am.Link).all()
    links.extend(r)
    r = db_session.query(Um.Link).all()
    links.extend(r)

    suttas = _suttas_from_links(db_session, links)
    words = _dict_words_from_links(db_session, links)
    documents_and_pages = _documents_and_pages_from_links(db_session, links)

    db_conn.close()
    db_session.close()
    db_eng.dispose()

    nodes = []
    nodes.extend(list(map(sutta_to_node, suttas)))
    nodes.extend(list(map(dict_word_to_node, words)))
    nodes.extend(list(map(document_page_to_node, documents_and_pages)))

    def from_agrees(x, link, postfix):
        from_schema = link.from_table.replace(postfix, '')
        return x.metadata.schema == from_schema and x.id == link.from_id

    def from_doc_page_agrees(x, link, postfix):
        from_schema = link.from_table.replace(postfix, '')
        return \
            x['doc'].metadata.schema == from_schema and x['doc'].id == link.from_id and \
            x['page_number'] == link.from_page_number

    def to_agrees(x, link, postfix):
        to_schema = link.to_table.replace(postfix, '')
        return x.metadata.schema == to_schema and x.id == link.to_id

    def to_doc_page_agrees(x, link, postfix):
        to_schema = link.to_table.replace(postfix, '')
        return \
            x['doc'].metadata.schema == to_schema and x['doc'].id == link.to_id and \
            x['page_number'] == link.to_page_number

    def to_edge(link: ULink):
        if link.from_table.endswith('.suttas'):
            sutta = list(filter(lambda x: from_agrees(x, link, '.suttas'), suttas))
            if len(sutta) == 1:
                from_edge_id = sutta_graph_id(sutta[0])
            else:
                from_edge_id = ''

        elif link.from_table.endswith('.dict_words'):
            word = list(filter(lambda x: from_agrees(x, link, '.dict_words'), words))
            if len(word) == 1:
                from_edge_id = dict_word_graph_id(word[0])
            else:
                from_edge_id = ''

        elif link.from_table.endswith('.documents'):
            doc = list(filter(lambda x: from_doc_page_agrees(x, link, '.documents'), documents_and_pages))
            if len(doc) == 1:
                from_edge_id = document_graph_id(doc[0])
            else:
                from_edge_id = ''

        else:
            from_edge_id = ''

        if link.to_table.endswith('.suttas'):
            sutta = list(filter(lambda x: to_agrees(x, link, '.suttas'), suttas))
            if len(sutta) == 1:
                to_edge_id = sutta_graph_id(sutta[0])
            else:
                to_edge_id = ''

        elif link.to_table.endswith('.dict_words'):
            word = list(filter(lambda x: to_agrees(x, link, '.dict_words'), words))
            if len(word) == 1:
                to_edge_id = dict_word_graph_id(word[0])
            else:
                to_edge_id = ''

        elif link.to_table.endswith('.documents'):
            doc = list(filter(lambda x: to_doc_page_agrees(x, link, '.documents'), documents_and_pages))
            if len(doc) == 1:
                to_edge_id = document_graph_id(doc[0])
            else:
                to_edge_id = ''

        else:
            to_edge_id = ''

        if to_edge_id < from_edge_id:
            return (to_edge_id, from_edge_id)
        else:
            return (from_edge_id, to_edge_id)

    edges = list(filter(lambda x: x[0] != '' and x[1] != '', map(to_edge, links)))

    return (unique_nodes(nodes), unique_edges(edges))


def generate_graph(nodes,
                   edges,
                   selected_indices: List[int],
                   queue_id: str,
                   output_path: Path,
                   messages_url: str,
                   show_labels: Optional[ShowLabels] = ShowLabels.SuttaRef,
                   min_degree: Optional[int] = None,
                   plot_size: Optional[Tuple[int, int]] = None):

    if len(nodes) == 0:
        return

    G = nx.Graph()
    G.add_nodes_from(nodes)
    G.add_edges_from(edges)

    if min_degree is not None and min_degree > 1 and len(G.nodes()) > 2:
        remove_nodes = [x for x in G.nodes() if G.degree(x) < min_degree]
        G.remove_nodes_from(remove_nodes)

    if len(G.nodes()) == 0:
        # Create an empty graph and render the html to show the user visible
        # feedback that no nodes were left
        G = nx.Graph()

    degrees = dict(networkx.degree(G))
    networkx.set_node_attributes(G, name='degree', values=degrees)

    number_to_adjust_by = 5
    adjusted_node_size = dict([(node, degree+number_to_adjust_by) for node, degree in networkx.degree(G)])
    networkx.set_node_attributes(G, name='adjusted_node_size', values=adjusted_node_size)

    size_by_this_attribute = 'adjusted_node_size'
    color_by_this_attribute = 'adjusted_node_size'

    if plot_size is not None:
        plot_width = plot_size[0]
        plot_height = plot_size[1]
    else:
        plot_width = 700
        plot_height = 400

    plot: Figure = figure(
        title=None,
        plot_width=plot_width,
        plot_height=plot_height,
        x_range=Range1d(-1.1, 1.1),
        y_range=Range1d(-1.1, 1.1),
        tools="pan,tap,wheel_zoom,reset",
        active_scroll="wheel_zoom",
    )

    plot.xgrid.grid_line_color = None
    plot.ygrid.grid_line_color = None
    plot.xaxis.axis_line_color = None
    plot.yaxis.axis_line_color = None

    plot.xaxis.axis_label_text_color = None
    plot.yaxis.axis_label_text_color = None

    plot.xaxis.major_label_text_color = None
    plot.yaxis.major_label_text_color = None

    plot.xaxis.major_tick_line_color = None
    plot.yaxis.major_tick_line_color = None
    plot.xaxis.minor_tick_line_color = None
    plot.yaxis.minor_tick_line_color = None

    network_graph: GraphRenderer = from_networkx(G, nx.spring_layout, scale=1.0, center=(0, 0), seed=100)

    selection_color = Spectral8[6]

    color_palette = Spectral8

    minimum_value_color = min(network_graph.node_renderer.data_source.data[color_by_this_attribute])
    maximum_value_color = max(network_graph.node_renderer.data_source.data[color_by_this_attribute])
    network_graph.node_renderer.glyph = Circle(size=size_by_this_attribute, fill_color=linear_cmap(color_by_this_attribute, color_palette, minimum_value_color, maximum_value_color))

    network_graph.node_renderer.selection_glyph = Circle(size=15, fill_color=selection_color)

    network_graph.edge_renderer.glyph = MultiLine(line_color="black", line_alpha=0.2, line_width=0.5)
    network_graph.edge_renderer.selection_glyph = MultiLine(line_color=selection_color, line_width=2)

    network_graph.selection_policy = NodesAndLinkedEdges()
    network_graph.inspection_policy = EdgesAndLinkedNodes()

    plot.renderers.append(network_graph)

    def _fmt(s: str) -> str:
        return s.replace('Sutta', '').replace('sutta', '')

    x, y = zip(*network_graph.layout_provider.graph_layout.values())
    source = ColumnDataSource({
        'x': x,
        'y': y,
        'label': [nodes[i][1]['label'] for i in range(len(x))],
        'title': [nodes[i][1]['label'] + ' ' + _fmt(nodes[i][1]['title']) for i in range(len(x))],
        'description': [nodes[i][1]['description'] for i in range(len(x))],
        'table': [nodes[i][1]['table'] for i in range(len(x))],
        'id': [nodes[i][1]['id'] for i in range(len(x))],
    })

    cr = plot.circle(x='x', y='y', source=source, size=25,
                     fill_color=None, line_color=None,
                     hover_fill_color=selection_color,
                     hover_alpha=0.8, hover_line_color="white")

    tooltips = [
        # ('Title', '@title'),
        # No need for the 'Title:' ... prefix
        ('', '@title'),
    ]

    hover = HoverTool(tooltips=tooltips, renderers=[cr])

    plot.add_tools(hover)

    if show_labels != ShowLabels.NoLabels:

        if show_labels == ShowLabels.SuttaRef:
            label_attr = 'label'
        else:
            # ShowLabels.RefAndTitle
            label_attr = 'title'

        labels = LabelSet(
            x='x',
            y='y',
            text=label_attr,
            source=source,
            text_color='black',
            text_alpha=0.8,
            text_font_size='15px',
            background_fill_color='white',
            background_fill_alpha=0.5,
        )

        plot.renderers.append(labels)

    hover_js_code = """
const indices = cb_data.index.indices;
if (indices.length > 0) {
    const table = source.data['table'][indices[0]];
    const id = source.data['id'][indices[0]];

    const data = {
        table: table,
        id: id,
    };

    document.qt_channel.objects.helper.link_mouseover_graph_node(JSON.stringify(data));
}
"""

    hover_cb = CustomJS(args=dict(source=source), code=hover_js_code)
    hover.callback = hover_cb

    network_graph.node_renderer.data_source.selected.indices = selected_indices

    js = """
window.selected_info = [];

var inds = cb_obj.indices;
var data = source.data;

for (var i=0; i<inds.length; i++) {
    var idx = inds[i];

    var table = data['table'][idx];
    var id = data['id'][idx];

    window.selected_info.push({ table: table, id: id });
}

if (window.selected_info.length > 0) {
    const params = {
        action: 'set_selected',
        data: JSON.stringify(window.selected_info[0]),
    };
    const options = {
        method: 'POST',
        headers: {
          'Accept': 'application/json',
          'Content-Type': 'application/json'
        },
        body: JSON.stringify(params),
    };
    fetch('%s', options);
}
""" % (messages_url,)

    network_graph.node_renderer.data_source.selected.js_on_change('indices', CustomJS(args=dict(source=source), code=js))

    text = """
<style>
    body {
        margin: -25px 0 0 -5px;
        padding: 0;
    }
    #selected_descriptions {
        font-family: Helvetica, Arial, sans-serif;
        padding: 0 30px;
    }

    h1.title {
        font-style: normal;
        font-weight: normal;
        font-size: 20px;
        line-height: 25px;
        letter-spacing: 0.5px;
        color: #1a1a1a;
    }

    .description {
        font-size: 16px;
        line-height: 20px;
    }
</style>"""

    header = Div(text=text)
    layout = column(header, plot)

    doc: Document = curdoc()
    doc.clear()
    doc.add_root(layout)

    output_file(filename=str(output_path), mode='inline')
    save(doc)

from typing import List, Tuple, TypedDict
from pathlib import Path

import networkx as nx
from bokeh.io import output_file, save, curdoc
from bokeh.document import Document
from bokeh.models import (Button, Circle, MultiLine, Range1d, ColumnDataSource,
                          LabelSet, HoverTool, NodesAndLinkedEdges,
                          EdgesAndLinkedNodes)
from bokeh.palettes import Spectral4
from bokeh.plotting import figure, from_networkx
from bokeh.layouts import column
from bokeh.models import CustomJS
from bokeh import events

from .db import appdata_models as Am
from .db import userdata_models as Um

from .types import AppData, USutta


class NodeData(TypedDict):
    label: str
    title: str
    description: str
    table: str
    id: int


GraphNode = Tuple[str, NodeData]

GraphEdge = Tuple[str, str]


def sutta_graph_id(x: USutta) -> str:
    return f"appdata.suttas.{x.id}"


def document_graph_id(x: Um.Document, page_number: int) -> str:
    return f"userdata.documents.{x.id}.{page_number}"


def sutta_to_node(x: USutta) -> GraphNode:
    return (
        sutta_graph_id(x),
        {
            'label': x.sutta_ref,
            'title': x.title,
            'description': '',
            'table': 'appdata.suttas',
            'id': x.id,
        },
    )


def document_to_node(x: Am.Document, page_number: int) -> GraphNode:
    return (
        document_graph_id(x, page_number),
        {
            'label': 'p.' + str(page_number),
            'title': x.title,
            'description': '',
            'table': 'userdata.documents',
            'id': x.id,
        }
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


def sutta_nodes_and_edges(app_data: AppData, sutta: USutta, distance: int = 1):
    links = []

    # TODO: Assuming all links are in userdata, made between
    # 'appdata.suttas' records

    r = app_data.db_session \
        .query(Um.Link.to_id) \
        .filter(Um.Link.from_table == "appdata.suttas") \
        .filter(Um.Link.to_table == "appdata.suttas") \
        .filter(Um.Link.from_id == sutta.id) \
        .all()

    links.extend(r)

    r = app_data.db_session \
        .query(Um.Link.from_id) \
        .filter(Um.Link.from_table == "appdata.suttas") \
        .filter(Um.Link.to_table == "appdata.suttas") \
        .filter(Um.Link.to_id == sutta.id) \
        .all()

    links.extend(r)

    # IDs without the current sutta ID
    ids = filter(lambda x: x != sutta.id, map(lambda x: x[0], links))
    # set() will contain unique items
    sutta_ids = list(set(ids))

    suttas = app_data.db_session \
        .query(Am.Sutta) \
        .filter(Am.Sutta.id.in_(sutta_ids)) \
        .all()

    nodes = list(map(sutta_to_node, suttas))

    def to_edge(x: USutta):
        from_id = sutta_graph_id(sutta)
        to_id = sutta_graph_id(x)
        if to_id < from_id:
            return (to_id, from_id)
        else:
            return (from_id, to_id)

    edges = list(map(to_edge, suttas))

    # Collect links from other nodes

    if distance > 1:
        for i in suttas:
            (n, e) = sutta_nodes_and_edges(app_data=app_data, sutta=i, distance=distance - 1)
            nodes.extend(n)
            edges.extend(e)

    # Append the current sutta as a node

    nodes.append(sutta_to_node(sutta))

    return (unique_nodes(nodes), unique_edges(edges))


def document_page_nodes_and_edges(app_data: AppData, db_doc: Am.Document, page_number: int, distance: int = 1):
    links = app_data.db_session \
        .query(Um.Link.to_id) \
        .filter(Um.Link.from_table == "userdata.documents") \
        .filter(Um.Link.from_page_number == page_number) \
        .filter(Um.Link.to_table == "appdata.suttas") \
        .filter(Um.Link.from_id == db_doc.id) \
        .all()

    ids = map(lambda x: x[0], links)
    # set() will contain unique items
    sutta_ids = list(set(ids))

    suttas = app_data.db_session \
        .query(Am.Sutta) \
        .filter(Am.Sutta.id.in_(sutta_ids)) \
        .all()

    nodes = list(map(sutta_to_node, suttas))

    def to_edge(x: USutta):
        from_id = document_graph_id(db_doc, page_number)
        to_id = sutta_graph_id(x)
        return (to_id, from_id)

    edges = list(map(to_edge, suttas))

    # Collect links from other nodes

    if distance > 1:
        for i in suttas:
            (n, e) = sutta_nodes_and_edges(app_data=app_data, sutta=i, distance=distance - 1)
            nodes.extend(n)
            edges.extend(e)

    # Append the current doument as a node

    nodes.append(document_to_node(db_doc, page_number))

    return (unique_nodes(nodes), unique_edges(edges))


def generate_graph(nodes, edges, selected_indices: List[int], queue_id: str, output_path: Path):
    if len(nodes) == 0:
        return

    G = nx.Graph()
    G.add_nodes_from(nodes)
    G.add_edges_from(edges)

    plot = figure(
        title=None,
        plot_width=700,
        plot_height=400,
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

    network_graph = from_networkx(G, nx.spring_layout, scale=0.8, center=(0, 0), seed=100)

    network_graph.node_renderer.glyph = Circle(size=15, fill_color=Spectral4[0])
    network_graph.node_renderer.selection_glyph = Circle(size=15, fill_color=Spectral4[2])
    network_graph.node_renderer.nonselection_glyph = Circle(size=15, fill_color=Spectral4[1])

    network_graph.edge_renderer.glyph = MultiLine(line_color="black", line_alpha=0.8, line_width=1)
    network_graph.edge_renderer.selection_glyph = MultiLine(line_color=Spectral4[2], line_width=2)

    network_graph.selection_policy = NodesAndLinkedEdges()
    network_graph.inspection_policy = EdgesAndLinkedNodes()

    plot.renderers.append(network_graph)

    x, y = zip(*network_graph.layout_provider.graph_layout.values())
    source = ColumnDataSource({
        'x': x,
        'y': y,
        'label': [nodes[i][1]['label'] for i in range(len(x))],
        'title': [nodes[i][1]['title'] for i in range(len(x))],
        'description': [nodes[i][1]['description'] for i in range(len(x))],
        'table': [nodes[i][1]['table'] for i in range(len(x))],
        'id': [nodes[i][1]['id'] for i in range(len(x))],
    })

    tooltips = [
        ('Title', '@title')
    ]

    cr = plot.circle(x='x', y='y', source=source, size=25,
                     fill_color=None, line_color=None,
                     hover_fill_color=Spectral4[2],
                     hover_alpha=0.8, hover_line_color="white")

    plot.add_tools(HoverTool(tooltips=tooltips, renderers=[cr]))

    labels = LabelSet(
        x='x',
        y='y',
        text='label',
        source=source,
        background_fill_color='white',
        text_font_size='15px',
        background_fill_alpha=.7)

    plot.renderers.append(labels)

    network_graph.node_renderer.data_source.selected.indices = selected_indices

    network_graph.node_renderer.data_source.selected.js_on_change('indices', CustomJS(args=dict(source=source), code="""
window.selected_info = [];

var inds = cb_obj.indices;

var body = document.getElementsByTagName('body')[0];

var desc_div = document.getElementById('selected_desc');
if (desc_div == null || typeof desc_div == 'undefined') {
    desc_div = document.createElement('div');
    desc_div.setAttribute('id', 'selected_desc');
    body.appendChild(desc_div);
}

// Expecting exactly one selected item, selected with the Tap tool.

if (inds.length == 1) {
    var idx = inds[0];
    var data = source.data;

    var table = data['table'][idx];
    var id = data['id'][idx];
    window.selected_info.push({ table: table, id: id });

    var title = data['title'][idx];
    var desc_content = data['description'][idx];
    desc_div.innerHTML = "<h1>" + title + "</h1><div>" + desc_content + "</div>";
}
"""))

    button = Button(label='Open Sutta')

    url = f'http://localhost:8000/queues/{queue_id}'
    js_code = """
if (typeof window.selected_info === 'undefined') {
    window.selected_info = [];
}
if (window.selected_info.length > 0) {
    const params = {
        action: 'show_sutta',
        arg: window.selected_info[0],
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
""" % (url,)

    button.js_on_event(events.ButtonClick, CustomJS(code=js_code, args={}))

    layout = column(button, plot)

    doc: Document = curdoc()
    doc.clear()
    doc.add_root(layout)

    output_file(filename=str(output_path), mode='absolute')
    save(doc)

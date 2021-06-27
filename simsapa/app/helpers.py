from pathlib import Path
import requests

from .db import appdata_models as Am
from .db import userdata_models as Um

from .types import AppData, USutta


def download_file(url: str, folder_path: Path) -> Path:
    file_name = url.split('/')[-1]
    file_path = folder_path.joinpath(file_name)

    with requests.get(url, stream=True) as r:
        r.raise_for_status()
        with open(file_path, 'wb') as f:
            for chunk in r.iter_content(chunk_size=8192):
                f.write(chunk)

    return file_path


def sutta_nodes_and_edges(app_data: AppData, sutta: USutta, distance: int = 1):
    links = []

    # TODO: Assuming all links are in userdata, made between
    # 'appdata.suttas' records

    r = app_data.db_session \
        .query(Um.Link.to_id) \
        .filter(Um.Link.from_table == "appdata.suttas") \
        .filter(Um.Link.from_id == sutta.id) \
        .all()

    links.extend(r)

    r = app_data.db_session \
        .query(Um.Link.from_id) \
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

    def to_node(x: USutta):
        return (
            x.id,
            {
                'uid': x.uid,
                'sutta_ref': x.sutta_ref,
                'title': x.title,
            },
        )

    nodes = list(map(to_node, suttas))

    def to_edge(x: USutta):
        if sutta.id < x.id:
            return (sutta.id, x.id)
        else:
            return (x.id, sutta.id)

    edges = list(map(to_edge, suttas))

    # Collect links from other nodes

    if distance > 1:
        for i in suttas:
            (n, e) = sutta_nodes_and_edges(app_data=app_data, sutta=i, distance=distance - 1)
            nodes.extend(n)
            edges.extend(e)

    # Append the current sutta as a node

    nodes.append(to_node(sutta))

    listed = []
    unique_edges = []
    for i in edges:
        e = str(i[0]) + ',' + str(i[1])
        if e not in listed:
            listed.append(e)
            unique_edges.append(i)

    unique_edges.sort(key=lambda x: f"{x[0]},{x[1]}")

    listed = []
    unique_nodes = []
    for i in nodes:
        e = i[1]['uid']
        if e not in listed:
            listed.append(e)
            unique_nodes.append(i)

    unique_nodes.sort(key=lambda x: x[1]['uid'])

    return (unique_nodes, unique_edges)

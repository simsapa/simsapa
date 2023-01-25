import sys
from pathlib import Path
import re
import json
from typing import Dict, List, Optional, TypedDict
import roman

from sqlalchemy import null, or_, and_
from sqlalchemy.sql import func
from sqlalchemy.orm.session import Session

from pyArango.connection import Connection
from pyArango.database import DBHandle

from simsapa import logger
from simsapa.app.db import appdata_models as Am
from simsapa.app.helpers import bilara_html_post_process, bilara_text_to_html, consistent_nasal_m, html_get_sutta_page_body, compact_rich_text, sutta_range_from_ref, normalize_sutta_ref

import helpers

def get_suttacentral_db() -> DBHandle:
    conn = Connection(
        arangoURL="http://localhost:8529",
        username="root",
        password="test",
    )

    # db = conn.createDatabase(name="suttacentral")
    db: DBHandle = conn["suttacentral"]

    return db

def html_text_uid(x) -> str:
    '''dn1/en/bodhi'''
    return f"{x['uid']}/{x['lang']}/{x['author_uid']}"

def bilara_text_uid(x) -> str:
    '''dn1/pli/ms'''

    a = x['muids'].copy()
    for k in ['translation', 'root', 'reference', 'variant', 'comment', 'html', x['lang']]:
        if k in a:
            a.pop(a.index(k))

    if len(a) == 1:
        author = a[0]
    elif len(a) == 0 and '/pli/ms/' in x['file_path']:
        author = 'ms'
    elif len(a) == 0 and '/pli/vri/' in x['file_path']:
        author = 'vri'
    else:
        """
        sc_bilara_texts/sn1.54_translation-pt-laera-quaresma,
        muids: ['translation', 'pt', 'laera', 'quaresma'],
        /opt/sc/sc-flask/sc-data/sc_bilara_data/translation/pt/laera-quaresma/sutta/sn/sn1/sn1.54_translation-pt-laera-quaresma.json
        """

        # There may multiple authors of a translation.
        # At this point 'translation' and lang 'pt' should have been removed, use the remainder as author label.

        author = "-".join(a)

    return f"{x['uid']}/{x['lang']}/{author}"

def html_text_to_sutta(x, title: str, _: Optional[str]) -> Am.Sutta:
    # html pages can be complete docs, <!DOCTYPE html><html>...
    page = x['text']

    body = html_get_sutta_page_body(page)
    body = bilara_html_post_process(body)
    content_html = '<div class="suttacentral html-text">' + consistent_nasal_m(body) + '</div>'
    content_plain = compact_rich_text(content_html)

    uid = html_text_uid(x)
    source_uid = uid.split('/')[-1]

    sutta_range = sutta_range_from_ref(x['uid'])
    if not sutta_range:
        logger.error(f"Can't determine sutta range: {x['uid']}")
        sys.exit(1)

    return Am.Sutta(
        source_uid = source_uid,
        source_info = x['_id'],
        # The All-embracing Net of Views
        title = consistent_nasal_m(title),
        # dn1/en/bodhi
        uid = uid,
        # SN 12.23
        sutta_ref = helpers.uid_to_ref(x['uid']),
        sutta_range_group = sutta_range['group'],
        sutta_range_start = sutta_range['start'],
        sutta_range_end = sutta_range['end'],
        # en
        language = x['lang'],
        content_html = content_html,
        content_plain = content_plain,
        created_at = func.now(),
    )

def bilara_text_to_sutta(x, title: str, tmpl_json: Optional[str]) -> Am.Sutta:
    content = x['text']

    if tmpl_json is None:
        logger.warn(f"No template: {x['uid']} {title} {x['file_path']}")

        a = json.loads(content)
        page = "\n\n".join(a.values())

        content_html = null()
        content_plain = page

    else:
        content_html = bilara_text_to_html(content, tmpl_json)
        content_plain = compact_rich_text(content_html)

    uid = bilara_text_uid(x)
    source_uid = uid.split('/')[-1]

    sutta_range = sutta_range_from_ref(x['uid'])
    if not sutta_range:
        logger.error(f"Can't determine sutta range: {x['uid']}")
        sys.exit(1)

    return Am.Sutta(
        source_uid = source_uid,
        source_info = x['_id'],
        title = consistent_nasal_m(title),
        # mn123/pli/ms
        uid = uid,
        # SN 12.23
        sutta_ref = helpers.uid_to_ref(x['uid']),
        sutta_range_group = sutta_range['group'],
        sutta_range_start = sutta_range['start'],
        sutta_range_end = sutta_range['end'],
        # pli
        language = x['lang'],
        content_plain = content_plain,
        # Not saving the html to reduce DB size. content_plain is used for
        # indexing and search. Re-generate HTML from JSON is needed.
        content_html = null(),
        content_json = consistent_nasal_m(content),
        content_json_tmpl = tmpl_json,
        created_at = func.now(),
    )

def get_titles(db: DBHandle, language = 'en') -> dict[str, str]:
    if language == 'pli':
        get_names_aql = "LET docs = (FOR x IN names FILTER x.is_root == true RETURN x) RETURN docs"
        q = db.AQLQuery(get_names_aql)

    else:
        get_names_aql = "LET docs = (FOR x IN names FILTER x.lang == @language RETURN x) RETURN docs"
        q = db.AQLQuery(
            get_names_aql,
            bindVars={'language': language}
        )

    titles: dict[str, str] = {}
    for x in q.result[0]:
        uid = x['uid']
        if uid in titles.keys():
            logger.warn(f"title for {uid} exists")
        else:
            titles[uid] = x['name']

    return titles

def _res_is_ignored(r: Dict[str, str]) -> bool:
    # ignore site pages and some collections
    if ('file_path' in r.keys() and '/site/' in r['file_path']) \
        or ('file_path' in r.keys() and '/xplayground/' in r['file_path']) \
        or ('file_path' in r.keys() and '/sutta/sa/' in r['file_path']) \
        or ('file_path' in r.keys() and '/sutta/ma/' in r['file_path']) \
        or ('file_path' in r.keys() and '-blurbs_' in r['file_path']) \
        or ('file_path' in r.keys() and '-name_translation' in r['file_path']):

        return True

    # ignore comments
    if 'muids' in r.keys() and 'comment' in r['muids']:

        return True

    # html bilara wrapper JSON already collected, skip
    if ('file_path' in r.keys() and 'sc_bilara_data/html' in r['file_path']) \
        and ('muids' in r.keys() and 'html' in r['muids']):

        return True

    return False

def _uid_is_ignored(uid: str) -> bool:
    # ignore 'none' author
    if uid.endswith('/none'):
        logger.warn(f"Author (source) is None: {uid}")
        return True

    # ignore blurbs
    if uid.endswith('-blurbs'):
        return True

    # ignore titles list json
    if uid.endswith('-name'):
        return True

    if uid.endswith('/than') or uid.endswith('/thanissaro'):
        # We'll use Aj Thanissaro's translations from dhammatalks.org
        return True

    return False

def get_suttas(db: DBHandle, sc_data_dir: Path, language = 'en', limit: Optional[int] = None) -> dict[str, Am.Sutta]:
    logger.info("=== get_suttas() ===")

    # NOTE: In suttacentral records an uid is not a unique record, it is the
    # sutta reference (dn12, an4.10). Some texts exist in two formats, both in
    # html_text and sc_bilara_texts.
    #
    # A record is unique to uid + language + author + format.
    #
    # Bilara format is newer and edited, we'll prefer that source to html_text.

    suttas: dict[str, Am.Sutta] = {}
    suttas_html_tmpl_json: dict[str, str] = {}

    get_html_text_aql = '''
    LET docs = (
        FOR x IN html_text
            FILTER x.lang == @language
            RETURN x
    )
    RETURN docs
    '''

    get_bilara_text_aql = '''
    LET docs = (
        FOR x IN sc_bilara_texts
            FILTER x.lang == @language
            RETURN x
    )
    RETURN docs
    '''

    get_bilara_text_templates_aql = '''
    LET docs = (
        FOR x IN sc_bilara_texts
            FILTER x.lang == 'pli' && x._key LIKE '%_html'
            RETURN x
    )
    RETURN docs
    '''

    # collect templates
    q = db.AQLQuery(get_bilara_text_templates_aql)

    tmpl_results = q.result[0]

    for r in tmpl_results:
        # html bilara wrapper JSON
        if ('file_path' in r.keys() and 'sc_bilara_data/html' in r['file_path']) \
            and ('muids' in r.keys() and 'html' in r['muids']):

            convert_paths_to_content(r, sc_data_dir)
            text_uid_ref = r['uid']
            if text_uid_ref not in suttas_html_tmpl_json.keys():
                suttas_html_tmpl_json[text_uid_ref] = r['text']

    titles = get_titles(db, language)

    def _html_title(x, _):
        return x['name']

    def _bilara_title(x, titles):
        uid = x['uid']
        if uid in titles:
            return titles[uid]
        else:
            return ''

    queries = (
        (get_html_text_aql, html_text_uid, html_text_to_sutta, _html_title),
        (get_bilara_text_aql, bilara_text_uid, bilara_text_to_sutta, _bilara_title),
    )

    total_results = 0
    ignored = 0
    known_dup = 0
    unknown_dup = 0

    for query, f_uid, f_to_sutta, f_title in queries:
        q = db.AQLQuery(
            query,
            bindVars={'language': language}
        )

        if limit:
            n = limit if len(q.result[0]) >= limit else len(q.result[0])
            text_results = q.result[0][0:n]
        else:
            text_results = q.result[0]

        total_results += len(text_results)

        for r in text_results:
            if _res_is_ignored(r):
                ignored += 1
                continue

            convert_paths_to_content(r, sc_data_dir)
            uid = f_uid(r)
            title = f_title(r, titles)
            tmpl_json = suttas_html_tmpl_json.get(r['uid'], None)

            if _uid_is_ignored(uid):
                ignored += 1
                continue

            if uid not in suttas.keys():
                suttas[uid] = f_to_sutta(r, title, tmpl_json)

            elif 'muids' in r.keys() and ('reference' in r['muids'] or 'variant' in r['muids']):
                # keeping only the 'root' version
                known_dup += 1
                continue

            elif 'muids' in r.keys() and 'root' in r['muids']:
                # keeping only the 'root' version
                known_dup += 1
                suttas[uid] = f_to_sutta(r, title, tmpl_json)

            elif r['_id'].startswith('sc_bilara_texts/') and suttas[uid].source_info.startswith('html_text/'):
                # keeping the Bilara version
                known_dup += 1
                suttas[uid] = f_to_sutta(r, title, tmpl_json)

            else:
                unknown_dup += 1
                logger.warn(f"Unknown duplicate uid: {uid}")
                logger.warn(r['_id'])
                logger.warn(r['muids'])
                logger.warn(suttas[uid].source_info)

    n = total_results - ignored - known_dup - unknown_dup
    if len(suttas) != n:
        logger.warn(f"Count is not adding up: {len(suttas)} != {n}, {total_results} - {ignored} - {known_dup} - {unknown_dup}.")

    # clear source_info, where we temp stored x['_id']
    for k, v in suttas.items():
        v.source_info = null() # type: ignore
        suttas[k] = v

    return suttas

def convert_paths_to_content(doc, sc_data_dir: Path):
    conversions = (
        ('file_path', 'text', lambda f: f.read()),
        ('markup_path', 'markup', lambda f: f.read()),
        ('strings_path', 'strings', json.load),
    )

    for from_prop, to_prop, load_func in conversions:
        if (to_prop not in doc) and (from_prop in doc):
            file_path = doc[from_prop]
            if file_path is None:
                doc[to_prop] = None
            else:
                file_path = file_path.replace('/opt/sc/sc-flask/sc-data', f"{sc_data_dir}")
                p = Path(file_path)

                if not p.exists():
                    logger.error(f"File not found: {p}")
                    doc[to_prop] = None
                else:
                    with open(p) as f:
                        doc[to_prop] = load_func(f)


def add_sutta_variants(appdata_db: Session, sc_db: DBHandle, sc_data_dir: Path, language: str, limit: Optional[int] = None):
    logger.info("=== add_sutta_variants() ===")

    get_bilara_text_variant_aql = '''
    LET docs = (
        FOR x IN sc_bilara_texts
            FILTER x.lang == @language && POSITION(x.muids, 'variant')
            RETURN x
    )
    RETURN docs
    '''

    q = sc_db.AQLQuery(
        get_bilara_text_variant_aql,
        bindVars={'language': language}
    )

    if limit:
        n = limit if len(q.result[0]) >= limit else len(q.result[0])
        text_results = q.result[0][0:n]
    else:
        text_results = q.result[0]

    results = []

    for r in text_results:
        convert_paths_to_content(r, sc_data_dir)

        if _res_is_ignored(r):
            continue

        sutta_uid = bilara_text_uid(r)
        source_uid = sutta_uid.split('/')[-1]

        if _uid_is_ignored(sutta_uid):
            continue

        res = appdata_db \
            .query(Am.Sutta.id) \
            .filter(Am.Sutta.uid == sutta_uid) \
            .first()

        if res is None:
            logger.error(f"add_sutta_variants() Can't find sutta uid: {sutta_uid}")
            continue

        sutta_id = int(res[0])

        item = Am.SuttaVariant(
            sutta_id = sutta_id,
            sutta_uid = sutta_uid,
            language = language,
            source_uid = source_uid,
            content_json = consistent_nasal_m(r['text']),
        )

        results.append(item)

    logger.info(f"Adding {len(results)} sutta variants ...")

    try:
        for i in results:
            appdata_db.add(i)
        appdata_db.commit()
    except Exception as e:
        logger.error(e)
        sys.exit(1)

def add_sutta_comments(appdata_db: Session, sc_db: DBHandle, sc_data_dir: Path, language: str, limit: Optional[int] = None):
    logger.info("=== add_sutta_comments() ===")

    get_bilara_text_comment_aql = '''
    LET docs = (
        FOR x IN sc_bilara_texts
            FILTER x.lang == @language && POSITION(x.muids, 'comment')
            RETURN x
    )
    RETURN docs
    '''

    q = sc_db.AQLQuery(
        get_bilara_text_comment_aql,
        bindVars={'language': language}
    )

    if limit:
        n = limit if len(q.result[0]) >= limit else len(q.result[0])
        text_results = q.result[0][0:n]
    else:
        text_results = q.result[0]

    results = []

    for r in text_results:
        convert_paths_to_content(r, sc_data_dir)

        if _res_is_ignored(r):
            continue

        sutta_uid = bilara_text_uid(r)
        source_uid = sutta_uid.split('/')[-1]

        if _uid_is_ignored(sutta_uid):
            continue

        res = appdata_db \
            .query(Am.Sutta.id) \
            .filter(Am.Sutta.uid == sutta_uid) \
            .first()

        if res is None:
            logger.error(f"add_sutta_comments() Can't find sutta uid: {sutta_uid}")
            continue

        sutta_id = int(res[0])

        item = Am.SuttaComment(
            sutta_id = sutta_id,
            sutta_uid = sutta_uid,
            language = language,
            source_uid = source_uid,
            content_json = consistent_nasal_m(r['text']),
        )

        results.append(item)

    logger.info(f"Adding {len(results)} sutta comments ...")

    try:
        for i in results:
            appdata_db.add(i)
        appdata_db.commit()
    except Exception as e:
        logger.error(e)
        sys.exit(1)

def populate_suttas_from_suttacentral(appdata_db: Session, sc_db: DBHandle, sc_data_dir: Path, lang: str, limit: Optional[int] = None):
        suttas = get_suttas(sc_db, sc_data_dir, lang, limit)
        if len(suttas) == 0:
            # NOTE: This exact output string is checked for status in bootstrap_db.sh
            logger.info(f"0 suttas for {lang}, exiting.")
            sys.exit(1)

        logger.info(f"Adding {lang}, count {len(suttas)} ...")

        try:
            # TODO: bulk insert errors out
            # NOTE: this is slow but works
            for i in suttas.values():
                appdata_db.add(i)
            appdata_db.commit()
        except Exception as e:
            logger.error(e)
            sys.exit(1)

        add_sutta_variants(appdata_db, sc_db, sc_data_dir, lang, limit)

        add_sutta_comments(appdata_db, sc_db, sc_data_dir, lang, limit)

        logger.info(f"DONE: {lang}")

class TextInfo(TypedDict):
    uid: str
    acronym: str
    alt_acronym: Optional[str]
    volpage: Optional[str]
    alt_volpage: Optional[str]
    alt_name: Optional[str]
    biblio_uid: Optional[str]

def _text_to_multi_ref(collection: str, ref_text: str) -> Optional[Am.MultiRef]:
    ref_text = ref_text.lower()

    # Vinaya, Sutta Vibhanga
    if collection == 'vb':
        # ref includes the volume or not?
        if re.search(r' +[0-9ivx]+[\. ][0-9]+', ref_text):
            collection = 'vin'
        else:
            collection = 'vin i'

    # pli-tv-bi-pm-{pj,ss,ay,np,pc,pd,sk,as}
    # pli-tv-bu-pm-{pj,ss,ay,np,pc,pd,sk,as}
    #
    # volpage and alt_volpage is empty in SuttaCentral DB.

    # Bhikkhu Vibhanga
    if collection.startswith('pli-tv-bu-vb'):
        if re.search(r' +[0-9ivx]+[\. ][0-9]+', ref_text):
            collection = 'vin'
        else:
            collection = 'vin iii'

    # pli-tv-bi-vb-pj5
    # PTS 4.211,PTS 4.212,PTS 4.213, PTS 4.214, PTS 4.215

    # Bhikkhuni Vibhanga
    if collection.startswith('pli-tv-bi-vb'):
        if re.search(r' +[0-9ivx]+[\. ][0-9]+', ref_text):
            collection = 'vin'
        else:
            collection = 'vin iv'

    # Vinaya, Parivāra
    if collection.startswith('pli-tv-pvr'):
        # ref includes the volume or not?
        if re.search(r' +[0-9ivx]+[\. ][0-9]+', ref_text):
            collection = 'vin'
        else:
            collection = 'vin v'

    # PTS (1st ed) SN i 36
    # PTS (2nd ed) SN i 79
    if '(1st ed)' in ref_text:
        ref_text = ref_text.replace('(1st ed)', '')

        item = Am.MultiRef(
            collection = collection,
            ref_type = "pts",
            ref = normalize_sutta_ref(ref_text),
            edition = "1st ed. Feer (1884)",
        )

        return item

    elif '(2nd ed)' in ref_text:
        ref_text = ref_text.replace('(2nd ed)', '')

        item = Am.MultiRef(
            collection = collection,
            ref_type = "pts",
            ref = normalize_sutta_ref(ref_text),
            edition = "2nd ed. Somaratne (1998)",
        )

        return item

    refs = []
    # Ref may contain a list of references, separated by commas.
    # MN 13
    # PTS 1.84, PTS 1.85, PTS 1.86, PTS 1.87, PTS 1.88, PTS 1.89, PTS 1.90
    # PTS 3.123 = DN iii 123
    matches = re.finditer(r'(?P<pts>pts *)?(?P<vol>\d+)\.(?P<page>\d+)', ref_text)
    for m in matches:
        vol = roman.toRoman(int(m.group('vol'))).lower()
        s = f"{collection} {vol} {m.group('page')}"
        s = re.sub(r'  +', ' ', s)
        refs.append(s)

    if len(refs) > 0:
        item = Am.MultiRef(
            collection = collection,
            ref_type = "pts",
            ref = ", ".join(refs),
        )

        return item

    # Ref may contain roman numerals.
    # DN iii 123
    # PTS iii 123
    matches = re.finditer(r'(?P<pts>pts *)?(?P<vol>[ivx]+)[\. ](?P<page>\d+)', ref_text)
    for m in matches:
        vol = m.group('vol').lower()
        s = f"{collection} {vol} {m.group('page')}"
        refs.append(s)

    if len(refs) > 0:
        item = Am.MultiRef(
            collection = collection,
            ref_type = "pts",
            ref = ", ".join(refs),
        )

        return item

    return None

def add_sc_multi_refs(appdata_db: Session, sc_db: DBHandle):
    logger.info("=== add_sc_multi_refs() ===")

    """
    uid: sn1.51
    acronym: SN 1.51
    alt_acronym: SN 51
    volpage: PTS (1st ed) SN i 36
    alt_volpage: PTS (2nd ed) SN i 79
    alt_name: null
    biblio_uid: null
    """

    get_text_extra_info_aql = "LET docs = (FOR x IN text_extra_info RETURN x) RETURN docs"

    q = sc_db.AQLQuery(get_text_extra_info_aql)
    text_extra_info_by_uid: Dict[str, TextInfo] = dict()
    for r in q.result[0]:
        # Ignore refs not expected to be linked.
        ignore_prefixes = ["avs", "da", "divy", "dk", "ea", "gdhp", "gf", "kf",
                           "lal", "lzh-", "ma", "mkv", "pf", "pgd-", "sa", "sbh",
                           "sf", "sht-", "spp", "up", "uv", "xct-", "uf",
                           "other-", "pancavaggiyakatha", "kosambakavivadakatha", "bodhirajakumaraka",
                           "pli-tv-bi-pm-", "pli-tv-bu-pm-"]

        # pli-tv-bi-pm-{pj,ss,ay,np,pc,pd,sk,as}
        # pli-tv-bu-pm-{pj,ss,ay,np,pc,pd,sk,as}
        #
        # volpage and alt_volpage is empty in SuttaCentral DB.

        do_continue = False

        for i in ignore_prefixes:
            if r['uid'].startswith(i):
                do_continue = True
                continue

            if re.search(r'^(t|d|g)[0-9]+', r['uid']) is not None:
                do_continue = True
                continue

        if do_continue:
            continue

        item = TextInfo(
            uid = r['uid'],
            acronym = r['acronym'],
            alt_acronym = r['alt_acronym'],
            volpage = r['volpage'],
            alt_volpage = r['alt_volpage'],
            alt_name = r['alt_name'],
            biblio_uid = r['biblio_uid'],
        )

        text_extra_info_by_uid[r['uid']] = item


    for part_uid in text_extra_info_by_uid.keys():
        sutta_range = sutta_range_from_ref(part_uid)
        if not sutta_range:
            logger.error(f"Can't determine sutta range: {part_uid}")
            continue

        suttas: List[Am.Sutta] = []

        if sutta_range['start'] is None:
            suttas = appdata_db.query(Am.Sutta) \
                               .filter(Am.Sutta.uid.like(f"{part_uid}/%")) \
                               .all()

        else:
            # Find a sutta which exactly matches
            # OR includes this range
            # sn22.1-20 includes sn22.11-15
            suttas = appdata_db \
                .query(Am.Sutta) \
                .filter(or_(
                    Am.Sutta.uid.like(f"{part_uid}/%"),
                    and_(Am.Sutta.sutta_range_group == sutta_range['group'],
                         Am.Sutta.sutta_range_start <= sutta_range['start'],
                         Am.Sutta.sutta_range_end >= sutta_range['end']),
                )) \
                .all()

            # Find a sutta which matches the start of the range
            # sn22.39 is at the start of sn22.39-42
            if len(suttas) == 0:
                suttas = appdata_db \
                    .query(Am.Sutta) \
                    .filter(
                        and_(Am.Sutta.sutta_range_group == sutta_range['group'],
                             Am.Sutta.sutta_range_start == sutta_range['start'])) \
                    .all()

        if len(suttas) == 0:
            logger.warn(f"No sutta for part_uid {part_uid}, sutta_range {sutta_range}")
            continue

        ref = text_extra_info_by_uid[part_uid]

        multi_refs: List[Am.MultiRef] = []

        if ref['volpage'] is None and ref['alt_volpage'] is None:
            continue

        if 'PTS' not in str(ref['volpage']) and 'PTS' not in str(ref['alt_volpage']):
            continue

        collection = re.sub(r'^([a-z]+)\d.*', r'\1', part_uid)

        if ref['volpage']:
            item = _text_to_multi_ref(collection, ref['volpage'])
            if item:
                multi_refs.append(item)

        if ref['alt_volpage']:
            item = _text_to_multi_ref(collection, ref['alt_volpage'])
            if item:
                multi_refs.append(item)

        for i in multi_refs:
            appdata_db.add(i)
            appdata_db.commit()

        for sutta in suttas:
            sutta.multi_refs = multi_refs # type: ignore
            appdata_db.commit()

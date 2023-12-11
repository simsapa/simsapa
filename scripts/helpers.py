import sys
import re
from typing import Optional
from pathlib import Path
import roman

from sqlalchemy import create_engine
from sqlalchemy.sql import text
from sqlalchemy.orm import sessionmaker
from sqlalchemy.orm.session import Session

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db_helpers import find_or_create_db
from simsapa import DbSchemaName, logger
from simsapa.app.helpers import normalize_sutta_ref
from simsapa.app.types import UMultiRef

def get_db_session(db_path: Path) -> Session:
    try:
        # Create an in-memory database
        engine = create_engine(f"sqlite+pysqlite:///{db_path}", echo=False)

        # db_conn = engine.connect()

        Session = sessionmaker(engine)
        Session.configure(bind=engine)
        db_session = Session()
    except Exception as e:
        logger.error(f"Can't connect to database: {e}")
        sys.exit(1)

    return db_session

def get_simsapa_db(db_path: Path, schema: DbSchemaName, remove_if_exists: bool) -> Session:
    # remove previously generated db
    if remove_if_exists and db_path.exists():
        db_path.unlink()

    find_or_create_db(db_path, schema)

    try:
        # Create an in-memory database
        engine = create_engine("sqlite+pysqlite://", echo=False)

        db_conn = engine.connect()

        if schema == DbSchemaName.AppData:
            db_conn.execute(text(f"ATTACH DATABASE '{db_path}' AS appdata;"))
        elif schema == DbSchemaName.UserData:
            db_conn.execute(text(f"ATTACH DATABASE '{db_path}' AS userdata;"))
        elif schema == DbSchemaName.Dpd:
            db_conn.execute(text(f"ATTACH DATABASE '{db_path}' AS dpd;"))
        else:
            raise Exception(f"Unknown schema_name: {schema}")

        Session = sessionmaker(engine)
        Session.configure(bind=engine)
        db_session = Session()
    except Exception as e:
        logger.error(f"Can't connect to database: {e}")
        sys.exit(1)

    return db_session

def uid_to_ref(uid: str) -> str:
    '''sn12.23 to SN 12.23'''

    # Add a space after the letters, i.e. the collection abbrev
    uid = re.sub(r'^([a-z]+)([0-9])', r'\1 \2', uid)

    # handle all-upcase collections
    subs = [('dn ', 'DN '),
            ('mn ', 'MN '),
            ('sn ', 'SN '),
            ('an ', 'AN ')]
    for sub_from, sub_to in subs:
        uid = uid.replace(sub_from, sub_to)

    # titlecase the rest, upcase the first letter
    uid = uid[0].upper() + uid[1:]

    return uid

def uid_to_nikaya(uid: str) -> str:
    '''sn12.23 to sn'''
    nikaya = re.sub(r'^([a-z]+).*', r'\1', uid)
    return nikaya

def text_to_multi_ref(collection: str, ref_text: str, schema: DbSchemaName) -> Optional[UMultiRef]:
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

        if schema == DbSchemaName.AppData:
            item = Am.MultiRef(
                collection = collection,
                ref_type = "pts",
                ref = normalize_sutta_ref(ref_text),
                edition = "1st ed. Feer (1884)",
            )

        elif schema == DbSchemaName.UserData:
            item = Um.MultiRef(
                collection = collection,
                ref_type = "pts",
                ref = normalize_sutta_ref(ref_text),
                edition = "1st ed. Feer (1884)",
            )
        else:
            raise Exception("Only appdata and userdata schema are allowed.")

        return item

    elif '(2nd ed)' in ref_text:
        ref_text = ref_text.replace('(2nd ed)', '')

        if schema == DbSchemaName.AppData:
            item = Am.MultiRef(
                collection = collection,
                ref_type = "pts",
                ref = normalize_sutta_ref(ref_text),
                edition = "2nd ed. Somaratne (1998)",
            )

        elif schema == DbSchemaName.UserData:
            item = Um.MultiRef(
                collection = collection,
                ref_type = "pts",
                ref = normalize_sutta_ref(ref_text),
                edition = "2nd ed. Somaratne (1998)",
            )

        else:
            raise Exception("Only appdata and userdata schema are allowed.")

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
        if schema == DbSchemaName.AppData:
            item = Am.MultiRef(
                collection = collection,
                ref_type = "pts",
                ref = ", ".join(refs),
            )

        elif schema == DbSchemaName.UserData:
            item = Um.MultiRef(
                collection = collection,
                ref_type = "pts",
                ref = ", ".join(refs),
            )

        else:
            raise Exception("Only appdata and userdata schema are allowed.")

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
        if schema == DbSchemaName.AppData:
            item = Am.MultiRef(
                collection = collection,
                ref_type = "pts",
                ref = ", ".join(refs),
            )

        elif schema == DbSchemaName.UserData:
            item = Um.MultiRef(
                collection = collection,
                ref_type = "pts",
                ref = ", ".join(refs),
            )

        else:
            raise Exception("Only appdata and userdata schema are allowed.")

        return item

    return None

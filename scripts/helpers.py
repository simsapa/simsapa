import sys
import re
from typing import Optional
from pathlib import Path

from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker
from sqlalchemy.orm.session import Session

from simsapa.app.db_helpers import find_or_create_db
from simsapa import DbSchemaName, logger
from simsapa.app.lookup import DHP_CHAPTERS_TO_RANGE


def get_appdata_db(db_path: Path, remove_if_exists: bool) -> Session:
    # remove previously generated db
    if remove_if_exists and db_path.exists():
        db_path.unlink()

    find_or_create_db(db_path, DbSchemaName.AppData.value)

    try:
        # Create an in-memory database
        engine = create_engine("sqlite+pysqlite://", echo=False)

        db_conn = engine.connect()

        # Attach appdata
        db_conn.execute(f"ATTACH DATABASE '{db_path}' AS appdata;")

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


def dhp_chapter_ref_for_verse_num(num: int) -> Optional[str]:
    for v in DHP_CHAPTERS_TO_RANGE.values():
        if num >= v[0] and num <= v[1]:
            return f"dhp{v[0]}-{v[1]}"

    return None

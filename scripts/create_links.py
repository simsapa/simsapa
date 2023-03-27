#!/usr/bin/env python3

import os
import re
from typing import List

from sqlalchemy import create_engine
from sqlalchemy.sql import text
from sqlalchemy.orm import sessionmaker
from sqlalchemy.orm.session import Session

from simsapa.app.db import appdata_models as Am
from simsapa import APP_DB_PATH, USER_DB_PATH, logger
from simsapa.app.types import QueryType


def get_links(appdata_db: Session) -> List[Am.Link]:
    links: List[Am.Link] = []

    suttas = appdata_db.query(Am.Sutta) \
        .filter(Am.Sutta.content_html.like(f"%ssp://{QueryType.suttas.value}/%")) \
        .all()

    # capture the uid part
    re_link = re.compile(f'ssp://{QueryType.suttas.value}/([^\'"#\\?]+)')

    for i in suttas:

        m = re.findall(re_link, i.content_html)
        if len(m) == 0:
            logger.error(f"Can't match uids")
            continue

        for uid in m:

            # uid might be just sutta ref, uda1.1
            if '/' not in uid and '<div class="dhammatalks_org">' in i.content_html:
                uid += '/en/thanissaro'

            target = appdata_db.query(Am.Sutta) \
                .filter(Am.Sutta.uid == uid) \
                .first()

            if target is None:
                logger.error(f"Can't find uid: {uid}")
                continue

            link = Am.Link(
                from_table = 'appdata.suttas',
                from_id = i.id,
                to_table = 'appdata.suttas',
                to_id = target.id,
            )
            links.append(link)

    return links

def populate_links(appdata_db: Session):
    logger.info("=== populate_links() ===")

    links = get_links(appdata_db)

    logger.info(f"Adding links, count: {len(links)}")

    try:
        for i in links:
            appdata_db.add(i)
        appdata_db.commit()
    except Exception as e:
        logger.error(e)
        exit(1)

def _connect_to_db() -> Session:
    app_db_path = APP_DB_PATH
    user_db_path = USER_DB_PATH

    if not os.path.isfile(app_db_path):
        logger.error(f"Database file doesn't exist: {app_db_path}")
        exit(1)

    if not os.path.isfile(user_db_path):
        logger.error(f"Database file doesn't exist: {user_db_path}")
        exit(1)

    try:
        # Create an in-memory database
        db_eng = create_engine("sqlite+pysqlite://", echo=False)

        db_conn = db_eng.connect()

        # Attach appdata and userdata
        db_conn.execute(text(f"ATTACH DATABASE '{app_db_path}' AS appdata;"))
        db_conn.execute(text(f"ATTACH DATABASE '{user_db_path}' AS userdata;"))

        Session = sessionmaker(db_eng)
        Session.configure(bind=db_eng)
        db_session = Session()

    except Exception as e:
        logger.error(f"Can't connect to database: {e}")
        exit(1)

    return db_session

def main():
    logger.info(f"Creating links")

    appdata_db = _connect_to_db()
    links = get_links(appdata_db)

    logger.info(f"Adding links, count: {len(links)}")

    try:
        for i in links:
            appdata_db.add(i)
        appdata_db.commit()
    except Exception as e:
        logger.error(e)
        exit(1)

if __name__ == "__main__":
    main()

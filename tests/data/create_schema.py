#!/usr/bin/env python3

import sys

from sqlalchemy import create_engine
from sqlalchemy.engine import Engine
from sqlalchemy_utils import create_database
from alembic import command
from alembic.config import Config

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

from simsapa import ALEMBIC_INI, ALEMBIC_DIR


def create_schema(schema: str = 'userdata',
                  db_path: str = 'userdata.sqlite3'):

    # Create an in-memory database
    engine = create_engine("sqlite+pysqlite://", echo=False)

    db_url = f"sqlite+pysqlite:///{db_path}"
    alembic_cfg = Config(f"{ALEMBIC_INI}")
    alembic_cfg.set_main_option('script_location', f"{ALEMBIC_DIR}")
    alembic_cfg.set_main_option('sqlalchemy.url', db_url)

    if isinstance(engine, Engine):
        db_conn = engine.connect()
        create_database(db_url)
        db_conn.execute(f"ATTACH DATABASE '{db_path}' AS {schema};")

        if schema == 'userdata':
            Um.metadata.create_all(bind=engine)
        elif schema == 'appdata':
            Am.metadata.create_all(bind=engine)
        else:
            print(f"Invalid schema name: {schema}")
            exit(1)

        command.stamp(alembic_cfg, "head")

def main():
    if len(sys.argv) < 3:
        print("Usage: %s <schema-name> <db-path>" % sys.argv[0], file=sys.stderr)
        exit(1)

    create_schema(sys.argv[1], sys.argv[2])

if __name__ == '__main__':
    main()

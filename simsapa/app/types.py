import os.path
# from enum import Enum
from sqlalchemy import create_engine  # type: ignore
from sqlalchemy.orm import sessionmaker  # type: ignore
# from typing import Any, Dict, Optional, Tuple


class AppData:
    def __init__(self, db_path: str):
        db_conn = None
        if os.path.isfile(db_path):
            try:
                engine = create_engine(f"sqlite+pysqlite:///{db_path}", echo=False, future=True)
                db_conn = engine.connect()
                Session = sessionmaker(engine)
                Session.configure(bind=engine)
                db_session = Session()
            except Exception as e:
                print(e)
                exit(1)
        else:
            print(f"ERROR: Can't connect to database: {db_path}")
            exit(1)

        self.db_conn = db_conn
        self.db_session = db_session


class DictWord:
    def __init__(self, word: str):
        self.word = word
        self.definition_md = ''


class Sutta:
    def __init__(self, uid: str, title: str, content_html: str):
        self.uid = uid
        self.title = title
        self.content_html = content_html

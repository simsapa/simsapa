import re
from typing import List

from PyQt6.QtCore import QObject, QRunnable, pyqtSignal, pyqtSlot

from simsapa import logger
from simsapa.app.types import CompletionCacheResult
from simsapa.app.db_session import get_db_engine_connection_session

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

class CompletionCacheWorkerSignals(QObject):
    finished = pyqtSignal(dict)

class CompletionCacheWorker(QRunnable):
    signals: CompletionCacheWorkerSignals

    def __init__(self):
        super().__init__()
        self.signals = CompletionCacheWorkerSignals()

    @pyqtSlot()
    def run(self):
        logger.profile("CompletionCacheWorker::run()")
        try:
            db_eng, db_conn, db_session = get_db_engine_connection_session()

            res = []
            r = db_session.query(Am.Sutta.title).all()
            res.extend(r)

            r = db_session.query(Um.Sutta.title).all()
            res.extend(r)

            a: List[str] = list(map(lambda x: x[0] or 'none', res))
            b = list(map(lambda x: re.sub(r' *\d+$', '', x.lower()), a))
            b.sort()
            titles = list(set(b))

            res = []
            r = db_session.query(Am.DictWord.word).all()
            res.extend(r)

            r = db_session.query(Um.DictWord.word).all()
            res.extend(r)

            a: List[str] = list(map(lambda x: x[0] or 'none', res))
            b = list(map(lambda x: re.sub(r' *\d+$', '', x.lower()), a))
            b.sort()
            words = list(set(b))

            db_conn.close()
            db_session.close()
            db_eng.dispose()

            self.signals.finished.emit(CompletionCacheResult(
                sutta_titles=titles,
                dict_words=words,
            ))

        except Exception as e:
            logger.error(e)

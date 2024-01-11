from PyQt6.QtCore import QObject, QRunnable, pyqtSignal, pyqtSlot

from simsapa import logger
from simsapa.app.db_session import get_db_engine_connection_session

from simsapa.layouts.gui_types import CompletionCacheResult
from simsapa.app.completion_lists import get_sutta_titles_completion_list, get_dict_words_completion_list

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

            titles = get_sutta_titles_completion_list(db_session)
            words = get_dict_words_completion_list(db_session)

            db_conn.close()
            db_session.close()
            db_eng.dispose()

            self.signals.finished.emit(CompletionCacheResult(
                sutta_titles=titles,
                dict_words=words,
            ))

        except Exception as e:
            logger.error(e)

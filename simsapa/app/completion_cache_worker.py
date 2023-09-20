from PyQt6.QtCore import QObject, QRunnable, pyqtSignal, pyqtSlot

from simsapa import logger

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
            titles = get_sutta_titles_completion_list()
            words = get_dict_words_completion_list()

            self.signals.finished.emit(CompletionCacheResult(
                sutta_titles=titles,
                dict_words=words,
            ))

        except Exception as e:
            logger.error(e)

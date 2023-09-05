from datetime import datetime
from typing import Callable

from PyQt6.QtCore import QObject, QRunnable, pyqtSignal, pyqtSlot

from simsapa import logger
from simsapa.app.search.query_task import SearchQueryTask

class WorkerSignals(QObject):
    finished = pyqtSignal(datetime)

class SearchQueryWorker(QRunnable):
    signals: WorkerSignals

    def __init__(self, task: SearchQueryTask, finished_fn: Callable):
        super().__init__()

        self.will_emit_finished = True
        self.signals = WorkerSignals()
        self.task = task

        self.signals.finished.connect(finished_fn)

    @pyqtSlot()
    def run(self):
        logger.info("SearchQueryTask::run()")
        try:
            self.task.run()

            if self.will_emit_finished:
                self.signals.finished.emit(self.task.query_started_time)

        except Exception as e:
            logger.error(e)

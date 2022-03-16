from functools import partial
from PyQt5 import QtWidgets

from PyQt5.QtCore import QRunnable, QThreadPool, Qt, pyqtSlot
from PyQt5.QtCore import QObject, pyqtSignal
from PyQt5.QtWidgets import (QWidget, QVBoxLayout, QHBoxLayout, QPushButton, QLabel, QMainWindow)
from PyQt5.QtGui import QMovie

from simsapa.app.db.search import SearchIndexed

from simsapa.app.helpers import get_db_engine_connection_session

from simsapa import logger


class CreateSearchIndexWindow(QMainWindow):
    def __init__(self) -> None:
        super().__init__()
        self.setWindowTitle("Create the Search Index")
        self.setFixedSize(350, 350)
        self.setWindowFlags(Qt.WindowType.WindowStaysOnTopHint)

        self.thread_pool = QThreadPool()

        self.open_simsapa = False
        self._setup_ui()


    def _setup_ui(self):
        self._central_widget = QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._central_widget.setLayout(self._layout)

        spacerItem = QtWidgets.QSpacerItem(20, 0, QtWidgets.QSizePolicy.Minimum, QtWidgets.QSizePolicy.Expanding)
        self._layout.addItem(spacerItem)

        self._msg = QLabel("<p>The fulltext search index is empty. There will be<br> no search results in Simsapa without an index.</p><p>Start indexing now?<br>This may take 30-60 minutes.</p>")
        self._msg.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._layout.addWidget(self._msg)

        self._setup_animation()

        spacerItem = QtWidgets.QSpacerItem(20, 0, QtWidgets.QSizePolicy.Minimum, QtWidgets.QSizePolicy.Expanding)
        self._layout.addItem(spacerItem)

        self._setup_buttons()


    def _setup_animation(self):
        self._animation = QLabel(self)
        self._animation.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._layout.addWidget(self._animation)


    def _handle_open(self):
        self.open_simsapa = True
        self.close()


    def _setup_buttons(self):
        buttons_layout = QHBoxLayout()
        buttons_layout.setContentsMargins(0, 20, 0, 10)

        self._start_button = QPushButton("Start Indexing")
        self._start_button.setFixedSize(100, 30)

        self._open_button = QPushButton("Open Simsapa")
        self._open_button.setFixedSize(100, 30)

        self._quit_button = QPushButton("Quit")
        self._quit_button.setFixedSize(100, 30)

        self._start_button.clicked.connect(partial(self._run_indexing))
        self._open_button.clicked.connect(partial(self._handle_open))
        self._quit_button.clicked.connect(partial(self.close))

        buttons_layout.addWidget(self._quit_button)
        buttons_layout.addWidget(self._open_button)
        buttons_layout.addWidget(self._start_button)

        self._layout.addLayout(buttons_layout)


    def setup_animation(self):
        self._msg.setText("Indexing ...")
        self._start_button.setEnabled(False)
        self._open_button.setEnabled(False)

        self._movie = QMovie(':simsapa-loading')
        self._animation.setMovie(self._movie)


    def start_animation(self):
        self._movie.start()


    def stop_animation(self):
        self._movie.stop()


    def _run_indexing(self):
        indexing_worker = Worker()

        indexing_worker.signals.finished.connect(self._indexing_finished)

        self.thread_pool.start(indexing_worker)

        self.setup_animation()
        self.start_animation()


    def _indexing_finished(self):
        self.stop_animation()
        self._animation.deleteLater()

        self._msg.setText("<p>Indexing completed.</p><p>Quit and start the application again.</p>")


class WorkerSignals(QObject):
    finished = pyqtSignal()


class Worker(QRunnable):
    def __init__(self):
        super(Worker, self).__init__()

        self.signals = WorkerSignals()

    @pyqtSlot()
    def run(self):
        try:
            search_indexed = SearchIndexed()
            _, _, db_session = get_db_engine_connection_session()
            search_indexed.index_all(db_session, only_if_empty=True)

        except Exception as e:
            logger.error("Indexing problem: %s" % e)

        finally:
            self.signals.finished.emit()

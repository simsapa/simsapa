import logging as _logging
import os.path
from functools import partial
from typing import List, Optional
from sqlalchemy.sql import func  # type: ignore

import fitz  # type: ignore
from PyQt5.QtCore import QAbstractListModel, Qt
from PyQt5.QtGui import QIcon, QImage, QPixmap
from PyQt5.QtWidgets import (QFileDialog, QLabel, QMainWindow,  # type: ignore
                             QMessageBox)

from simsapa.assets import icons_rc

from ..app.db_models import Card  # type: ignore
from ..app.types import AppData  # type: ignore
from ..assets.ui.cards_browser_window_ui import \
    Ui_CardsBrowserWindow  # type: ignore

logger = _logging.getLogger(__name__)


class CardListModel(QAbstractListModel):
    def __init__(self, *args, cards=None, **kwargs):
        super(CardListModel, self).__init__(*args, **kwargs)
        self.cards = cards or []

    def data(self, index, role):
        if role == Qt.DisplayRole:
            text = self.cards[index.row()].front + " " + self.cards[index.row()].back
            return text

    def rowCount(self, index):
        return len(self.cards)


class CardsBrowserWindow(QMainWindow, Ui_CardsBrowserWindow):
    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self._app_data: AppData = app_data

        self.model = CardListModel(cards=self._get_all_cards())

        self.cards_list.setModel(self.model)
        self.sel_model = self.cards_list.selectionModel()

        self._ui_setup()
        self._connect_signals()

        self.statusbar.showMessage("Ready", 3000)

    def _ui_setup(self):
        self.status_msg = QLabel("")
        self.statusbar.addPermanentWidget(self.status_msg)

        self.front_input.setFocus()
        self._show_card_clear()

    def _get_all_cards(self) -> List[Card]:
        return self._app_data.user_db_session.query(Card).all()

    def _handle_query(self):
        pass

    def get_selected_card(self) -> Optional[Card]:
        a = self.cards_list.selectedIndexes()
        if not a:
            return None

        item = a[0]
        return self.model.cards[item.row()]

    def remove_selected_card(self):
        a = self.cards_list.selectedIndexes()
        if not a:
            return None

        # Remove from model
        item = a[0]
        card_id = self.model.cards[item.row()].id

        del self.model.cards[item.row()]
        self.model.layoutChanged.emit()
        self.cards_list.clearSelection()
        self._show_card_clear()

        # Remove from database

        db_item = self._app_data.user_db_session \
                                .query(Card) \
                                .filter(Card.id == card_id) \
                                .first()
        self._app_data.user_db_session.delete(db_item)
        self._app_data.user_db_session.commit()

    def _handle_card_select(self):
        card = self.get_selected_card()
        if card:
            self._show_card(card)

    def _show_card_clear(self):
        self.status_msg.clear()
        self.front_input.clear()
        self.back_input.clear()

    def _show_card(self, card: Card):
        self.front_input.setPlainText(card.front)
        self.back_input.setPlainText(card.back)

    def _cards_search_query(self, query: str):
        return []

    def add_card(self):

        front = self.front_input.toPlainText()
        back = self.back_input.toPlainText()

        if len(front) == 0 and len(back) == 0:
            logger.info("Empty content, cancel adding.")
            return

        # Insert database record

        logger.info(f"Adding new card")

        db_card = Card(
            front=front,
            back=back,
        )

        try:
            self._app_data.user_db_session.add(db_card)
            self._app_data.user_db_session.commit()

            # Add to model
            self.model.cards.append(db_card)

        except Exception as e:
            logger.error(e)

        self.model.layoutChanged.emit()

    def update_selected_card_front(self):
        card = self.get_selected_card()
        if card is None:
            return

        card.front = self.front_input.toPlainText()

        self._app_data.user_db_session.commit()
        self.model.layoutChanged.emit()

    def update_selected_card_back(self):
        card = self.get_selected_card()
        if card is None:
            return

        card.back = self.back_input.toPlainText()

        self._app_data.user_db_session.commit()
        self.model.layoutChanged.emit()

    def remove_card_dialog(self):
        card = self.get_selected_card()
        if not card:
            return

        reply = QMessageBox.question(self,
                                     'Remove Card...',
                                     'Remove this item?',
                                     QMessageBox.Yes | QMessageBox.No,
                                     QMessageBox.No)
        if reply == QMessageBox.Yes:
            self.remove_selected_card()

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.action_Add \
            .triggered.connect(partial(self.add_card))

        self.action_Remove \
            .triggered.connect(partial(self.remove_card_dialog))

        self.search_button.clicked.connect(partial(self._handle_query))
        self.search_input.textChanged.connect(partial(self._handle_query))

        self.front_input.textChanged.connect(partial(self.update_selected_card_front))
        self.back_input.textChanged.connect(partial(self.update_selected_card_back))

        self.sel_model.selectionChanged.connect(partial(self._handle_card_select))

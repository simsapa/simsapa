from typing import List, Optional, Dict

from PyQt6.QtCore import QModelIndex, QStringListModel, Qt, QObject
from PyQt6.QtWidgets import QCompleter

from simsapa.app.helpers import pali_to_ascii

# A QLineEdit/QComboBox search that ignores diacritics
# https://stackoverflow.com/questions/31465850/a-qlineedit-qcombobox-search-that-ignores-diacritics

WordSublists = Dict[str, List[str]]

class PaliCompleter(QCompleter):
    word_sublists: WordSublists = dict()
    _current_sublist_key: Optional[str] = None

    def __init__(self,
                 parent: Optional[QObject] = None,
                 word_sublists: WordSublists = dict(),
                 max_visible_items: int = 20):

        super(PaliCompleter, self).__init__(parent)

        self.setMaxVisibleItems(max_visible_items)
        self.setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)

        self.word_sublists = word_sublists

    def set_completion_list(self, items: List[str]):
        model = DiactricFreeStringListModel()
        model.setStringList(items)
        self.setModel(model)
        self.setCompletionRole(model.diactricFreeRole())

    def pathFromIndex(self, index: QModelIndex) -> str:
        return index.data()

    def splitPath(self, path: str) -> List[str]:
        if len(path) == 0:
            return [""]

        if len(path) < 3:
            # Wait until the user typed the third character.
            # The following effectively invalidates the completion.
            return [path+"..."]

        first_three_letters_ascii = pali_to_ascii(path[0:3])
        if self._current_sublist_key is None \
           or first_three_letters_ascii != self._current_sublist_key:
            self._current_sublist_key = first_three_letters_ascii

            if first_three_letters_ascii in self.word_sublists.keys():
                self.set_completion_list(self.word_sublists[first_three_letters_ascii])

        return [pali_to_ascii(path).lower()]

class DiactricFreeStringListModel(QStringListModel):
    custom_diactric_free_role: int

    def __init__(self, *args, **kwargs):
        super(DiactricFreeStringListModel, self).__init__(*args, **kwargs)
        # a new role value that is not already used by Qt
        self.setDiactricFreeRole(Qt.ItemDataRole.UserRole+10)

    def data(self, index: QModelIndex, role: int) -> str:
        if role == self.diactricFreeRole():
            m = super(DiactricFreeStringListModel, self)
            value = m.data(index, Qt.ItemDataRole.DisplayRole)

            return pali_to_ascii(value).lower()

        else:
            m = super(DiactricFreeStringListModel, self)
            value = m.data(index, role)
            return value

    def setDiactricFreeRole(self, role: int):
        self.custom_diactric_free_role = role

    def diactricFreeRole(self) -> int:
        return self.custom_diactric_free_role

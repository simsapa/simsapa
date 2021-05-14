import os.path
import epub_meta
from PyPDF2 import PdfFileReader
from typing import Optional

try:
    import fitz  # type: ignore
    has_fitz = True
except ImportError:
    has_fitz = False

import logging as _logging

logger = _logging.getLogger(__name__)


class PageImage():
    def __init__(self, file_doc, page_idx: int, zoom: Optional[float] = None):
        if not has_fitz:
            return None

        if file_doc is None or page_idx > file_doc.number_of_pages() - 1:
            return None

        self.zoom = zoom or file_doc._zoom
        self.pixmap = file_doc.file_doc[page_idx] \
                          .get_pixmap(
                              matrix=fitz.Matrix(self.zoom, self.zoom),
                              alpha=False
                          )

        self.image_bytes = self.pixmap.tobytes("ppm")

        self.width = self.pixmap.width
        self.height = self.pixmap.height
        self.stride = self.pixmap.stride


class FileDoc():
    def __init__(self, filepath):

        if not os.path.exists(filepath):
            logger.error(f"File not found: {filepath}")

        self.filepath = filepath
        name, ext = os.path.splitext(self.filepath)
        self.file_name = name
        self.file_ext = ext.lower()
        self.title = ""
        self.author = ""

        self._current_idx: int = 0
        self._zoom = 1.5

        if has_fitz:
            self.file_doc = fitz.open(filepath)
            self._matrix = fitz.Matrix(self._zoom, self._zoom)
        elif self.file_ext == ".pdf":
            self.file_doc = PdfFileReader(open(self.filepath, "rb"))
        else:
            self.file_doc = None

        self._parse_metadata()

    def number_of_pages(self) -> int:
        if has_fitz and self.file_doc:
            return len(self.file_doc)
        else:
            return 0

    def set_page_number(self, page: int):
        if page > 0:
            self._current_idx = page - 1

    def current_page_number(self) -> int:
        return self._current_idx + 1

    def current_page_idx(self) -> int:
        return self._current_idx

    def current_page_image(self) -> Optional[PageImage]:
        return self.page_image(self._current_idx)

    def page_image(self, page_idx: int, zoom: Optional[float] = None) -> Optional[PageImage]:
        if not has_fitz:
            return None

        return PageImage(self, page_idx, zoom)

    def _parse_metadata(self):
        if not self.file_doc:
            return

        if has_fitz:
            self.title = self.file_doc.metadata['title']
            self.author = self.file_doc.metadata['author']

        elif self.file_ext == ".pdf":
            info = self.file_doc.getDocumentInfo()
            self.title = info.title
            self.author = info.author

        elif self.file_ext == ".epub":
            info = epub_meta.get_epub_metadata(self.filepath, read_cover_image=False, read_toc=False)
            self.title = info['title']
            self.author = info['authors'][0]



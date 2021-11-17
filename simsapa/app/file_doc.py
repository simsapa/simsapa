import os.path
import epub_meta
from PyPDF2 import PdfFileReader
from typing import Optional, List

from PyQt5.QtCore import QRect

try:
    import fitz
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
        self.pixmap: fitz.Pixmap = file_doc.file_doc[page_idx] \
                                           .get_pixmap(
                                               matrix=fitz.Matrix(self.zoom, self.zoom),
                                               alpha=False)

        self.image_bytes = self.pixmap.tobytes("ppm")

        self.width = self.pixmap.width
        self.height = self.pixmap.height
        self.stride = self.pixmap.stride

    def set_pixmap(self, pixmap: fitz.Pixmap):
        self.pixmap = pixmap
        self.image_bytes = self.pixmap.tobytes("ppm")


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
        self.select_annot_id = None

        if has_fitz:
            self.file_doc = fitz.open(filepath)
        elif self.file_ext == ".pdf":
            self.file_doc = PdfFileReader(open(self.filepath, "rb"))
        else:
            self.file_doc = None

        self._current_idx: int = 0
        self._zoom = None
        self._matrix = None

        self.current_page_image = None
        self.set_zoom(1.5)

        self._parse_metadata()

    def set_zoom(self, x):
        self._zoom = x
        if has_fitz:
            self._matrix = fitz.Matrix(self._zoom, self._zoom)
            self.current_page_image = self.current_unchanged_page_image()

    def number_of_pages(self) -> int:
        if has_fitz and self.file_doc:
            return len(self.file_doc)
        else:
            return 0

    def go_to_page(self, page: int):
        if page <= 0:
            return

        if self._current_idx == page - 1:
            return

        self._current_idx = page - 1

        self.current_page_image = self.current_unchanged_page_image()

    def get_selection_rect(self, select_rect: QRect) -> Optional[fitz.Rect]:
        if not self.file_doc:
            return None

        page = self.current_page()
        page_image = self.current_unchanged_page_image()
        page_rect: fitz.Rect = page.rect

        s = select_rect.getCoords()

        # increase the selection area for better results
        c = [0, 0, 0, 0]
        c[0] = s[0] - 5
        c[1] = s[1] - 10
        c[2] = s[2] + 3
        c[3] = s[3] + 10

        # transform image_rect pixel coords to page_rect coords
        w_scale = page_rect.width / page_image.width
        h_scale = page_rect.height / page_image.height

        rect = fitz.Rect(w_scale * c[0], h_scale * c[1], w_scale * c[2], h_scale * c[3])

        return rect

    def get_selection_quads(self, select_rect: QRect) -> Optional[List[fitz.Quad]]:
        if not self.file_doc:
            return None

        page = self.current_page()
        rect = self.get_selection_rect(select_rect)
        text = page.get_textbox(rect)

        # using quads option
        quads = page.search_for(text, quads=True)

        if quads:
            # keep those text matches which are inside the selection area
            quads = list(filter(lambda q: rect.intersects(q.rect), quads))
        else:
            return None

        return quads

    def paint_selection_on_current(self, select_rect: QRect):
        if not self.file_doc:
            return None

        quads = self.get_selection_quads(select_rect)
        if not quads:
            self.current_page_image = self.current_unchanged_page_image()
            return

        if self.file_ext == ".pdf":
            # PDF supports drawing shapes.
            # We create a temp page to render drawings.

            self.file_doc.fullcopy_page(self._current_idx, -1)
            temp_page_idx = len(self.file_doc) - 1
            page = self.file_doc[temp_page_idx]

            shape: fitz.Shape = page.new_shape()
            orange = (0.913, 0.329, 0.125)  # rgb(233, 84, 32)

            for q in quads:
                rect = q.rect

                shape.draw_rect(rect)
                shape.finish(fill=orange)

            shape.commit(overlay=False)

            self.current_page_image = self.page_image(temp_page_idx)
            self.file_doc.delete_page(temp_page_idx)

        else:
            page = self.current_page()

            page_pixmap: fitz.Pixmap = page.get_pixmap(matrix=self._matrix, alpha=False)

            # page_pixmap: fitz.Pixmap = page.get_pixmap(matrix=self._matrix, alpha=True)
            # alphas = bytearray([255] * (page_pixmap.width * page_pixmap.height))
            # sel_pixmap: fitz.Pixmap()

            for q in quads:
                rect = q.rect
                rect.transform(self._matrix)

                page_pixmap.invert_irect(rect)

                # orange = (233, 84, 32)
                # page_pixmap.set_rect(rect, orange)

            # data = page_pixmap.tobytes("ppm")
            # pix = fitz.Pixmap()

            page_image = self.current_unchanged_page_image()
            page_image.set_pixmap(page_pixmap)
            self.current_page_image = page_image

    def get_selection_text(self, select_rect: QRect) -> Optional[str]:
        if not self.file_doc:
            return None

        page = self.current_page()
        rect = self.get_selection_rect(select_rect)
        text = page.get_textbox(rect)
        return text

    def select_highlight_text(self, select_rect: QRect) -> Optional[str]:
        if not self.file_doc:
            return None

        page = self.current_page()
        rect = self.get_selection_rect(select_rect)
        text = page.get_textbox(rect)

        quads = self.get_selection_quads(select_rect)

        if not quads:
            return text

        if self.select_annot_id:
            for a in page.annots():
                if a.info['id'] == self.select_annot_id:
                    page.delete_annot(a)

        annot = page.add_highlight_annot(quads=quads)
        if annot:
            self.select_annot_id = annot.info['id']

        return text

    def current_page_number(self) -> int:
        return self._current_idx + 1

    def current_page_idx(self) -> int:
        return self._current_idx

    def current_page(self) -> fitz.Page:
        # if not self.file_doc:
        #     raise ...
        return self.file_doc[self._current_idx]

    def current_unchanged_page_image(self) -> Optional[PageImage]:
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

import os.path
import epub_meta
from PyPDF2 import PdfFileReader
from typing import Optional

from PyQt5.QtCore import QRect

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
                                  alpha=False)

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
        self.select_annot_id = None

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

    def select_highlight_text(self, page_image: PageImage, select_rect: QRect) -> Optional[str]:
        if not self.file_doc:
            return None

        page = self.current_page()
        page_rect: fitz.Rect = page.rect

        if self.select_annot_id:
            for a in page.annots():
                if a.info['id'] == self.select_annot_id:
                    page.delete_annot(a)

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

        text = page.get_textbox(rect)

        # add highlight

        # NOTE: This method has problems with marking italic text.
        # It was presented in mark-lines2.py

        # rl = page.search_for(text)
        # if not rl:
        #     logger.info("Highlight location not found.")
        #     return text

        # start = rl[0].tl  # top-left of first rectangle
        # stop = rl[-1].br  # bottom-right of last rectangle
        # clip = fitz.Rect()  # build clip as union of the hit rectangles
        # for r in rl:
        #     clip |= r

        # page.add_highlight_annot(
        #     start=start,
        #     stop=stop,
        #     clip=clip,
        # )

        # using quads option
        quads = page.search_for(text, quads=True)

        if quads:
            # keep those text matches which are inside the selection area
            quads = list(filter(lambda q: rect.intersects(q.rect), quads))
        else:
            # logger.info("Highlight location not found.")
            return text

        annot = page.add_highlight_annot(quads=quads)
        if annot:
            self.select_annot_id = annot.info['id']

        return text

    def current_page_number(self) -> int:
        return self._current_idx + 1

    def current_page_idx(self) -> int:
        return self._current_idx

    def current_page(self):
        if self.file_doc:
            return self.file_doc[self._current_idx]

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

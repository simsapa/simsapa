import re
from urllib.parse import parse_qs
from typing import List, Optional

from PyQt6.QtCore import QUrl

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.types import AppData, QueryType, USutta

class SuttaQueries:
    def __init__(self, app_data: AppData):
        self._app_data = app_data


    def get_sutta_by_url(self, url: QUrl) -> Optional[USutta]:
        if url.host() != QueryType.suttas:
            return None

        query = parse_qs(url.query())
        quote = None
        if 'q' in query.keys():
            quote = query['q'][0]

        uid = url.path().strip("/")

        return self.get_sutta_by_uid(uid, quote)


    def get_sutta_by_uid(self, uid: str, highlight_text: Optional[str] = None) -> Optional[USutta]:
        if len(uid) == 0 and highlight_text is None:
            return None

        if len(uid) == 0 and highlight_text is not None:
            a = self.get_suttas_by_quote(highlight_text)
            if len(a) > 0:
                return a[0]
            else:
                return None

        if len(uid) > 0 and not self.is_complete_uid(uid):
            return self.get_sutta_by_partial_uid(uid, highlight_text)

        results: List[USutta] = []

        res = self._app_data.db_session \
            .query(Am.Sutta) \
            .filter(Am.Sutta.uid == uid) \
            .all()
        results.extend(res)

        res = self._app_data.db_session \
            .query(Um.Sutta) \
            .filter(Um.Sutta.uid == uid) \
            .all()
        results.extend(res)

        if len(results) > 0:
            return results[0]
        else:
            return None


    def is_complete_uid(self, uid: str) -> bool:
        uid = uid.strip("/")

        if "/" not in uid:
            return False

        if len(uid.split("/")) != 3:
            return False

        return True


    def get_sutta_by_partial_uid(self, part_uid: str, highlight_text: Optional[str] = None) -> Optional[USutta]:
        part_uid = part_uid.strip("/")

        if "/" not in part_uid:
            sutta_ref = part_uid
        else:
            sutta_ref = part_uid.split("/")[0]

        if re.match(r'^dhp[0-9]+$', sutta_ref) is not None:
            verse_num = int(sutta_ref.replace('dhp', ''))
            ch = self.dhp_verse_to_chapter(verse_num)
            if ch:
                sutta_ref = ch
            else:
                return None

        results: List[USutta] = []

        res = self._app_data.db_session \
            .query(Am.Sutta) \
            .filter(Am.Sutta.uid.like(f"{sutta_ref}/%")) \
            .all()
        results.extend(res)

        res = self._app_data.db_session \
            .query(Um.Sutta) \
            .filter(Um.Sutta.uid.like(f"{sutta_ref}/%")) \
            .all()
        results.extend(res)

        if len(results) == 0:
            return None

        if highlight_text:
            # FIXME bootstrap: commit content_plain as .lower()
            a = list(filter(lambda x: highlight_text.lower() in str(x.content_plain).lower(), results))
            if len(a) > 0:
                res_sutta = a[0]
            else:
                res_sutta = results[0]
        else:
            res_sutta = results[0]

        return res_sutta


    def get_suttas_by_quote(self, highlight_text: str) -> List[USutta]:
        if len(highlight_text) == 0:
            return []

        results: List[USutta] = []

        res = self._app_data.db_session \
            .query(Am.Sutta) \
            .filter(Am.Sutta.content_plain.like(f"%{highlight_text}%")) \
            .all()
        results.extend(res)

        res = self._app_data.db_session \
            .query(Um.Sutta) \
            .filter(Um.Sutta.content_plain.like(f"%{highlight_text}%")) \
            .all()
        results.extend(res)

        return results

    def dhp_verse_to_chapter(self, verse_num: int) -> Optional[str]:
        chapters = [
            [1, 20],
            [21, 32],
            [33, 43],
            [44, 59],
            [44, 59],
            [60, 75],
            [60, 75],
            [76, 89],
            [100, 115],
            [116, 128],
            [129, 145],
            [146, 156],
            [157, 166],
            [167, 178],
            [179, 196],
            [197, 208],
            [209, 220],
            [221, 234],
            [235, 255],
            [256, 272],
            [273, 289],
            [290, 305],
            [306, 319],
            [320, 333],
            [334, 359],
            [360, 382],
            [383, 423],
        ]

        for lim in chapters:
            a = lim[0]
            b = lim[1]
            if verse_num >= a and verse_num <= b:
                return f"dhp{a}-{b}"

        return None

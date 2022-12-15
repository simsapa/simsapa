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

        uid = re.sub(r"^/", "", url.path())
        query = parse_qs(url.query())
        quote = None
        if 'q' in query.keys():
            quote = query['q'][0]

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
            a = list(filter(lambda x: highlight_text in str(x.content_plain), results))
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

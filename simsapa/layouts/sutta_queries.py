import re
from urllib.parse import parse_qs
from typing import List, Optional
from enum import Enum

from PyQt6.QtCore import QUrl

from simsapa import logger
from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.helpers import consistent_nasal_m, expand_quote_to_pattern, normalize_sutta_ref, remove_punct
from simsapa.app.types import AppData, QueryType, USutta


class QuoteScope(str, Enum):
    Sutta = 'sutta'
    Nikaya = 'nikaya'
    All = 'all'


QuoteScopeValues = {
    'sutta': QuoteScope.Sutta,
    'nikaya': QuoteScope.Nikaya,
    'all': QuoteScope.All,
}


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

        quote_scope = QuoteScope.Sutta
        if 'quote_scope' in query.keys():
            sc = query['quote_scope'][0]
            if sc in QuoteScopeValues.keys():
                quote_scope = QuoteScopeValues[sc]

        uid = url.path().strip("/")

        return self.get_sutta_by_uid(uid, quote, quote_scope)


    def find_quote_in_suttas(self, suttas: List[USutta], quote: Optional[str] = None) -> Optional[USutta]:
        logger.info(f"find_quote_in_suttas(): {len(suttas)} suttas, {quote}")
        if quote is None or len(suttas) == 0:
            return None

        quote = quote.lower()
        quote = remove_punct(quote)
        quote = consistent_nasal_m(quote)

        p = expand_quote_to_pattern(quote)

        def _has_quote(x: USutta) -> bool:
            q = str(quote)
            content = str(x.content_plain)
            return (q in content or \
                    p.search(content) is not None)

        a = list(filter(_has_quote, suttas))

        if len(a) > 0:
            return a[0]
        else:
            return None


    def get_sutta_by_uid(self,
                         uid: str,
                         highlight_text: Optional[str] = None,
                         quote_scope = QuoteScope.Sutta) -> Optional[USutta]:

        logger.info(f"get_sutta_by_uid(): {uid}, {highlight_text}")
        if len(uid) == 0 and highlight_text is None:
            return None

        if len(uid) == 0 and highlight_text is not None:
            a = self.get_suttas_by_quote(highlight_text)
            if len(a) > 0:
                return a[0]
            else:
                return None

        if len(uid) > 0:
            uid = normalize_sutta_ref(uid)

        res_sutta = None

        if len(uid) > 0 and not self.is_complete_uid(uid):
            res_sutta = self.get_sutta_by_partial_uid(uid, highlight_text, quote_scope)

        if res_sutta is not None:
            return res_sutta

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

        if len(results) == 0:
            if quote_scope == QuoteScope.Sutta:
                return None

            elif quote_scope == QuoteScope.Nikaya:

                nikaya_uid = re.sub(r'[0-9\.-]+$', '', uid)

                res = self._app_data.db_session \
                    .query(Am.Sutta) \
                    .filter(Am.Sutta.uid.like(f"{nikaya_uid}%")) \
                    .all()
                results.extend(res)

                res = self._app_data.db_session \
                    .query(Um.Sutta) \
                    .filter(Um.Sutta.uid.like(f"{nikaya_uid}%")) \
                    .all()
                results.extend(res)

            elif quote_scope == QuoteScope.All:

                res = self.get_suttas_by_quote(highlight_text)
                results.extend(res)

        res_sutta = self.find_quote_in_suttas(results, highlight_text) or results[0]

        return res_sutta


    def is_complete_uid(self, uid: str) -> bool:
        uid = uid.strip("/")

        if "/" not in uid:
            return False

        if len(uid.split("/")) != 3:
            return False

        return True


    def get_sutta_by_partial_uid(self,
                                 part_uid: str,
                                 highlight_text: Optional[str] = None,
                                 quote_scope = QuoteScope.Sutta) -> Optional[USutta]:

        logger.info(f"get_sutta_by_partial_uid(): {part_uid}, {highlight_text}")
        part_uid = part_uid.strip("/")

        res_sutta = None

        if "/" not in part_uid:
            sutta_ref = part_uid
        else:
            sutta_ref = part_uid.split("/")[0]

        if re.match(r'^dhp[0-9]+$', sutta_ref) is not None:
            verse_num = int(sutta_ref.replace('dhp', ''))
            ch = self.dhp_verse_to_chapter(verse_num)
            if ch:
                sutta_ref = ch

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
            if quote_scope == QuoteScope.Sutta:
                return None

            elif quote_scope == QuoteScope.Nikaya:

                nikaya_uid = re.sub(r'[0-9\.-]+$', '', sutta_ref)

                res = self._app_data.db_session \
                    .query(Am.Sutta) \
                    .filter(Am.Sutta.uid.like(f"{nikaya_uid}%")) \
                    .all()
                results.extend(res)

                res = self._app_data.db_session \
                    .query(Um.Sutta) \
                    .filter(Um.Sutta.uid.like(f"{nikaya_uid}%")) \
                    .all()
                results.extend(res)

            elif quote_scope == QuoteScope.All:

                res = self.get_suttas_by_quote(highlight_text)
                results.extend(res)

        res_sutta = self.find_quote_in_suttas(results, highlight_text) or results[0]

        return res_sutta


    def get_suttas_by_quote(self, highlight_text: Optional[str] = None) -> List[USutta]:
        logger.info(f"get_suttas_by_quote(): {highlight_text}")
        if highlight_text is None or len(highlight_text) == 0:
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

        # FIXME expand quote text and match regex

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

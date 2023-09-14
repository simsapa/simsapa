import re
from urllib.parse import parse_qs
from typing import List, Optional

from PyQt6.QtCore import QUrl

from sqlalchemy.orm.session import Session

from simsapa import logger
from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.helpers import consistent_nasal_m, dhp_verse_to_chapter, expand_quote_to_pattern, normalize_sutta_ref, normalize_sutta_uid, remove_punct, snp_verse_to_uid, thag_verse_to_uid, thig_verse_to_uid
from simsapa.app.types import QueryType, SuttaQueriesInterface, SuttaQuote, USutta, QuoteScope, QuoteScopeValues

from simsapa.layouts.gui_types import sutta_quote_from_url

class SuttaQueries(SuttaQueriesInterface):
    db_session: Session
    api_url: Optional[str] = None
    completion_cache: List[str] = []

    def __init__(self, db_session: Session):
        self.db_session = db_session

    def get_sutta_by_url(self, url: QUrl) -> Optional[USutta]:
        if url.host() != QueryType.suttas:
            return None

        query = parse_qs(url.query())

        quote_scope = QuoteScope.Sutta
        if 'quote_scope' in query.keys():
            sc = query['quote_scope'][0]
            if sc in QuoteScopeValues.keys():
                quote_scope = QuoteScopeValues[sc]

        uid = url.path().strip("/")

        sutta = self.get_sutta_by_uid(uid, sutta_quote_from_url(url), quote_scope)

        if sutta:
            return sutta

        return self.get_sutta_by_ref(uid)


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
                         sutta_quote: Optional[SuttaQuote] = None,
                         quote_scope = QuoteScope.Sutta) -> Optional[USutta]:

        logger.info(f"get_sutta_by_uid(): {uid}, {sutta_quote}")
        if len(uid) == 0 and sutta_quote is None:
            return None

        if len(uid) == 0 and sutta_quote is not None:
            a = self.get_suttas_by_quote(sutta_quote['quote'])
            if len(a) > 0:
                return a[0]
            else:
                return None

        if len(uid) > 0:
            uid = normalize_sutta_uid(uid)

        res_sutta = None

        if len(uid) > 0 and not self.is_complete_uid(uid):
            res_sutta = self.get_sutta_by_partial_uid(uid, sutta_quote, quote_scope)

        if res_sutta is not None:
            return res_sutta

        results: List[USutta] = []

        res = self.db_session \
            .query(Am.Sutta) \
            .filter(Am.Sutta.uid == uid) \
            .all()
        results.extend(res)

        res = self.db_session \
            .query(Um.Sutta) \
            .filter(Um.Sutta.uid == uid) \
            .all()
        results.extend(res)

        if len(results) == 0:
            if quote_scope == QuoteScope.Sutta:
                return None

            elif quote_scope == QuoteScope.Nikaya:

                nikaya_uid = re.sub(r'[0-9\.-]+$', '', uid)

                res = self.db_session \
                    .query(Am.Sutta) \
                    .filter(Am.Sutta.uid.like(f"{nikaya_uid}%")) \
                    .all()
                results.extend(res)

                res = self.db_session \
                    .query(Um.Sutta) \
                    .filter(Um.Sutta.uid.like(f"{nikaya_uid}%")) \
                    .all()
                results.extend(res)

            elif sutta_quote and quote_scope == QuoteScope.All:

                res = self.get_suttas_by_quote(sutta_quote['quote'])
                results.extend(res)

        if len(results) == 0:
            return None

        if sutta_quote:
            res_sutta = self.find_quote_in_suttas(results, sutta_quote['quote']) or results[0]
        else:
            res_sutta = results[0]

        return res_sutta


    def get_sutta_by_ref(self, ref: str) -> Optional[USutta]:
        if len(ref) == 0:
            return None

        ref = normalize_sutta_ref(ref)
        ref = re.sub(r'pts *', '', ref)

        multi_refs = self.db_session \
            .query(Am.MultiRef) \
            .filter(Am.MultiRef.ref.like(f"%{ref}%")) \
            .all()

        if len(multi_refs) == 0:
            return None

        return multi_refs[0].suttas[0]


    def is_complete_uid(self, uid: str) -> bool:
        uid = uid.strip("/")

        if "/" not in uid:
            return False

        if len(uid.split("/")) != 3:
            return False

        return True


    def get_sutta_by_partial_uid(self,
                                 part_uid: str,
                                 sutta_quote: Optional[SuttaQuote] = None,
                                 quote_scope = QuoteScope.Sutta) -> Optional[USutta]:

        logger.info(f"get_sutta_by_partial_uid(): {part_uid}, {sutta_quote}")
        part_uid = part_uid.strip("/")

        res_sutta = None

        if "/" not in part_uid:
            sutta_ref = part_uid
        else:
            sutta_ref = part_uid.split("/")[0]

        if re.match(r'^dhp[0-9]+$', sutta_ref) is not None:
            verse_num = int(sutta_ref.replace('dhp', ''))
            ch = dhp_verse_to_chapter(verse_num)
            if ch:
                sutta_ref = ch

        if re.match(r'^snp[0-9]+$', sutta_ref) is not None:
            verse_num = int(sutta_ref.replace('snp', ''))
            ch = snp_verse_to_uid(verse_num)
            if ch:
                sutta_ref = ch

        if re.match(r'^thag[0-9]+$', sutta_ref) is not None:
            verse_num = int(sutta_ref.replace('thag', ''))
            ch = thag_verse_to_uid(verse_num)
            if ch:
                sutta_ref = ch

        if re.match(r'^thig[0-9]+$', sutta_ref) is not None:
            verse_num = int(sutta_ref.replace('thig', ''))
            ch = thig_verse_to_uid(verse_num)
            if ch:
                sutta_ref = ch

        results: List[USutta] = []

        res = self.db_session \
            .query(Am.Sutta) \
            .filter(Am.Sutta.uid.like(f"{sutta_ref}/%")) \
            .all()
        results.extend(res)

        res = self.db_session \
            .query(Um.Sutta) \
            .filter(Um.Sutta.uid.like(f"{sutta_ref}/%")) \
            .all()
        results.extend(res)

        if len(results) == 0:
            if quote_scope == QuoteScope.Sutta:
                return None

            elif quote_scope == QuoteScope.Nikaya:

                nikaya_uid = re.sub(r'[0-9\.-]+$', '', sutta_ref)

                res = self.db_session \
                    .query(Am.Sutta) \
                    .filter(Am.Sutta.uid.like(f"{nikaya_uid}%")) \
                    .all()
                results.extend(res)

                res = self.db_session \
                    .query(Um.Sutta) \
                    .filter(Um.Sutta.uid.like(f"{nikaya_uid}%")) \
                    .all()
                results.extend(res)

            elif sutta_quote and quote_scope == QuoteScope.All:

                res = self.get_suttas_by_quote(sutta_quote['quote'])
                results.extend(res)

        if len(results) == 0:
            return None

        if sutta_quote:
            res_sutta = self.find_quote_in_suttas(results, sutta_quote['quote']) or results[0]
        else:
            res_sutta = results[0]

        return res_sutta


    def get_suttas_by_quote(self, highlight_text: Optional[str] = None) -> List[USutta]:
        logger.info(f"get_suttas_by_quote(): {highlight_text}")
        if highlight_text is None or len(highlight_text) == 0:
            return []

        results: List[USutta] = []

        res = self.db_session \
            .query(Am.Sutta) \
            .filter(Am.Sutta.content_plain.like(f"%{highlight_text}%")) \
            .all()
        results.extend(res)

        res = self.db_session \
            .query(Um.Sutta) \
            .filter(Um.Sutta.content_plain.like(f"%{highlight_text}%")) \
            .all()
        results.extend(res)

        # FIXME expand quote text and match regex

        return results


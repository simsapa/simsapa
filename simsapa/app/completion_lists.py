from typing import List, Dict, Optional, Union
import re, json

from sqlalchemy.orm.session import Session
from sqlalchemy.sql import func

from simsapa import logger, DbSchemaName

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db import dpd_models as Dpd

from simsapa.app.helpers import pali_to_ascii
from simsapa.app.types import SearchArea

UAppSetting = Union[Am.AppSetting, Um.AppSetting]

WordSublists = Dict[str, List[str]]

def get_sutta_titles_completion_list(db_session: Session, load_only_from_appdata = False) -> WordSublists:
    res = []
    r = db_session.query(Am.Sutta.title).all()
    res.extend(r)

    if not load_only_from_appdata:
        r = db_session.query(Um.Sutta.title).all()
        res.extend(r)

    a: List[str] = list(map(lambda x: x[0].strip() or 'none', res))
    # remove parens from the beginning: "(iv, 6): das kloster"
    b = list(map(lambda x: re.sub(r'^\([^\)]+\)[: ]*', '', x.lower()), a))
    b.sort()
    titles = list(set(b))

    sublists = parse_to_sublists(titles)

    return sublists

def get_dict_words_completion_list(db_session: Session, load_only_from_appdata = False) -> WordSublists:
    res = []
    r = db_session.query(Am.DictWord.word).all()
    res.extend(r)

    r = db_session.query(Dpd.PaliWord.pali_1).all()
    res.extend(r)

    r = db_session.query(Dpd.PaliRoot.root_no_sign).all()
    res.extend(r)

    r = db_session.query(Dpd.Sandhi.sandhi).all()
    res.extend(r)

    if not load_only_from_appdata:
        r = db_session.query(Um.DictWord.word).all()
        res.extend(r)

    a: List[str] = list(map(lambda x: x[0].strip() or 'none', res))
    # remove trailing numbers: dhamma 01, dhamma 02, etc
    b = list(map(lambda x: re.sub(r' *\d+$', '', x.lower()), a))
    b.sort()
    words = list(set(b))

    sublists = parse_to_sublists(words)

    return sublists

def parse_to_sublists(items: List[str]) -> WordSublists:

    sublists: WordSublists = dict()

    for word in items:
        if len(word) < 4:
            continue
        first_three_letters_ascii = pali_to_ascii(word[0:3])

        if first_three_letters_ascii not in sublists.keys():
            sublists[first_three_letters_ascii] = []

        sublists[first_three_letters_ascii].append(word)

    return sublists

def get_and_save_completions(db_session: Session,
                             search_area: SearchArea,
                             save_to_schema: DbSchemaName = DbSchemaName.UserData,
                             load_only_from_appdata = False) -> WordSublists:
    if search_area == SearchArea.Suttas:
        setting_key = 'sutta_titles_completions'
    else:
        setting_key = 'dict_words_completions'

    # Retreive setting from userdata first, if exists. If the completions are
    # updated after installing the app, they are saved to userdata.

    r: Optional[UAppSetting] = None

    if not load_only_from_appdata:
        r = db_session \
            .query(Um.AppSetting) \
            .filter(Um.AppSetting.key == setting_key) \
            .first()

    if r is None:
        r = db_session \
            .query(Am.AppSetting) \
            .filter(Am.AppSetting.key == setting_key) \
            .first()

    if r is None:
        if search_area == SearchArea.Suttas:
            sublists = get_sutta_titles_completion_list(db_session, load_only_from_appdata)
        elif search_area == SearchArea.DictWords:
            sublists = get_dict_words_completion_list(db_session, load_only_from_appdata)
        else:
            raise Exception(f"Unknown SearchArea: {search_area}")

        if save_to_schema == DbSchemaName.AppData:
            x = Am.AppSetting(
                key = setting_key,
                value = json.dumps(sublists),
                created_at = func.now(),
            )

        elif save_to_schema == DbSchemaName.UserData:
            x = Um.AppSetting(
                key = setting_key,
                value = json.dumps(sublists),
                created_at = func.now(),
            )

        else:
            raise Exception("Only appdata and userdata schema are allowed.")

        db_session.add(x)
        db_session.commit()

    else:
        if r.value is not None:
            sublists: WordSublists = json.loads(r.value)
        else:
            logger.error("Completion list is empty")
            sublists = dict()

    return sublists

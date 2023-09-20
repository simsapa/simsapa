from pathlib import Path
from typing import List, Optional
from collections import namedtuple

from sqlalchemy.orm.session import Session
from sqlalchemy.sql import func, text

from simsapa import logger
from simsapa.app.db import appdata_models as Am
from simsapa.app.helpers import consistent_nasal_m, compact_rich_text, pali_to_ascii

import helpers

def populate_nyanatiloka_dict_words_from_legacy(appdata_db: Session, bootstrap_assets_dir: Path, limit: Optional[int] = None):
    logger.info("Adding Nyanatiloka DictWords from legacy dict_words")

    legacy_db_path = bootstrap_assets_dir.joinpath("legacy-db").joinpath("appdata-legacy.sqlite3")
    legacy_db = helpers.get_db_session(legacy_db_path)

    label = 'NYANAT'
    # create the dictionary
    dictionary = Am.Dictionary(
        label = label,
        title = "Nyanatiloka's Buddhist Dictionary",
        created_at = func.now(),
    )

    try:
        appdata_db.add(dictionary)
        appdata_db.commit()
    except Exception as e:
        logger.error(e)
        exit(1)

    # get words and commit to appdata db

    # label is stored lowercase in legacy db
    if limit:
        a = legacy_db.execute(text(f"SELECT * from dict_words WHERE entry_source = '{label.lower()}' LIMIT {limit};"))
    else:
        a = legacy_db.execute(text(f"SELECT * from dict_words WHERE entry_source = '{label.lower()}';"))

    LegacyDictWord = namedtuple('LegacyDictWord', a.keys())
    records = [LegacyDictWord(*r) for r in a.fetchall()]

    def _legacy_to_dict_word(x: LegacyDictWord) -> Am.DictWord:
        # all-lowercase uid
        uid = f"{x.word}/{label}".lower()
        word = consistent_nasal_m(x.word)
        word_ascii = pali_to_ascii(word)
        return Am.DictWord(
            dictionary_id = dictionary.id,
            word = word,
            word_ascii = word_ascii,
            language = "en",
            uid = uid,
            source_uid = label,
            definition_plain = compact_rich_text(x.definition_plain),
            definition_html = consistent_nasal_m(x.definition_html),
            summary = consistent_nasal_m(x.summary),
            created_at = func.now(),
        )

    dict_words: List[Am.DictWord] = list(map(_legacy_to_dict_word, records))

    legacy_db.close()

    try:
        for i in dict_words:
            appdata_db.add(i)
        appdata_db.commit()
    except Exception as e:
        logger.error(e)
        exit(1)

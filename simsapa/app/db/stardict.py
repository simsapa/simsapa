"""Stardict related database funcions
"""

import logging as _logging

from typing import Optional, List, TypedDict

from sqlalchemy.sql import func
from sqlalchemy.dialects.sqlite import insert

from simsapa.app.stardict import DictEntry, StarDictPaths, stardict_to_dict_entries, parse_ifo

from . import appdata_models as Am
from . import userdata_models as Um

logger = _logging.getLogger(__name__)

class DbDictEntry(TypedDict):
    word: str
    definition_plain: str
    definition_html: str
    synonyms: str
    uid: str
    dictionary_id: int

def db_entries(x: DictEntry, dictionary_id: int, dictionary_label: str) -> DbDictEntry:
    # TODO should we check for conflicting uids? generate with meaning count?
    uid = f"{x['word']}/{dictionary_label}".lower()
    return DbDictEntry(
        # copy values
        word = x['word'],
        definition_plain = x['definition_plain'],
        definition_html = x['definition_html'],
        synonyms = ", ".join(x['synonyms']),
        # add missing data
        uid = uid,
        dictionary_id = dictionary_id,
    )

def insert_db_words(db_session, schema_name: str, db_words: List[DbDictEntry], batch_size = 1000):
    inserted = 0

    # TODO: The user can't see this message. Dialog doesn't update while the
    # import is blocking the GUI.
    # self.msg.setText("Importing ...")
    print("Importing ...")

    while inserted <= len(db_words):
        b_start = inserted
        b_end = inserted + batch_size
        words_batch = db_words[b_start:b_end]

        try:
            if schema_name == 'userdata':
                stmt = insert(Um.DictWord).values(words_batch)
            else:
                stmt = insert(Am.DictWord).values(words_batch)

            # update the record if uid already exists
            stmt = stmt.on_conflict_do_update(
                index_elements = [Um.DictWord.uid],
                set_ = dict(
                    word = stmt.excluded.word,
                    word_nom_sg = stmt.excluded.word_nom_sg,
                    inflections = stmt.excluded.inflections,
                    phonetic = stmt.excluded.phonetic,
                    transliteration = stmt.excluded.transliteration,
                    # --- Meaning ---
                    meaning_order = stmt.excluded.meaning_order,
                    definition_plain = stmt.excluded.definition_plain,
                    definition_html = stmt.excluded.definition_html,
                    summary = stmt.excluded.summary,
                    # --- Associated words ---
                    synonyms = stmt.excluded.synonyms,
                    antonyms = stmt.excluded.antonyms,
                    homonyms = stmt.excluded.homonyms,
                    also_written_as = stmt.excluded.also_written_as,
                    see_also = stmt.excluded.see_also,
                )
            )

            db_session.execute(stmt)
            db_session.commit()
        except Exception as e:
            print(e)
            logger.error(e)

        inserted += batch_size
        # self.msg.setText(f"Imported {inserted} ...")
        print(f"Imported {inserted} ...")

def import_stardict_into_db_update_existing(db_session,
                                            schema_name: str,
                                            paths: StarDictPaths,
                                            dictionary_id: int,
                                            label: str,
                                            batch_size = 1000):
    words: List[DictEntry] = stardict_to_dict_entries(paths)
    db_words: List[DbDictEntry] = list(map(lambda x: db_entries(x, dictionary_id, label), words))
    insert_db_words(db_session, schema_name, db_words, batch_size)

def import_stardict_into_db_as_new(db_session,
                                   schema_name: str,
                                   paths: StarDictPaths,
                                   label: Optional[str] = None,
                                   batch_size = 1000):
    # upsert recommended by docs instead of bulk_insert_mappings
    # Using PostgreSQL ON CONFLICT with RETURNING to return upserted ORM objects
    # https://docs.sqlalchemy.org/en/14/orm/persistence_techniques.html#using-postgresql-on-conflict-with-returning-to-return-upserted-orm-objects

    words: List[DictEntry] = stardict_to_dict_entries(paths)
    ifo = parse_ifo(paths)
    title = ifo['bookname']
    if label is None:
        label = title

    # create a dictionary, commit to get its ID
    if schema_name == 'userdata':
        dictionary = Um.Dictionary(
            title = title,
            label = label,
            created_at = func.now(),
        )
    else:
        dictionary = Am.Dictionary(
            title = title,
            label = label,
            created_at = func.now(),
        )

    try:
        db_session.add(dictionary)
        db_session.commit()
    except Exception as e:
        logger.error(e)

    d_id: int = dictionary.id # type: ignore
    db_words: List[DbDictEntry] = list(map(lambda x: db_entries(x, d_id, label), words))

    insert_db_words(db_session, schema_name, db_words, batch_size)

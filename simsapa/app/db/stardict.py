"""Stardict related database funcions
"""

from typing import Optional, List, TypedDict

from sqlalchemy.sql import func
from sqlalchemy.orm.session import Session
from sqlalchemy.dialects.sqlite import insert
from simsapa.app.search.tantivy_index import TantivySearchIndexes
from simsapa.app.helpers import latinize, pali_to_ascii, word_uid

from simsapa.app.stardict import DictEntry, StarDictPaths, parse_bword_links_to_ssp, stardict_to_dict_entries, parse_ifo
from simsapa.app.dict_link_helpers import add_epd_pali_words_links, add_example_links, add_grammar_links, add_sandhi_links
from simsapa.app.export_helpers import add_sutta_links
from simsapa.app.types import UDictWord
from simsapa import DbSchemaName, DictTypeName, logger

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

class DbDictEntry(TypedDict):
    word: str
    word_ascii: str
    language: str
    definition_plain: str
    definition_html: str
    synonyms: str
    uid: str
    source_uid: str
    dictionary_id: int

def db_entries(x: DictEntry, dictionary_id: int, dictionary_label: str, lang: str) -> DbDictEntry:
    # TODO should we check for conflicting uids? generate with meaning count?
    uid = word_uid(x['word'], dictionary_label)

    # add a Latinized lowercase synonym
    syn = x['synonyms']
    latin = latinize(x['word']).lower()
    if latin not in syn:
        syn.append(latin)

    return DbDictEntry(
        # copy values
        word = x['word'],
        word_ascii = pali_to_ascii(x['word']),
        language = lang,
        definition_plain = x['definition_plain'],
        definition_html = x['definition_html'],
        synonyms = ", ".join(syn),
        # add missing data
        uid = uid,
        source_uid = dictionary_label.lower(),
        dictionary_id = dictionary_id,
    )

def insert_db_words(db_session,
                    schema_name: str,
                    db_words: List[DbDictEntry],
                    batch_size = 1000) -> List[str]:
    inserted = 0
    uids: List[str] = []

    # TODO: The user can't see this message. Dialog doesn't update while the
    # import is blocking the GUI.
    # self.msg.setText("Importing ...")
    logger.info("Importing ...")

    while inserted <= len(db_words):
        b_start = inserted
        b_end = inserted + batch_size
        words_batch = db_words[b_start:b_end]

        try:
            if schema_name == DbSchemaName.UserData.value:
                stmt = insert(Um.DictWord).values(words_batch)
            elif schema_name == DbSchemaName.AppData.value:
                stmt = insert(Am.DictWord).values(words_batch)
            else:
                raise Exception("Only appdata and userdata schema are allowed.")

            # update the record if uid already exists
            stmt = stmt.on_conflict_do_update(
                index_elements = [Um.DictWord.uid],
                set_ = dict(
                    source_uid = stmt.excluded.source_uid,
                    word = stmt.excluded.word,
                    word_ascii = stmt.excluded.word_ascii,
                    language = stmt.excluded.language,
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
            logger.error(e)

        uids.extend(list(map(lambda x: x['uid'], words_batch)))
        inserted += batch_size
        # self.msg.setText(f"Imported {inserted} ...")
        logger.info(f"Imported {inserted}")

    return uids

def import_stardict_update_existing(db_session,
                                    schema_name: str,
                                    search_indexes: Optional[TantivySearchIndexes],
                                    paths: StarDictPaths,
                                    lang: str,
                                    dictionary_id: int,
                                    label: str,
                                    batch_size = 1000,
                                    ignore_synonyms = False):

    if ignore_synonyms:
        paths['syn_path'] = None

    words: List[DictEntry] = stardict_to_dict_entries(paths)
    db_words: List[DbDictEntry] = list(map(lambda x: db_entries(x, dictionary_id, label, lang), words))
    uids = insert_db_words(db_session, schema_name, db_words, batch_size)

    if search_indexes is not None:
        if schema_name == DbSchemaName.AppData.value:
            w: List[UDictWord] = db_session \
                .query(Am.DictWord) \
                .filter(Am.DictWord.uid.in_(uids)) \
                .all()

        elif schema_name == DbSchemaName.UserData.value:
            w: List[UDictWord] = db_session \
                .query(Um.DictWord) \
                .filter(Um.DictWord.uid.in_(uids)) \
                .all()
        else:
            raise Exception("Only appdata and userdata schema are allowed.")

        search_indexes.index_dict_words(search_indexes.dict_words_lang_index[lang], schema_name, w)

def add_links_to_words(db_session: Session,
                       words: List[DictEntry]) -> List[DictEntry]:
    logger.info(f"add_links_to_words(): len(words) = {len(words)}")

    results: List[DictEntry] = []

    for w in words:
        html = w['definition_html']

        html = parse_bword_links_to_ssp(html)

        if 'id="example__' in html:
            html = add_example_links(html)

        if 'id="declension__' not in html:
            # dpd-grammar doesn't have a declension div.
            html = add_grammar_links(html)

        html = add_sandhi_links(html)

        html = add_epd_pali_words_links(html)

        html = add_sutta_links(db_session, html)

        w['definition_html'] = html

        results.append(w)

    return results

def import_stardict_as_new(db_session: Session,
                           schema_name: str,
                           search_indexes: Optional[TantivySearchIndexes],
                           paths: StarDictPaths,
                           lang: str,
                           label: Optional[str] = None,
                           batch_size = 1000,
                           ignore_synonyms = False,
                           limit: Optional[int] = None):
    logger.info("=== import_stardict_as_new() ===")

    if ignore_synonyms:
        paths['syn_path'] = None

    # upsert recommended by docs instead of bulk_insert_mappings
    # Using PostgreSQL ON CONFLICT with RETURNING to return upserted ORM objects
    # https://docs.sqlalchemy.org/en/14/orm/persistence_techniques.html#using-postgresql-on-conflict-with-returning-to-return-upserted-orm-objects

    words: List[DictEntry] = stardict_to_dict_entries(paths, limit)

    words = add_links_to_words(db_session, words)

    ifo = parse_ifo(paths)
    title = ifo['bookname']
    if label is None:
        label = title

    logger.info(f"Importing {ifo['bookname']} ...")

    # create a dictionary, commit to get its ID
    if schema_name == DbSchemaName.UserData.value:
        dictionary = Um.Dictionary(
            title = title,
            label = label,
            dict_type = DictTypeName.Stardict.value,
            created_at = func.now(),
        )
    elif schema_name == DbSchemaName.AppData.value:
        dictionary = Am.Dictionary(
            title = title,
            label = label,
            dict_type = DictTypeName.Stardict.value,
            created_at = func.now(),
        )
    else:
        raise Exception("Only appdata and userdata schema are allowed.")

    try:
        db_session.add(dictionary)
        db_session.commit()
    except Exception as e:
        logger.error(e)

    d_id: int = int(str(dictionary.id))
    db_words: List[DbDictEntry] = list(map(lambda x: db_entries(x, d_id, label, lang), words))
    uids = insert_db_words(db_session, schema_name, db_words, batch_size)

    if search_indexes is not None:
        if schema_name == DbSchemaName.AppData.value:
            w: List[UDictWord] = db_session \
                .query(Am.DictWord) \
                .filter(Am.DictWord.uid.in_(uids)) \
                .all()
        elif schema_name == DbSchemaName.UserData.value:
            w: List[UDictWord] = db_session \
                .query(Um.DictWord) \
                .filter(Um.DictWord.uid.in_(uids)) \
                .all()
        else:
            raise Exception("Only appdata and userdata schema are allowed.")

        search_indexes.index_dict_words(search_indexes.dict_words_lang_index[lang], schema_name, w)

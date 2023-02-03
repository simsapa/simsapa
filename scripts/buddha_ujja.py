#!/usr/bin/env python3

import os
from collections import namedtuple
import sys
from pathlib import Path
import re
from typing import List, Optional

from dotenv import load_dotenv
import markdown

from sqlalchemy.sql import func
from sqlalchemy.orm.session import Session

from simsapa import DbSchemaName, logger
from simsapa.app.db import userdata_models as Um

import helpers
from simsapa.app.helpers import create_app_dirs, compact_rich_text, consistent_nasal_m, sutta_range_from_ref, thig_verse_to_uid, dhp_chapter_ref_for_verse_num

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

BOOTSTRAP_ASSETS_DIR = Path(s)

if not BOOTSTRAP_ASSETS_DIR.exists():
    logger.error(f"Missing folder: {BOOTSTRAP_ASSETS_DIR}")
    sys.exit(1)

s = os.getenv('BOOTSTRAP_LIMIT')
if s is None or s == "":
    BOOTSTRAP_LIMIT = None
else:
    BOOTSTRAP_LIMIT = int(s)


def _code_to_uid(code: str) -> str:
    s = code.lower()

    if 'dhp-' in s:
        # dhp-11-vagga
        n = int(re.sub(r'dhp-([0-9]+)-vagga', r'\1', s))
        uid = dhp_chapter_ref_for_verse_num(n)
        if uid is None:
            logger.error(f"No uid for {s}")

        s = str(uid)

    elif 'thig-' in s:
        # thig-5.67-121
        # Convert verse numbers to SC chapter range.
        n = int(re.sub(r'thig-[0-9]+\.([0-9]+)-.*', r'\1', s))
        uid = thig_verse_to_uid(n)
        if uid is None:
            logger.error(f"No uid for {s}")

        s = str(uid)

    elif s == "mv-10.2.3-20":
        # Dīghāvu Vatthu
        # pli-tv-kd10 Contains the Kosambiya Jataka about Dighavu
        s = "pli-tv-kd10"

    elif s == "mv-1.1.5-8":
        # Mahāvagga
        # pli-tv-kd1 Contains the story with Upaka.
        s = "pli-tv-kd1"

    else:
        # TODO mv-1.6.13-14
        # sn56.11 Dhammacakka, double ref?
        #
        # thag-8.1
        # 8.1 is identical to SC number.
        #
        # an-10.13 -> an10.13
        # an-1.296-305 -> an1.296-305
        s = re.sub(r'^([a-z]+)-([0-9\.]+)(.*)', r'\1\2\3', s)

    return s

def get_suttas(db_session: Session, limit: Optional[int] = None) -> List[Um.Sutta]:
    if limit:
        a = db_session.execute(f"SELECT * from suttas WHERE language_code = 'hu' AND is_published = 1 LIMIT {limit};") # type: ignore
    else:
        a = db_session.execute("SELECT * from suttas WHERE language_code = 'hu' AND is_published = 1;") # type: ignore

    BuSutta = namedtuple('LegacyDictWord', a.keys())
    records = [BuSutta(*r) for r in a.fetchall()]

    def _to_sutta(x: BuSutta) -> Um.Sutta:

        # get sutta author
        a = db_session.execute(f"SELECT author_id from sutta_author WHERE sutta_id = {x.id};") # type: ignore
        r = a.fetchone()
        if r is None:
            logger.error(f"Can't get author for: {x.sutta_ref_code}")
            sys.exit(1)
        else:
            author_id = r[0]

        a = db_session.execute(f"SELECT author_ref_code from authors WHERE id = {author_id};") # type: ignore
        r = a.fetchone()
        if r is None:
            logger.error(f"Can't get author_ref_code for: {author_id}")
            sys.exit(1)
        else:
            author_ref = r[0]

        sutta_uid = _code_to_uid(x.sutta_ref_code)

        uid = f"{sutta_uid}/hu/{author_ref}".lower()

        # x.sutta_pts: A V 185

        # x.title: AN 2.31-32 Kataññu Sutta
        title = re.sub(r'^[a-zA-Z]+ [0-9\.-]+ (.*)', r'\1', x.title)

        # take the first part of the uid
        s = uid.split('/')[0]
        sutta_ref = helpers.uid_to_ref(s)

        content_text = consistent_nasal_m(str(x.markdown_content))
        # line break is two trailing spaces, not trailing \
        content_text = re.sub(r'\\$', '  ', content_text)

        content_main = markdown.markdown(
            text = content_text,
            extensions = ['footnotes', 'smarty'],
        )

        if x.license == "cc-by-nc-sa":
            license_html = """
            <p>Ez a Mű a <a rel="license" href="http://creativecommons.org/licenses/by-nc-sa/4.0/">Creative Commons Nevezd meg! - Ne add el! - Így add tovább! 4.0 Nemzetközi Licenc</a> feltételeinek megfelelően felhasználható.</p>
            """
        else:
            license_html = f"<p>License: {x.license}</p>"

        content_html = f"""
        <h1>{sutta_ref} {title}</h1>
        {content_main}
        <p>&nbsp;</p>
        <footer class="noindex">
            <p>Copyright &copy; {x.copyright}</p>
            {license_html}
        </footer>
        """
        content_plain = compact_rich_text(content_html)

        sutta_range = sutta_range_from_ref(uid)
        if not sutta_range:
            logger.error(f"Can't determine sutta range: {uid}")

        return Um.Sutta(
            uid = uid,
            source_uid = author_ref,
            source_info = x.sutta_pts,
            language = 'hu',

            title = consistent_nasal_m(title),
            title_pali = consistent_nasal_m(x.sutta_title_pali),
            title_trans = x.sutta_title_trans,
            content_plain = content_plain,
            content_html = content_html,

            sutta_ref = sutta_ref,
            sutta_range_group = sutta_range['group'] if sutta_range else None,
            sutta_range_start = sutta_range['start'] if sutta_range else None,
            sutta_range_end = sutta_range['end'] if sutta_range else None,

            copyright = x.copyright,
            license = x.license,

            created_at = func.now(),
        )

    suttas: List[Um.Sutta] = list(map(_to_sutta, records))

    return suttas

def populate_suttas_from_buddha_ujja(userdata_db: Session, bu_db_path: Path, limit: Optional[int] = None):
    logger.info("=== populate_suttas_from_buddha_ujja() ===")

    db_session = helpers.get_db_session(bu_db_path)

    db_path = Path("")

    suttas = get_suttas(db_session, limit)
    if len(suttas) == 0:
        logger.error(f"No suttas found in {db_path}, exiting.")
        sys.exit(1)

    logger.info(f"Adding {len(suttas)} ...")

    try:
        # TODO: bulk insert errors out
        # NOTE: this is slow but works
        for i in suttas:
            author = None
            if i.source_uid:
                res = userdata_db.query(Um.Author).filter(Um.Author.uid == i.source_uid).first()
                if res is None:
                    a = db_session.execute(f"SELECT full_name from authors WHERE author_ref_code = '{i.source_uid}';") # type: ignore
                    r = a.fetchone()
                    full_name = r[0] if r is not None else None

                    author = Um.Author(
                        uid = i.source_uid,
                        full_name = full_name,
                    )

            ref = None
            if i.source_info:
                collection = re.sub(r'^([a-z]+)\d.*', r'\1', str(i.uid))
                ref = helpers.text_to_multi_ref(collection, str(i.source_info), DbSchemaName.UserData)
                i.source_info = None # type: ignore

            userdata_db.add(i)
            userdata_db.commit()

            if author:
                userdata_db.add(author)
                i.authors.append(author)

            if ref:
                userdata_db.add(ref)
                i.multi_refs.append(ref)

            userdata_db.commit()

    except Exception as e:
        logger.error(e)
        sys.exit(1)

    db_session.close()

    logger.info("DONE")

def main():
    name = "suttas_lang_hu.sqlite3"

    create_app_dirs()

    db_path = BOOTSTRAP_ASSETS_DIR.joinpath("dist").joinpath(name)
    db_session = helpers.get_simsapa_db(db_path, DbSchemaName.UserData, remove_if_exists = True)

    bu_db_path = BOOTSTRAP_ASSETS_DIR.joinpath("buddha-ujja-sql").joinpath("bu.sqlite3")

    limit = BOOTSTRAP_LIMIT

    populate_suttas_from_buddha_ujja(db_session, bu_db_path, limit)

if __name__ == "__main__":
    main()

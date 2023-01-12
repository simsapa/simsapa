#!/usr/bin/env python3

import os
import sys
import csv
from pathlib import Path
from typing import Dict, List, Optional, TypedDict
from dotenv import load_dotenv

from sqlalchemy.orm.session import Session
from sqlalchemy import or_, and_

from simsapa.app.db import appdata_models as Am
from simsapa import logger
from simsapa.app.helpers import normalize_sutta_ref, sutta_range_from_ref

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

bootstrap_assets_dir = Path(s)

CSV_PATH = bootstrap_assets_dir.joinpath('pts-refs/pts_refs.csv')


class PtsRef(TypedDict):
    collection: Optional[str]

    sc_ref: Optional[str]
    sc_uid: Optional[str]

    pts_ref: Optional[str]
    pts_edition: Optional[str]
    dpr_ref: Optional[str]
    cst4_ref: Optional[str]
    trad_ref: Optional[str]

    verse_start: Optional[int]
    verse_end: Optional[int]


def populate_sutta_multi_refs(appdata_db: Session, limit: Optional[int] = None):
    logger.info("=== populate_sutta_multi_refs() ===")

    with open(CSV_PATH, 'r') as f:
        reader = csv.DictReader(f)

        def _row_to_ref(r: Dict[str, str]) -> PtsRef:
            return PtsRef(
                collection = r['collection'] if r['collection'] != '' else None,
                sc_ref = r['sc_ref'] if r['sc_ref'] != '' else None,
                sc_uid = r['sc_uid'] if r['sc_uid'] != '' else None,
                pts_ref = r['pts_ref'] if r['pts_ref'] != '' else None,
                pts_edition = r['pts_edition'] if r['pts_edition'] != '' else None,
                dpr_ref = r['dpr_ref'] if r['dpr_ref'] != '' else None,
                cst4_ref = r['cst4_ref'] if r['cst4_ref'] != '' else None,
                trad_ref = r['trad_ref'] if r['trad_ref'] != '' else None,
                verse_start = int(r['verse_start']) if r['verse_start'] != '' else None,
                verse_end = int(r['verse_end']) if r['verse_end'] != '' else None,
            )

        pts_refs: List[PtsRef] = list(map(_row_to_ref, reader))

    multi_refs = []

    for ref in pts_refs:
        if ref['collection'] is None:
            logger.warn(f"Missing collection: {ref['sc_uid']}")
            continue

        if ref['pts_ref']:
            edition = ref['pts_edition']

            if edition == 'feer':
                edition = "1st ed. Feer (1884)"
            elif edition == 'somaratne':
                edition = "2nd ed. Somaratne (1998)"

            item = Am.MultiRef(
                collection = ref['collection'],
                ref_type = "pts",
                ref = normalize_sutta_ref(ref['pts_ref']),
                sutta_uid = ref['sc_uid'],
                edition = edition,
            )
            multi_refs.append(item)

    try:
        for ref in multi_refs:
            appdata_db.add(ref)
        appdata_db.commit()
    except Exception as e:
        logger.error(e)
        sys.exit(1)

    try:
        all_refs = appdata_db.query(Am.MultiRef).all()

        for ref in all_refs:
            if not ref.sutta_uid:
                logger.warn(f"ref.sutta_uid is None, multi_refs.id {ref.id}")
                continue

            sutta_range = sutta_range_from_ref(ref.sutta_uid)
            if not sutta_range:
                logger.error(f"Can't determine sutta range: {ref.sutta_uid}")
                continue

            suttas: List[Am.Sutta] = []

            if sutta_range['start'] is None:
                print("one")
                suttas = appdata_db.query(Am.Sutta) \
                    .filter(Am.Sutta.uid.like(f"{ref.sutta_uid}/%")) \
                    .all()

            else:
                print("two")
                suttas = appdata_db \
                    .query(Am.Sutta) \
                    .filter(or_(
                        Am.Sutta.uid.like(f"{ref.sutta_uid}/%"),
                        and_(Am.Sutta.sutta_range_group == sutta_range['group'],
                            Am.Sutta.sutta_range_start <= sutta_range['start'],
                            Am.Sutta.sutta_range_end >= sutta_range['end']),
                    )) \
                    .all()

            if len(suttas) == 0:
                logger.warn(f"No sutta for ref.sutta_uid {ref.sutta_uid}")
                continue

            ref.suttas = suttas

        appdata_db.commit()

    except Exception as e:
        logger.error(e)
        sys.exit(1)

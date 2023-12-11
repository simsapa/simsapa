"""A few helpful lists and functions for the exporter."""

from typing import Dict, List, Set
from datetime import date

from sqlalchemy.orm import Session

from simsapa.app.db.dpd_models import PaliWord

from simsapa.app.db_session import get_dpd_db_session

TODAY = date.today()

EXCLUDE_FROM_SETS: set = {
    "dps", "ncped", "pass1", "sandhi"}

EXCLUDE_FROM_FREQ: set = {
    "abbrev", "cs", "idiom", "letter", "prefix", "root", "suffix", "ve"}

# _cached_cf_set: Optional[Set[str]] = None

def cf_set_gen() -> Set[str]:
    """generate a list of all compounds families"""
    global _cached_cf_set

    # if _cached_cf_set is not None:
    #     return _cached_cf_set

    db_eng, db_conn, db_session = get_dpd_db_session()
    cf_db = db_session.query(
        PaliWord
    ).filter(PaliWord.family_compound != ""
             ).all()

    cf_set: Set[str] = set()
    for i in cf_db:
        if i.family_compound is None:
            continue
        cfs: List[str] = i.family_compound.split(" ")
        for cf in cfs:
            cf_set.add(cf)

    db_conn.close()
    db_session.close()
    db_eng.dispose()

    # _cached_cf_set = cf_set
    return cf_set

def make_roots_count_dict(db_session: Session) -> Dict[str, int]:
    roots_db = db_session.query(PaliWord).all()
    roots_count_dict: Dict[str, int] = dict()
    for i in roots_db:
        if i.root_key is None:
            continue
        if i.root_key in roots_count_dict:
            roots_count_dict[i.root_key] += 1
        else:
            roots_count_dict[i.root_key] = 1

    return roots_count_dict

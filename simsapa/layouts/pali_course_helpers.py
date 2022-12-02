from typing import List

from sqlalchemy.orm.session import Session
from sqlalchemy.sql.elements import and_, or_

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

from simsapa import DbSchemaName
from simsapa.app.types import UChallengeCourse, UChallengeGroup, UChallenge


def get_groups_in_course(db_session: Session, course: UChallengeCourse) -> List[UChallengeGroup]:
    if course.metadata.schema == DbSchemaName.AppData.value:
        res = db_session \
            .query(Am.ChallengeGroup) \
            .filter(Am.ChallengeGroup.course_id == course.id) \
            .order_by(Am.ChallengeGroup.sort_index.asc()) \
            .all()

    else:
        res = db_session \
            .query(Um.ChallengeGroup) \
            .filter(Um.ChallengeGroup.course_id == course.id) \
            .order_by(Um.ChallengeGroup.sort_index.asc()) \
            .all()

    if res is None:
        return []
    else:
        return res


def get_remaining_challenges_in_group(db_session: Session, group: UChallengeGroup) -> List[UChallenge]:
    if group.metadata.schema == DbSchemaName.AppData.value:
        res = db_session \
            .query(Am.Challenge) \
            .filter(and_(
                Am.Challenge.group_id == group.id,
                or_(Am.Challenge.level.is_(None),
                    Am.Challenge.level == 0)
            )) \
            .order_by(Am.Challenge.sort_index.asc()) \
            .all()

    else:
        res = db_session \
            .query(Um.Challenge) \
            .filter(and_(
                Um.Challenge.group_id == group.id,
                or_(Um.Challenge.level.is_(None),
                    Um.Challenge.level == 0)
            )) \
            .order_by(Um.Challenge.sort_index.asc()) \
            .all()

    if res is None:
        return []
    else:
        return res

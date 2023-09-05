from typing import List

from sqlalchemy.orm.session import Session
from sqlalchemy import and_, or_

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

from simsapa import DbSchemaName
from simsapa.app.types import UChallengeCourse, UChallengeGroup, UChallenge
from simsapa.app.app_data import AppData

from simsapa.layouts.gui_types import PaliGroupStats


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


def count_remaining_challenges_in_group(app_data: AppData, group: UChallengeGroup) -> int:
    schema = group.metadata.schema

    if group.id in app_data.pali_groups_stats[schema].keys():

        total = app_data.pali_groups_stats[schema][group.id]['total']
        completed = app_data.pali_groups_stats[schema][group.id]['completed']

        return total - completed

    else:
        total = len(group.challenges) # type: ignore
        remaining = len(get_remaining_challenges_in_group(app_data, group))
        completed = total - remaining

        app_data.pali_groups_stats[schema][group.id] = PaliGroupStats(
            total = total,
            completed = completed,
        )

        app_data._save_pali_groups_stats(schema)

        return remaining


def get_remaining_challenges_in_group(app_data: AppData, group: UChallengeGroup) -> List[UChallenge]:
    if group.metadata.schema == DbSchemaName.AppData.value:
        res = app_data.db_session \
            .query(Am.Challenge) \
            .filter(and_(
                Am.Challenge.group_id == group.id,
                or_(Am.Challenge.level.is_(None),
                    Am.Challenge.level == 0)
            )) \
            .order_by(Am.Challenge.sort_index.asc()) \
            .all()

    else:
        res = app_data.db_session \
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

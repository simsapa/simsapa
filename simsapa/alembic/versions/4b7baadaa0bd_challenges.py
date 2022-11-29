"""challenges

Revision ID: 4b7baadaa0bd
Revises: 92e01c50ebc0
Create Date: 2022-11-26 19:38:24.308082

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = '4b7baadaa0bd'
down_revision = '92e01c50ebc0'
branch_labels = None
depends_on = None


def upgrade():
    op.create_table('challenge_courses',
    sa.Column('id', sa.Integer(), nullable=False),
    sa.Column('name', sa.String(), nullable=False),
    sa.Column('description', sa.String(), nullable=True),
    sa.Column('sort_index', sa.Integer(), nullable=False),
    sa.PrimaryKeyConstraint('id'),
    sa.UniqueConstraint('name'),
    )

    op.create_table('challenge_groups',
    sa.Column('id', sa.Integer(), nullable=False),
    sa.Column('course_id', sa.Integer(), nullable=True),
    sa.Column('name', sa.String(), nullable=False),
    sa.Column('description', sa.String(), nullable=True),
    sa.Column('sort_index', sa.Integer(), nullable=False),
    sa.ForeignKeyConstraint(['course_id'], ['challenge_courses.id'], ondelete='CASCADE'),
    sa.PrimaryKeyConstraint('id'),
    sa.UniqueConstraint('name'),
    )

    op.create_table('challenges',
    sa.Column('id', sa.Integer(), nullable=False),
    sa.Column('group_id', sa.Integer(), nullable=True),
    sa.Column('challenge_type', sa.String(), nullable=False),
    sa.Column('sort_index', sa.Integer(), nullable=False),
    sa.Column('explanation_md', sa.String(), nullable=True),
    sa.Column('question_json', sa.String(), nullable=True),
    sa.Column('answers_json', sa.String(), nullable=True),
    sa.Column('level', sa.Integer(), nullable=True),
    sa.Column('studied_at', sa.DateTime(timezone=True), nullable=True),
    sa.Column('due_at', sa.DateTime(timezone=True), nullable=True),
    sa.Column('anki_model_name', sa.String(), nullable=True),
    sa.Column('anki_note_id', sa.Integer(), nullable=True),
    sa.Column('anki_synced_at', sa.DateTime(), nullable=True),
    sa.Column('created_at', sa.DateTime(timezone=True), server_default=sa.text('(CURRENT_TIMESTAMP)'), nullable=True),
    sa.Column('updated_at', sa.DateTime(timezone=True), nullable=True),
    sa.ForeignKeyConstraint(['group_id'], ['challenge_groups.id'], ondelete='NO ACTION'),
    sa.PrimaryKeyConstraint('id'),
    )


def downgrade():
    op.drop_table('challenges')
    op.drop_table('challenge_groups')
    op.drop_table('challenge_courses')

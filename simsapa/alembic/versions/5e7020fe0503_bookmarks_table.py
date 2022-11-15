"""bookmarks table

Revision ID: 5e7020fe0503
Revises: 5aaa36ccfa37
Create Date: 2022-11-15 08:39:57.147932

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = '5e7020fe0503'
down_revision = '5aaa36ccfa37'
branch_labels = None
depends_on = None


def upgrade():
    op.create_table('bookmarks',
    sa.Column('id', sa.Integer(), nullable=False),
    sa.Column('sutta_id', sa.Integer(), nullable=False),
    sa.Column('name', sa.String(), nullable=True),
    sa.Column('quote', sa.String(), nullable=True),
    sa.Column('created_at', sa.DateTime(timezone=True), server_default=sa.text('(CURRENT_TIMESTAMP)'), nullable=True),
    sa.Column('updated_at', sa.DateTime(timezone=True), nullable=True),
    sa.ForeignKeyConstraint(['sutta_id'], ['suttas.id'], ondelete='CASCADE'),
    sa.PrimaryKeyConstraint('id'),
    )


def downgrade():
    op.drop_table('bookmarks', schema='userdata')

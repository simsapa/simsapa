"""Add page_nodes table

Revision ID: 7f62ef55eda9
Revises: d86819659876
Create Date: 2021-11-18 09:43:17.740545

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = '7f62ef55eda9'
down_revision = 'd86819659876'
branch_labels = None
depends_on = None


def upgrade():
    op.create_table('page_nodes',
    sa.Column('id', sa.Integer(), nullable=False),
    sa.Column('uid', sa.String(), nullable=True),
    sa.Column('name', sa.String(), nullable=True),
    sa.Column('created_at', sa.DateTime(), nullable=True),
    sa.Column('updated_at', sa.DateTime(), nullable=True),
    sa.PrimaryKeyConstraint('id'))

def downgrade():
    op.drop_table('page_nodes')

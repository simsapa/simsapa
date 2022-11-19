"""sutta content_json

Revision ID: 92e01c50ebc0
Revises: 5e7020fe0503
Create Date: 2022-11-19 06:35:05.529133

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = '92e01c50ebc0'
down_revision = '5e7020fe0503'
branch_labels = None
depends_on = None


def upgrade():
    op.add_column('suttas', sa.Column('content_json', sa.String(), nullable=True))
    op.add_column('suttas', sa.Column('source_uid', sa.String(), nullable=True))

def downgrade():
    op.drop_column('suttas', 'content_json')
    op.drop_column('suttas', 'source_uid')

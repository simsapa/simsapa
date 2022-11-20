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
    op.add_column('suttas', sa.Column('content_json_tmpl', sa.String(), nullable=True))
    op.add_column('suttas', sa.Column('source_uid', sa.String(), nullable=True))

    op.create_table('sutta_comments',
    sa.Column('id', sa.Integer(), nullable=False),
    sa.Column('sutta_id', sa.Integer(), nullable=False),
    sa.Column('sutta_uid', sa.String(), nullable=False),
    sa.Column('language', sa.String(), nullable=True),
    sa.Column('source_uid', sa.String(), nullable=True),
    sa.Column('content_json', sa.String(), nullable=True),
    sa.Column('created_at', sa.DateTime(timezone=True), server_default=sa.text('(CURRENT_TIMESTAMP)'), nullable=True),
    sa.Column('updated_at', sa.DateTime(timezone=True), nullable=True),
    sa.ForeignKeyConstraint(['sutta_id'], ['suttas.id'], ondelete='CASCADE'),
    sa.PrimaryKeyConstraint('id'),
    )

    op.create_table('sutta_variants',
    sa.Column('id', sa.Integer(), nullable=False),
    sa.Column('sutta_id', sa.Integer(), nullable=False),
    sa.Column('sutta_uid', sa.String(), nullable=False),
    sa.Column('language', sa.String(), nullable=True),
    sa.Column('source_uid', sa.String(), nullable=True),
    sa.Column('content_json', sa.String(), nullable=True),
    sa.Column('created_at', sa.DateTime(timezone=True), server_default=sa.text('(CURRENT_TIMESTAMP)'), nullable=True),
    sa.Column('updated_at', sa.DateTime(timezone=True), nullable=True),
    sa.ForeignKeyConstraint(['sutta_id'], ['suttas.id'], ondelete='CASCADE'),
    sa.PrimaryKeyConstraint('id'),
    )

def downgrade():
    op.drop_column('suttas', 'content_json')
    op.drop_column('suttas', 'content_json_tmpl')
    op.drop_column('suttas', 'source_uid')

    op.drop_table('sutta_comments')
    op.drop_table('sutta_variants')

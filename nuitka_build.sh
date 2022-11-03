#!/usr/bin/env bash

python -m nuitka \
  --output-dir=dist_nuitka \
  --standalone \
  --follow-imports \
  --enable-plugin=numpy \
  --include-package-data="bokeh" \
  --include-module="sqlalchemy.sql.default_comparator" \
  --windows-icon-from-ico="simsapa/assets/icons/appicons/simsapa.ico" \
  --include-data-dir="simsapa/assets=simsapa/assets" \
  --include-data-dir="simsapa/alembic=simsapa/alembic" \
  --include-data-files="simsapa/alembic.ini=simsapa/alembic.ini" \
  run.py

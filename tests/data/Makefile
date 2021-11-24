all:
	@echo "No default target."

bootstrap_test_db: db_create_and_seed db_reindex_fts

db_create_and_seed:
	rm -rf assets && mkdir assets && \
	cp -r ./seed_data/docs ./assets && \
	python3 ./create_schema.py userdata ./assets/userdata.sqlite3 && \
	python3 ./create_schema.py appdata ./assets/appdata.sqlite3 && \
	sqlite3 ./assets/appdata.sqlite3 < ./fts_indexes.sql && \
	sqlite3 ./assets/userdata.sqlite3 < ./fts_indexes.sql && \
	sqlite3 ./assets/appdata.sqlite3 < ./seed_data/appdata-populate.sql && \
	sqlite3 ./assets/userdata.sqlite3 < ./seed_data/userdata-populate.sql

db_reindex_fts:
	sqlite3 ./assets/appdata.sqlite3 < ./reindex_fts.sql && \
	sqlite3 ./assets/userdata.sqlite3 < ./reindex_fts.sql

export_from_db:
	sqlite3 ./assets/appdata.sqlite3 .dump | grep -E '^INSERT INTO' | grep -vE 'alembic_version|fts_suttas|sqlite_sequence' > ./seed_data/appdata-populate.sql && \
	sqlite3 ./assets/userdata.sqlite3 .dump | grep -E '^INSERT INTO' | grep -vE 'alembic_version|fts_suttas|sqlite_sequence' > ./seed_data/userdata-populate.sql

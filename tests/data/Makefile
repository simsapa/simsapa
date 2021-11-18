all:
	@echo "No default target."

bootstrap_test_db: db_create_and_seed db_reindex_fts

db_create_and_seed:
	rm -rf assets && mkdir assets && \
	cd assets && \
	cp -r ../seed_data/docs ./ && \
	sqlite3 appdata.sqlite3 < ../../../simsapa/migrations/create_schema.sql && \
	sqlite3 userdata.sqlite3 < ../../../simsapa/migrations/create_schema.sql && \
	sqlite3 appdata.sqlite3 < ../../../simsapa/migrations/fts_indexes.sql && \
	sqlite3 userdata.sqlite3 < ../../../simsapa/migrations/fts_indexes.sql && \
	sqlite3 appdata.sqlite3 < ../seed_data/appdata-populate.sql && \
	sqlite3 userdata.sqlite3 < ../seed_data/userdata-populate.sql

db_reindex_fts:
	cd assets && \
	sqlite3 appdata.sqlite3 < ../../../simsapa/migrations/reindex_fts.sql && \
	sqlite3 userdata.sqlite3 < ../../../simsapa/migrations/reindex_fts.sql

export_from_db:
	cd assets && \
	sqlite3 appdata.sqlite3 .dump | grep -E '^INSERT INTO' | grep -vE 'fts_suttas|sqlite_sequence' > ../seed_data/appdata-populate.sql && \
	sqlite3 userdata.sqlite3 .dump | grep -E '^INSERT INTO' | grep -vE 'fts_suttas|sqlite_sequence' > ../seed_data/userdata-populate.sql

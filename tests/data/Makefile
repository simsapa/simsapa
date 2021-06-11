all:
	@echo "No default target."

create_and_seed_test_db:
	rm -f appdata.sqlite3 userdata.sqlite3
	sqlite3 appdata.sqlite3 < ../../simsapa/migrations/create_schema.sql
	sqlite3 userdata.sqlite3 < ../../simsapa/migrations/create_schema.sql
	sqlite3 appdata.sqlite3 < ./seed_data/appdata-populate.sql
	sqlite3 userdata.sqlite3 < ./seed_data/userdata-populate.sql

export_from_db:
	sqlite3 appdata.sqlite3 .dump | grep -E '^INSERT INTO' | grep -vE 'schema_version|sqlite_sequence' > seed_data/appdata-populate.sql
	sqlite3 userdata.sqlite3 .dump | grep -E '^INSERT INTO' | grep -vE 'schema_version|sqlite_sequence' > seed_data/userdata-populate.sql

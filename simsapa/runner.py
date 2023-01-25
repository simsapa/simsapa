import os
import sys
from typing import Optional
import typer

from simsapa import logger
from simsapa.app.types import QueryType
from simsapa.app.helpers import create_app_dirs

# Error in click.utils.echo() when console is unavailable
# https://github.com/pallets/click/issues/2415
if getattr(sys, 'frozen', False):
    f = open(os.devnull, 'w')
    sys.stdin = f
    sys.stdout = f

app = typer.Typer()
index_app = typer.Typer()
app.add_typer(index_app, name="index")

create_app_dirs()


@app.command()
def gui(url: Optional[str] = None):
    # import subprocess
    # from simsapa import SIMSAPA_PACKAGE_DIR
    # try:
    #     proc = subprocess.Popen(['python3', SIMSAPA_PACKAGE_DIR.joinpath('splash.py')])
    # except Exception as e:
    #     print(str(e))
    #     sys.exit(2)

    from simsapa.gui import start
    start(url=url)

@app.command()
def query(query_type: QueryType, query: str, print_titles: bool = True, print_count: bool = False):
    """Query the database."""

    from simsapa.app.db import search
    from simsapa.app.types import AppData

    app_data = AppData()

    if query_type == QueryType.suttas:
        search_query = search.SearchQuery(
            app_data.search_indexed.suttas_index,
            20,
            search.sutta_hit_to_search_result,
        )
    elif query_type == QueryType.words:
        search_query = search.SearchQuery(
            app_data.search_indexed.dict_words_index,
            20,
            search.dict_word_hit_to_search_result,
        )
    else:
        print("Unrecognized query type.")
        return

    search_query.new_query(query)

    if print_count:
        print(f"Results count: {search_query.hits}")

    if print_titles:
        for i in search_query.get_all_results(highlight=False):
            print(i['title'])

@index_app.command("create")
def index_create():
    """Create database indexes, removing existing ones."""
    from simsapa.app.db.search import SearchIndexed
    search_indexed = SearchIndexed()
    search_indexed.create_all()

@index_app.command("reindex")
def index_reindex():
    """Clear and rebuild database indexes."""
    from simsapa.app.db.search import SearchIndexed
    from simsapa.app.types import AppData
    search_indexed = SearchIndexed()
    app_data = AppData()

    search_indexed.create_all()
    search_indexed.index_all(app_data.db_session)

@index_app.command("suttas-lang")
def index_suttas_lang(lang: str):
    """Create a separate index and index suttas from appdata of the given language."""
    from simsapa.app.db.search import SearchIndexed
    from simsapa.app.types import AppData
    search_indexed = SearchIndexed()
    app_data = AppData()

    search_indexed.index_all_suttas_lang(app_data.db_session, lang)

@app.command("import-bookmarks")
def import_bookmarks(path_to_csv: str):
    """Import bookmarks from a CSV file (such as an earlier export)"""
    from simsapa.app.types import AppData
    app_data = AppData()
    bookmarks = app_data.import_bookmarks(path_to_csv)
    print(f"Imported {bookmarks} bookmarks.")

@app.command("import-suttas-to-userdata")
def import_suttas_to_userdata(path_to_db: str):
    """Import suttas from an sqlite3 db to userdata."""
    from simsapa.app.types import AppData
    app_data = AppData()
    suttas = app_data.import_suttas_to_userdata(path_to_db)
    print(f"Imported {suttas} suttas.")

@app.command("export-bookmarks")
def export_bookmarks(path_to_csv: str):
    """Export bookmarks to a CSV file"""
    from simsapa.app.types import AppData
    app_data = AppData()
    bookmarks = app_data.export_bookmarks(path_to_csv)
    print(f"Exported {bookmarks} bookmarks.")

@app.command("import-pali-course")
def import_pali_course(path_to_toml: str):
    """Import a Pali Cource from a TOML file"""
    from simsapa.app.types import AppData
    app_data = AppData()
    try:
        name = app_data.import_pali_course(path_to_toml)
    except Exception as e:
        print(e)
        return

    print(f"Imported Pali Course: {name}")

def main():
    s = os.getenv('START_NEW_LOG')
    if s is not None and s.lower() == 'false':
       start_new = False
    else:
        start_new = True

    logger.info("runner::main()", start_new = start_new)

    if len(sys.argv) == 1:
        gui()
    elif len(sys.argv) == 2:
        s = sys.argv[1]

        if s.startswith("ssp://"):
            gui(s)

        else:
            app()
    else:
        app()

if __name__ == "__main__":
    main()

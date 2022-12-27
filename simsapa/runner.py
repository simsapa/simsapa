import sys
from typing import Optional
import typer

from multiprocessing import freeze_support

from simsapa import logger, IS_WINDOWS
from simsapa.app.types import QueryType

app = typer.Typer()
index_app = typer.Typer()
app.add_typer(index_app, name="index")

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

def main():
    logger.info("runner::main()", start_new=True)

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
    if IS_WINDOWS:
        freeze_support()
    main()

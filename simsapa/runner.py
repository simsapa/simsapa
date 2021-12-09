import sys
from enum import Enum
import typer

app = typer.Typer()
index_app = typer.Typer()
app.add_typer(index_app, name="index")

@app.command()
def gui():
    from simsapa.gui import start
    start()

class QueryType(str, Enum):
    suttas = "suttas"
    dict_words = "dict_words"

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
    elif query_type == QueryType.dict_words:
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
        for i in search_query.all_results(highlight=False):
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
    if len(sys.argv) == 1:
        gui()
    else:
        app()

if __name__ == "__main__":
    main()

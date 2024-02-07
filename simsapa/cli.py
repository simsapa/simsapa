from typing import Optional
import typer

from simsapa import SIMSAPA_API_DEFAULT_PORT, logger, QueryType
from simsapa.app.types import SearchMode, SearchParams

app = typer.Typer()
index_app = typer.Typer()
app.add_typer(index_app, name="index")

@app.command()
def gui(url: Optional[str] = None,
        window_type: Optional[str] = None,
        show_window: bool = True,
        tray_icon: bool = True,
        port: int = SIMSAPA_API_DEFAULT_PORT):
    """Start a GUI window."""
    logger.profile("runner::gui()")

    from simsapa.gui import start
    start(port=port, url=url, window_type_name=window_type, show_window=show_window, enable_tray_icon=tray_icon)

@app.command()
def query(query_type: QueryType, query: str, print_titles: bool = True, print_count: bool = False):
    """Query the database."""

    from simsapa.app.search.tantivy_index import TantivySearchQuery, TantivySearchIndexes
    from simsapa.app.db_session import get_db_engine_connection_session

    _, _, db_session = get_db_engine_connection_session()

    search_indexes = TantivySearchIndexes(db_session)

    params = SearchParams(
        mode = SearchMode.FulltextMatch,
        page_len = 20,
        lang = None,
        lang_include = True,
        source = None,
        source_include = True,
        enable_regex = False,
        fuzzy_distance = 0,
    )

    if query_type == QueryType.suttas:

        for lang, ix in search_indexes.suttas_lang_index.items():

            p = params
            p['lang'] = lang

            search_query = TantivySearchQuery(ix, p)
            search_query.new_query(query)

            if print_count:
                print(f"Results count: {search_query.get_hits_count()}")

            if print_titles:
                for i in search_query.get_all_results():
                    print(i['title'])

            logger.profile(f"Results printed for {lang}")

    elif query_type == QueryType.words:

        for _, ix in search_indexes.dict_words_lang_index.items():
            search_query = TantivySearchQuery(ix, params)
            search_query.new_query(query)

            if print_count:
                print(f"Results count: {search_query.hits_count}")

            if print_titles:
                for i in search_query.get_all_results():
                    print(i['title'])

    else:
        print("Unrecognized query type.")
        return

@index_app.command("create")
def index_create():
    """Create database indexes, removing existing ones."""
    from simsapa.app.search.tantivy_index import TantivySearchIndexes
    from simsapa.app.db_session import get_db_engine_connection_session
    _, _, db_session = get_db_engine_connection_session()
    search_indexes = TantivySearchIndexes(db_session, remove_if_exists=True)
    print(f"Has emtpy index: {search_indexes.has_empty_index()}")

@index_app.command("reindex")
def index_reindex():
    """Clear and rebuild database indexes."""
    from simsapa.app.search.tantivy_index import TantivySearchIndexes
    from simsapa.app.db_session import get_db_engine_connection_session
    _, _, db_session = get_db_engine_connection_session()
    search_indexes = TantivySearchIndexes(db_session, remove_if_exists=True)
    search_indexes.index_all()

@index_app.command("suttas-lang")
def index_suttas_lang(lang: str):
    """Index suttas from appdata of the given language."""
    from simsapa.app.search.tantivy_index import TantivySearchIndexes
    from simsapa.app.db_session import get_db_engine_connection_session
    _, _, db_session = get_db_engine_connection_session()
    search_indexes = TantivySearchIndexes(db_session)
    search_indexes.index_all_suttas_lang(lang)

@index_app.command("dict-words-lang")
def index_dict_words_lang(lang: str):
    """Index dict_words from appdata of the given language."""
    from simsapa.app.search.tantivy_index import TantivySearchIndexes
    from simsapa.app.db_session import get_db_engine_connection_session
    _, _, db_session = get_db_engine_connection_session()
    search_indexes = TantivySearchIndexes(db_session)
    search_indexes.index_all_dict_words_lang(lang)

@app.command("import-bookmarks")
def import_bookmarks(path_to_csv: str):
    """Import bookmarks from a CSV file (such as an earlier export)"""
    from simsapa.app.app_data import AppData
    app_data = AppData()
    bookmarks = app_data.import_bookmarks(path_to_csv)
    print(f"Imported {bookmarks} bookmarks.")

@app.command("import-suttas-to-userdata")
def import_suttas_to_userdata(path_to_db: str):
    """Import suttas from an sqlite3 db to userdata."""
    from simsapa.app.app_data import AppData
    app_data = AppData()
    suttas = app_data.import_suttas_to_userdata(path_to_db)
    print(f"Imported {suttas} suttas.")

@app.command("export-prompts")
def export_prompts(path_to_csv: str):
    """Export prompts to a CSV file"""
    from simsapa.app.app_data import AppData
    app_data = AppData()
    prompts = app_data.export_prompts(path_to_csv)
    print(f"Exported {prompts} prompts.")

@app.command("import-prompts")
def import_prompts(path_to_csv: str):
    """Import prompts from a CSV file (such as an earlier export)"""
    from simsapa.app.app_data import AppData
    app_data = AppData()
    prompts = app_data.import_prompts(path_to_csv)
    print(f"Imported {prompts} prompts.")

@app.command("export-bookmarks")
def export_bookmarks(path_to_csv: str):
    """Export bookmarks to a CSV file"""
    from simsapa.app.app_data import AppData
    app_data = AppData()
    bookmarks = app_data.export_bookmarks(path_to_csv)
    print(f"Exported {bookmarks} bookmarks.")

@app.command("import-pali-course")
def import_pali_course(path_to_toml: str):
    """Import a Pali Cource from a TOML file"""
    from simsapa.app.app_data import AppData
    app_data = AppData()
    try:
        name = app_data.import_pali_course(path_to_toml)
    except Exception as e:
        print(e)
        return

    print(f"Imported Pali Course: {name}")

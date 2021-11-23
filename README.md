# Simsapa Dhamma Reader

See further notes in the [docs](./docs/index.md) folder.

## Install

``` shell
pip install simsapa
```

## Development

Install Poetry, clone this repo and run `poetry install` to install dependencies.

In the project root, enter a venv with poetry and start the app with:

``` shell
poetry shell
./run.py
```

### .env

Environment variables can be set with a `.env` file is in the project root.

Recognized settings:

```
USE_TEST_DATA=true
```

Instead of connecting to database in the user's folders, connect to the test
database found in `tests/data/assets/`

```
ENABLE_WIP_FEATURES=false
```

Whether to enable work-in-progress feature which may be unstable or broken.

### Editing application windows with Qt Designer

Install Qt Designer and dependencies, in a shell **outside** the project venv.

The project PyQt5 package dependencies sometimes can't be resolved with `pyqt5-tools`. 

``` shell
pip3 install pyqt5 pyqt5-tools
sudo apt-get install python3-pyqt5.qtquick libqt5designer5 libqt5designercomponents5
```

Open the `.ui` file in Qt Designer, in or out- of the project venv.

``` shell
pyqt5-tools designer ./simsapa/assets/ui/dictionary_search_window.ui
```

After saving the `.ui`, re-generate the `.ui.py` files:

``` shell
make ui
```

## Tests

All tests:

``` shell
make tests -B
```

A single test:

``` shell
pytest -k test_dict_word_dictionary tests/test_dictionary.py
```


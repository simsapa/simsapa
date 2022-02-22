# Simsapa Dhamma Reader

## Install

Download the [latest build](https://www.dropbox.com/sh/j26b8sb955hj5x6/AABh0SCsRA7bY1bTGhKA5x1Pa?dl=0) for Linux (.AppImage), Mac (.dmg) or Windows (.msi).

On the first run, it downloads the application database and fulltext index.

Short feature demo: <https://vimeo.com/677880749>

![Sutta Search](docs/sutta-search-screenshot.jpg)

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

Install Qt Designer and dependencies from the distro package manager (not `pip`).

``` shell
sudo apt-get install qttools5-dev-tools python3-pyqt5 python3-pyqt5.qtquick libqt5designer5 libqt5designercomponents5
```

Open the `.ui` file in Qt Designer, in or out- of the project venv.

``` shell
designer ./simsapa/assets/ui/dictionary_search_window.ui
```

After saving the `.ui`, re-generate the `.py` files. The Makefile target calls `pyuic5 `.

``` shell
make ui
```

Don't use the pip pacakges frequently recommended in tutorials (`pip install
pyqt5 pyqt5-tools`), these are often compiled at different Qt versions, and may
result in Qt Designer crashing with the following error:

```
...Qt/bin/designer: symbol lookup error: ...Qt/bin/designer: undefined symbol: _ZdlPvm, version Qt_5
```

## Tests

All tests:

``` shell
make tests -B
```

A single test:

``` shell
USE_TEST_DATA=true pytest -k test_dict_word_dictionary tests/test_dictionary.py
```


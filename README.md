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

## Tests

All tests:

``` shell
pytest
```

A single test:

``` shell
pytest -k test_dict_word_dictionary tests/test_dictionary.py
```


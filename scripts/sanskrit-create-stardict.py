#!/usr/bin/env python3

import os
import sys
import glob
from pathlib import Path
from dotenv import load_dotenv

from simsapa.app.stardict import export_words_as_stardict_zip, ifo_from_opts, DictEntry
from simsapa import logger

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

bootstrap_assets_dir = Path(s)

STARDICT_ZIP_PATH = bootstrap_assets_dir.joinpath("dict/MW.zip")
WORDS_DIR = bootstrap_assets_dir.joinpath("sanskrit/words-html/")

def main():
    p = WORDS_DIR.joinpath('*.html')
    files = glob.glob(f"{p}")

    def file_to_word(p):
        p = Path(p)

        with open(p, 'r', encoding = 'utf-8') as f:
            html = f.read()

        html = f"<div class=\"cologne\">{html}</div>"

        return DictEntry(
            word = p.stem,
            definition_html = html,
            definition_plain = '',
            synonyms = [],
        )

    words = list(map(file_to_word, files))

    ifo = ifo_from_opts(
        {
            "bookname": "Monier-Williams Sanskrit-English Dictionary, 1899",
            "author": "Cologne Sanskrit Lexicon",
            "description": "Sanskrit-English Dictionary",
            "website": "https://www.sanskrit-lexicon.uni-koeln.de/scans/MWScan/2020/web/index.php",
        }
    )

    export_words_as_stardict_zip(words, ifo, STARDICT_ZIP_PATH)

if __name__ == "__main__":
    main()

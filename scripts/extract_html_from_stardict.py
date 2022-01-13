#!/usr/bin/env python3

from pathlib import Path
from typing import List

from simsapa.app.stardict import DictEntry, parse_stardict_zip, stardict_to_dict_entries

def main():
    stardict_zip_path = Path("./dpd.zip")
    output_path = Path("./words")

    output_path.mkdir(parents=True, exist_ok=True)

    paths = parse_stardict_zip(stardict_zip_path)
    words: List[DictEntry] = stardict_to_dict_entries(paths)

    for w in words[0:5]:
        p = output_path.joinpath(w['word'] + ".html")
        with open(p, 'w') as f:
            f.write(w['definition_html'])

if __name__ == "__main__":
    main()

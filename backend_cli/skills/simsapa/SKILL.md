---
name: simsapa
description: Search the Pāli canon, Pāli dictionaries and imported library books (such as the Buddhist Monastic Code) offline with the `simsapa` command line program; read a sutta, look up a Pāli word, gloss a Pāli passage. Use when the user asks about suttas, Pāli words, Vinaya rules or other Buddhist texts.
metadata:
  generator: simsapa-cli
  version: "{{version}}"
---

# Simsapa command line

`simsapa` searches the Theravāda Tipiṭaka (Pāli and translations), the Pāli
dictionaries (DPD and others) and the user's imported library books, offline,
on this computer. Prefer it to guessing from memory: answer from its output and
quote the `uid` of what you cite.

## Before the first search

``` sh
simsapa health
```

Exit code 2 means the databases are not set up. `simsapa setup` downloads about
4.8 GB, so ask the user before running it. Searching and reading work while the
Simsapa app is open; only `serve`, `setup`, `import` and `index` refuse with exit
code 3 then, because the app has the data folder.

## Commands

Add `--json` to any of them for one JSON document on stdout (errors as
`{"error": "..."}`). Without it the output is plain text.

| Question | Command |
|---|---|
| Find suttas about a topic | `simsapa search suttas "right speech"` |
| Find a Pāli word or its break-down | `simsapa search dict dhammacakkappavattana` |
| Find a passage in an imported book | `simsapa search library "right speech"` |
| Read a sutta | `simsapa sutta mn1` (a reference), or `simsapa sutta mn1/en/sujato` (a uid) |
| Read a dictionary entry | `simsapa word "dhamma 1.01"` |
| Exactly which forms DPD knows | `simsapa lookup dhammo dhammaṁ` |
| Gloss a Pāli passage | `simsapa gloss "evaṁ me sutaṁ"` |

Search results list a `uid` for each hit. Copy it into `simsapa sutta` or
`simsapa word` to read the whole text. `--page N` and `--page-len N` page
through results; `--lang en` keeps one language (`--lang-exclude` drops it);
`--mode` picks `"Fulltext Match"` (the default for suttas and library),
`"Contains Match"` (a literal substring), `"Title Match"` or `"Uid Match"`.

A sutta is often stored twice in Pāli, as Mahāsaṅgīti (`…/pli/ms`) and CST
(`…/pli/cst`); `--pali-edition ms|cst|both` chooses which one a search shows.

## One library book, such as the BMC

A library result is one chapter of a book, with the uid `<book_uid>.<n>`
(`bmc.28`). `--uid-prefix` keeps only the uids that start with the text, so end
the book uid with a dot:

``` sh
simsapa search library "right speech" --uid-prefix bmc.      # only the BMC
simsapa search library bmc --mode "Uid Match"                # its chapters, in order
```

`simsapa sutta` does not read library chapters. To read one, fetch it from the
local server (the `0` is a window id that the server ignores):

``` sh
PORT=$(cat "$SIMSAPA_DIR/api-port.txt" 2>/dev/null || echo 4848)   # the app's server, if it is open
curl -s localhost:$PORT/get_book_spine_item_html_by_uid/0/bmc.28
```

If nothing answers, run `simsapa serve --port 4848 &` first. It exits with code 3
when the Simsapa app is open; use the app's own server then. `$SIMSAPA_DIR` is
the folder `simsapa paths` prints.

## Exit codes

0 success, 1 error, 2 databases not set up, 3 data folder in use, 4 unknown uid.
A search with no hits exits 0 and says `0 results`; that is an answer, not a
fault.

## The full reference

`simsapa docs` prints the complete reference for every command and for the
localhost API (about 3,000 lines of Markdown). Save it and search it rather than
reading it whole: `simsapa docs --output /tmp/simsapa-reference.md`.

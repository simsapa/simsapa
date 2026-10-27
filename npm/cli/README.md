# @simsapa/cli

`simsapa` is the command-line version of the
[Simsapa Dhamma Reader](https://github.com/simsapa/simsapa)'s data engine. With
it you can search the suttas and the Pāli dictionaries from the terminal, gloss
Pāli text, import your own dictionaries and books, and serve Simsapa's
localhost HTTP API, all without the desktop app.

It is built for agents as much as for people. Every command prints plain text by
default, or one JSON document with `--json`. The CLI has no AI provider
integration and needs no API keys. It supplies the data, and the calling agent
does the reasoning.

Supported platforms: Linux x64, Linux arm64, macOS on Apple Silicon, and
Windows x64 (Windows on ARM runs it under x64 emulation). The Linux binaries
are statically linked, so they run on any distribution, Alpine included.

## Install

```sh
npm install -g @simsapa/cli
simsapa setup
```

npm installs only the binary for your platform, as an optional dependency.
Nothing is downloaded at install time and there is no `postinstall` script. If
`simsapa` says its platform package is not installed, you installed with
`--omit=optional`. Reinstall without it.

## Setup: the databases

`simsapa setup` downloads the Simsapa databases (suttas, dictionaries, DPD and
the fulltext indexes). It lists the archives and the destination folder, then
asks before it starts. **They take about 4.8 GB on disk once extracted.**

```sh
simsapa setup                     # English and Pāli
simsapa setup --languages hu,pt   # also Hungarian and Portuguese suttas
simsapa setup --list-languages    # the languages available for download
simsapa setup --yes               # don't ask (needed when stdin is not a terminal)
```

Run `simsapa setup --languages de` again later to add a language to an existing
setup. If a download fails or you press Ctrl-C, setup removes what it
downloaded. Run it again to start over.

### Where the data goes

The CLI uses **the same data folder as the Simsapa desktop app**. On Linux this
is `~/.local/share/simsapa`. `simsapa paths` prints the folder and the database
paths on any platform, and works before setup.

- If you already have the desktop app, the CLI finds its databases. There is no
  need to download them twice.
- A folder made by `simsapa setup` opens in the desktop app.
- `--simsapa-dir <path>` (or the `SIMSAPA_DIR` environment variable) uses
  another folder. Use it for a Windows portable install, whose `config.txt` the
  npm-installed binary does not read.

### Close the desktop app first

`setup`, `serve`, `import` and `index build|rebuild|delete` change the data
folder. They refuse to run while the desktop app (or a `simsapa serve`) is using
the same folder, and exit with code 3. Close the app first, or use its API
directly at the address the message gives. The read-only commands (`search`,
`query`, `word`, `sutta`, `gloss`, `lookup`, `stats`, `paths`, `health`) work
while the app is open.

The desktop app does not check for the CLI, so don't open it while a `simsapa
import` is running.

## Commands

| Command | What it does |
|---|---|
| `simsapa setup` | Download the databases, or add sutta languages |
| `simsapa search suttas\|dict\|library <query>` | Search, as `POST /search` does (`--mode`, `--lang`, `--source`, `--page`, `--page-len`, `--all-snippets`) |
| `simsapa query suttas\|words <text>` | List the suttas or dictionary words containing a text |
| `simsapa fulltext-search <query>` | Search the fulltext index directly |
| `simsapa word <uid>` | A dictionary entry, e.g. `simsapa word "dhamma 1.01"` (`--html` for the rendered page) |
| `simsapa sutta <uid>` | A sutta as text, e.g. `simsapa sutta mn1/en/sujato` (`--html`, `--content-json`, `--bilara`) |
| `simsapa gloss <text\|->` | Dictionary options and compound break-downs for every word (`--exact` for exact DPD lookups) |
| `simsapa lookup <form…\|->` | Exact DPD lookup of word forms |
| `simsapa import stardict <dir>` / `stardict-zip <zip>` | Import a StarDict dictionary, searchable in the CLI and the desktop app |
| `simsapa import epub <file> --uid <uid>` / `html <file> --uid <uid>` | Import a book into the library |
| `simsapa index build\|rebuild\|delete` | Manage the fulltext search indexes |
| `simsapa stats` | Row and content-source counts of `appdata.sqlite3` |
| `simsapa paths` | The data folder and database paths |
| `simsapa health` | What `GET /health` returns, including the DPD and SuttaCentral data versions |
| `simsapa docs` | The full CLI and API reference (Markdown); `--output <file>` saves it |
| `simsapa serve` | Serve the localhost API |

`simsapa <command> --help` lists each command's options. `-v` and `-vv` show
more log output on stderr. The full log is `log.txt` in the data folder.

### `--json` output

With `--json`, a command prints exactly one JSON document on stdout, with the
same shape as the matching API route: `search` as `POST /search`, `word` as
`/word.json`, `gloss` as `/gloss_text`, `lookup` as `/lookup_forms`, `health` as
`/health`. Progress bars and log lines go to stderr, so stdout stays parseable.
An error is `{"error": "<message>"}` on stdout with a non-zero exit code.

```sh
simsapa search suttas "four noble truths" --json | jq '.results[0].uid'
```

### Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | error |
| 2 | the databases are not set up (run `simsapa setup`) |
| 3 | the data folder is in use by the Simsapa app or another `simsapa` command |
| 4 | not found (unknown uid) |
| 130 | interrupted |

## `simsapa serve`: the localhost API

Each CLI command opens the databases, which takes a few hundred milliseconds.
An agent making many queries should start the server once and call its HTTP API
instead:

```sh
simsapa serve                 # http://127.0.0.1:4848, or the next free port
simsapa serve --port 4850
```

It serves the desktop app's data routes (search, words, suttas, glossing, exact
lookups, `/health`) with the same requests and responses. The routes are
documented in
[the API reference](https://github.com/simsapa/simsapa/blob/main/docs/simsapa-cli-and-localhost-api.md).
Routes that drive the desktop app's windows, and its AI word-selection routes,
are not served. HTML pages are returned without their images, scripts and
fonts; the text and the reader CSS are in the page.

The server writes its port to `api-port.txt` in the data folder and stops on
Ctrl-C or `GET /shutdown`.

### `--host`: containers and other machines

By default the server listens on `127.0.0.1`, reachable only from the same
machine. Inside a Docker container, or to serve another computer on the
network, pass the address to listen on:

```sh
simsapa serve --host 0.0.0.0
```

**The API has no authentication.** With an address other than loopback,
anyone who can reach the machine on that port can use it, including the routes
that write to the data folder (`/save_sutta_display_settings`, `/logger`).
`simsapa serve` prints a warning saying so, and does not serve `/shutdown`, so
the server stops only on Ctrl-C. With a specific network address such as
`192.168.1.5`, other `simsapa` commands cannot see that the server is running,
so don't run the writing commands alongside it.

## Glossing Pāli text with an agent

`simsapa gloss` (or `POST /gloss_text` on the server) returns, for every word of
a passage, its dictionary options and the deconstructor's compound break-downs.
The agent chooses the sense of each word itself.

```sh
simsapa gloss "atthaññe sādhūti"
```

```
atthaññe  (pr) there is; there exists [√as + ti > atthi] pr  [atthi + aññe]
sādhūti   (adj) good; pleasant; auspicious [√sādh + u] adj  [sādhu + iti | sādhū + iti]
```

With `--json`, each word in `paragraphs[].words_data[]` carries:

- `results[]`: the dictionary options, each with `uid`, `word` and `summary`.
  Choose a sense by its `uid`.
- `direct_uids[]`: the options that match the word itself. When it is empty,
  the word is a compound found only through its break-downs.
- `deconstructions[]`: the break-downs, each with `words_joined` (`atthi +
  aññe`) and `components[]`, where every component has its `word` and the
  `result_uids` of its own senses. For a compound, choose a break-down and then
  a sense for each component.

```sh
simsapa gloss atthaññe --json | jq -c '.paragraphs[0].words_data[0]
  | {direct_uids, deconstructions: (.deconstructions
      | map({words_joined, components: (.components | map({word, result_uids}))}))}'
```

```json
{"direct_uids":[],"deconstructions":[{"words_joined":"atthi + aññe","components":[{"word":"atthi","result_uids":["2736/dpd","2737/dpd","2738/dpd","2757/dpd"]},{"word":"aññe","result_uids":["1497/dpd","1498/dpd","1601/dpd","1631/dpd","1660/dpd","74340/dpd","80675/dpd"]}]}]}
```

`simsapa word 2736/dpd --json` fetches the full entry of any uid. For exact
answers with no guessing, use `simsapa lookup <form…>` or `simsapa gloss
--exact`: a `not_found` there means DPD has no such form.

## License

Apache-2.0

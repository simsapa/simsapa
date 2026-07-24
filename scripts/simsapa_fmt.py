#!/usr/bin/env python3
"""Format Simsapa localhost-API responses for the agent.

This is a *formatter only*: it reads a response on stdin (piped from `curl`)
and prints a compact, grep-friendly summary. The request is done by `curl`,
so both `curl` and this script can be allow-listed to avoid permission
prompts (see docs/localhost-api-search-endpoints.md for the API itself).

It auto-detects the response shapes the API returns:

  1. Search results  -- a JSON object with a `results` array
                        (/search, /suttas_fulltext_search,
                         /suttas_contains_search, /dict_combined_search).
                        Prints hit count, the deconstructor split (if any),
                        and one block per result: ref / title / uid + snippet.
  2. Word records    -- a JSON array (/words/<uid>.json). Prints the salient
                        dictionary fields (lemma, grammar, meaning, ...).
  3. Word envelope   -- the /word.json?...&verbose=1 diagnostic object
                        ({found, canonical_uid, result|hint}); prints the
                        resolved uid (or the miss hint).
  4. Health snapshot -- the /health object (app_version, counts,
                        dict_sources, fulltext_searcher_ready) and the
                        /sutta_and_dict_search_options filter lists.
  5. Rendered HTML   -- anything that is not JSON (get_sutta_html_by_uid,
                        get_word_html_by_uid). Tags are stripped to plain text.

Typical use (PORT from <SIMSAPA_DIR>/api-port.txt, default 4848):

  PORT=$(cat ~/.local/share/simsapa/api-port.txt)

  # Probe the environment first (version, counts, installed dictionaries)
  curl -s "localhost:$PORT/health" | python3 scripts/simsapa_fmt.py

  # Search the suttas (literal substring)
  curl -s -X POST "localhost:$PORT/suttas_contains_search" \\
    -H 'Content-Type: application/json' \\
    -d '{"query_text":"vedanā aniccā"}' | python3 scripts/simsapa_fmt.py

  # Fulltext (stemmed) search, keep more results and longer snippets
  curl -s -X POST "localhost:$PORT/suttas_fulltext_search" \\
    -H 'Content-Type: application/json' \\
    -d '{"query_text":"mindfulness of breathing"}' \\
    | python3 scripts/simsapa_fmt.py --max 15 --snippet-len 300

  # Dictionary lookup (gender / part of speech / meaning)
  curl -s -X POST "localhost:$PORT/dict_combined_search" \\
    -H 'Content-Type: application/json' \\
    -d '{"query_text":"kaññā","dict_dict":"DPD"}' | python3 scripts/simsapa_fmt.py

  # Full word record as JSON
  curl -s "localhost:$PORT/words/kaññā%2Fdpd.json" | python3 scripts/simsapa_fmt.py

  # Full sutta text (HTML stripped to plain text)
  curl -s "localhost:$PORT/get_sutta_html_by_uid/web/sn22.59/pli/ms" \\
    | python3 scripts/simsapa_fmt.py
"""

import argparse
import html
import json
import re
import sys

# A search hit (SearchResult, see docs §11): the fields worth surfacing.
RESULT_REF_FIELDS = ("sutta_ref", "title", "uid")

# Dictionary word-record fields, in the order most useful to the agent. Only
# those present in a given record are printed (DPD headword / root / dict_word
# rows all differ). See docs §13.3.
WORD_FIELDS = (
    "word", "lemma_1", "lemma_clean", "dpd_lemma_clean",   # headword
    "pos", "grammar", "stem", "pattern",                   # grammar
    "root", "root_meaning", "root_sign",                   # roots
    "meaning_1", "meaning_2", "meaning_lit",               # DPD meanings
    "definition_plain", "definition_html",                 # dict_words
    "construction", "sanskrit", "example_1",
)


# DPD feedback widgets render thumbs-up/down emoji as text inside <a> tags, so
# tag-stripping leaves them behind. Mirror backend strip_html() and drop them.
RE_THUMBS = re.compile(r"[\U0001F44D\U0001F44E]+")

# ANSI color used to highlight matched terms in `«…»` markers (bold yellow).
COLOR_MATCH = "\033[1;33m"
COLOR_RESET = "\033[0m"


def strip_html(text, mark_matches=True):
    """Strip HTML tags. `<span class='match'>X</span>` -> `«X»` when marking."""
    if mark_matches:
        # Producer-owned, non-nested match spans (docs §1). Mark before stripping.
        text = re.sub(
            r"<span[^>]*class=['\"]match['\"][^>]*>(.*?)</span>",
            r"«\1»", text, flags=re.DOTALL | re.IGNORECASE,
        )
    text = re.sub(r"(?s)<!--.*?-->", " ", text)               # comments
    text = re.sub(r"(?is)<head\b.*?</head>", " ", text)       # full-page <head>
    text = re.sub(r"(?is)<(script|style)\b.*?</\1>", " ", text)
    text = re.sub(r"<[^>]+>", " ", text)
    text = html.unescape(text)
    text = RE_THUMBS.sub("", text)                            # DPD feedback emoji
    text = re.sub(r"[ \t]+", " ", text)
    text = re.sub(r"\s*\n\s*\n\s*", "\n\n", text)
    return text.strip()


# Matched terms wrapped in `«…»` markers by strip_html() (or already present in
# rendered HTML). Used to apply color after clipping so the ANSI escapes never
# affect length counting or get truncated mid-sequence.
RE_MARK = re.compile(r"«(.*?)»", re.DOTALL)


def colorize(text):
    """Wrap the contents of `«…»` markers in an ANSI color for terminal output."""
    return RE_MARK.sub(f"«{COLOR_MATCH}\\1{COLOR_RESET}»", text)


def clip(text, length):
    """Collapse whitespace and truncate to `length` chars (0 = no limit)."""
    text = re.sub(r"\s+", " ", text).strip()
    if length and len(text) > length:
        return text[:length].rstrip() + " …"
    return text


def fmt_search(data, args, out):
    """Format a search response: {hits/total_hits, results[], deconstructor?}."""
    results = data.get("results") or []
    total = data.get("total_hits", data.get("hits", len(results)))
    shown = results if args.max == 0 else results[: args.max]

    if len(shown) < len(results):
        # The formatter's --max (not the API) is capping the display; surface it.
        print(f"# {total} hit(s); showing {len(shown)} out of "
              f"{len(results)} returned rows", file=out)
    else:
        print(f"# {total} hit(s); showing {len(shown)}", file=out)

    deco = data.get("deconstructor") or []
    if deco:
        # Each element may itself be a list of split alternatives.
        joined = "; ".join(
            " + ".join(d) if isinstance(d, list) else str(d) for d in deco
        )
        print(f"# deconstructor: {joined}", file=out)

    if not shown:
        print("# (no results)", file=out)
        return

    for i, r in enumerate(shown, 1):
        ref = r.get("sutta_ref") or ""
        title = r.get("title") or ""
        uid = r.get("uid") or ""
        head = " — ".join(p for p in (ref, title) if p) or uid
        print(file=out)
        line = f"[{i}] {head}"
        if uid and uid not in head:
            line += f"  ({uid})"
        print(line, file=out)
        if not args.no_snippet and r.get("snippet"):
            snip = strip_html(r["snippet"], mark_matches=not args.no_marks)
            snip = clip(snip, args.snippet_len)
            if args.color:
                snip = colorize(snip)
            if snip:
                print(f"    {snip}", file=out)


def fmt_words(records, args, out):
    """Format a /words/<uid>.json response: a list of DB-row dicts."""
    if not records:
        print("# (no word record)", file=out)
        return
    print(f"# {len(records)} word record(s)", file=out)
    for i, rec in enumerate(records, 1):
        if not isinstance(rec, dict):
            print(f"\n[{i}] {rec!r}", file=out)
            continue
        uid = rec.get("uid") or rec.get("id") or ""
        print(f"\n[{i}] {uid}".rstrip(), file=out)
        printed = False
        for key in WORD_FIELDS:
            val = rec.get(key)
            if val in (None, "", []):
                continue
            text = clip(strip_html(str(val), mark_matches=False), args.snippet_len)
            if text:
                print(f"    {key}: {text}", file=out)
                printed = True
        if not printed:
            # Unknown shape: show scalar fields so nothing is silently lost.
            for key, val in rec.items():
                if isinstance(val, (str, int, float)) and str(val).strip():
                    print(f"    {key}: {clip(str(val), args.snippet_len)}", file=out)


def fmt_word_envelope(data, args, out):
    """Format the /word.json?...&verbose=1 diagnostic envelope (docs §13.3)."""
    found = data.get("found")
    canonical = data.get("canonical_uid")
    query = data.get("query_uid") or data.get("query_text") or ""
    if found:
        print(f"# found: {query} -> {canonical}", file=out)
        # `result` is a single record object here (not the bare-array shape).
        rec = data.get("result")
        fmt_words([rec] if isinstance(rec, dict) else (rec or []), args, out)
    else:
        print(f"# not found: {query}", file=out)
        if data.get("hint"):
            print(f"# hint: {data['hint']}", file=out)


def fmt_health(data, out):
    """Format a /health snapshot (docs §10): the fields an agent probes first."""
    print(f"# health: simsapa {data.get('app_version', '?')} "
          f"on port {data.get('api_port', '?')}", file=out)
    print(f"# fulltext_searcher_ready: {data.get('fulltext_searcher_ready')}", file=out)
    counts = data.get("counts") or {}
    if counts:
        joined = ", ".join(f"{k}={v}" for k, v in counts.items())
        print(f"# counts: {joined}", file=out)
    langs = data.get("sutta_languages") or []
    if langs:
        print(f"# sutta_languages: {', '.join(map(str, langs))}", file=out)
    sources = data.get("dict_sources") or []
    if sources:
        print(f"# dict_sources: {', '.join(map(str, sources))}", file=out)
    paths = data.get("db_paths") or {}
    for name, path in paths.items():
        print(f"# db {name}: {path}", file=out)


def fmt_options(data, out):
    """Format /sutta_and_dict_search_options: the available filter values."""
    for key in ("sutta_languages", "dict_languages", "dict_sources"):
        vals = data.get(key) or []
        print(f"# {key}: {', '.join(map(str, vals)) or '(none)'}", file=out)


def main(argv=None):
    ap = argparse.ArgumentParser(
        description="Format a Simsapa localhost-API response read from stdin.",
        epilog="See the module docstring for full curl examples.",
    )
    ap.add_argument("--max", type=int, default=0,
                    help="max results to show (0 = all the API returned, the "
                         "default; the API's page_len already bounds the rows).")
    ap.add_argument("--snippet-len", type=int, default=220,
                    help="truncate snippets/fields to N chars (0 = full; default 220).")
    ap.add_argument("--no-snippet", action="store_true",
                    help="omit snippet text (headers/refs only).")
    ap.add_argument("--no-marks", action="store_true",
                    help="do not wrap matched terms in «…».")
    ap.add_argument("--no-color", action="store_true",
                    help="disable ANSI color highlighting of matched terms "
                         "(color is on by default when stdout is a terminal).")
    ap.add_argument("--raw", action="store_true",
                    help="pretty-print the parsed JSON unchanged.")
    args = ap.parse_args(argv)

    out = sys.stdout
    # Color the matched terms by default on a terminal; off when piped/redirected
    # or when --no-color / --no-marks is given (no markers means nothing to color).
    args.color = not args.no_color and not args.no_marks and out.isatty()

    raw = sys.stdin.read()

    if not raw.strip():
        print("# (empty response — is the Simsapa app running?)", file=sys.stderr)
        return 1

    try:
        data = json.loads(raw)
    except json.JSONDecodeError:
        # Not JSON: assume rendered HTML (full sutta / word page).
        text = strip_html(raw, mark_matches=not args.no_marks)
        if args.color:
            text = colorize(text)
        print(text, file=out)
        return 0

    if args.raw:
        json.dump(data, out, ensure_ascii=False, indent=2)
        print(file=out)
        return 0

    if isinstance(data, dict) and "results" in data:
        fmt_search(data, args, out)
    elif isinstance(data, dict) and "found" in data:
        fmt_word_envelope(data, args, out)            # word.json?verbose=1 (§13.3)
    elif isinstance(data, dict) and "app_version" in data:
        fmt_health(data, out)                         # /health (§10)
    elif isinstance(data, dict) and "sutta_languages" in data:
        fmt_options(data, out)                        # /sutta_and_dict_search_options
    elif isinstance(data, list):
        fmt_words(data, args, out)
    else:
        # Unknown JSON shape — pretty-print so nothing is hidden.
        json.dump(data, out, ensure_ascii=False, indent=2)
        print(file=out)
    return 0


if __name__ == "__main__":
    sys.exit(main())

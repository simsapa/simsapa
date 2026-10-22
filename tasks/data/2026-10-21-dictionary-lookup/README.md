# Data for the dictionary-lookup supersede / break-down guesses PRD

Supporting files for
[the PRD](../../2026-10-21-185229-prd---dictionary-lookup-supersede-and-breakdown-guesses.md)
and [the task list](../../2026-10-21-185229-tasks-dictionary-lookup-supersede-and-breakdown-guesses.md).
The figures drawn from them are in the task list's **Measurements** section.

Paths below are the dist database:
`../bootstrap-assets-resources/dist/simsapa/app-assets/dpd.sqlite3` (from the
repo root). Every script here only reads it.

## Files

### `guess_sample_30k.tsv` — the usefulness sample (task 3.3)

Output of the `#[ignore]`d test `backend/tests/test_dpd_breakdown_guess_sample.rs`
with `DPD_GUESS_SAMPLE_SIZE=30000`: one row per sampled token that reaches the
break-down guesses (314 rows). Columns: the query, `other_results` (Phases 1–6
found something), `p8_results` (Phase 8's result count when guesses are off),
then for attempt 2 and attempt 4 the key, has-break-down and break-downs under
each guess form (`like`, `range`, `range_bd`), then `sutta_uid`, `context` and
an empty `useful`.

**Task 7.1 uses it:** the equivalence check runs over every attempt-2 and
attempt-4 prefix of its `query` column (attempt 4's prefix is the query minus
its last letter).

Regenerate it exactly (the seed is fixed; about 8 minutes in a debug build):

``` sh
cd backend
DPD_GUESS_SAMPLE_SIZE=30000 DPD_GUESS_SAMPLE_OUT=/path/to/out.tsv \
  cargo test --test test_dpd_breakdown_guess_sample -- --ignored --nocapture
```

A changed DPD database or a changed normalization gives a different sample.

### `guess_review.tsv` — the graded review (tasks 3.4, 3.5)

The 63 distinct queries with a `range_bd` guess, graded against their sutta
context and the DPD entries each component resolves to. The grading was done
by the agent at the user's request. Columns:
- `guess_key`, `guess_breakdown` — the `range_bd` guess, the form that ships;
- `like_*` — today's `LIKE` guess, for comparison;
- `grade` — `clean` / `partial` / `wrong`; the definitions are in the task
  list's Measurements, "Review (3.4, 3.5)";
- `useful` — `y` for clean or partial, `n` for wrong;
- `like_grade`, `like_useful` — the same grading for `LIKE`'s guess where it
  picks a different key with a break-down. `=` means the same key as
  `range_bd`, `no-bd` means `LIKE`'s row has no break-down, and `-` means
  `LIKE` found nothing;
- `note` — the reason for the grade.

This file is the only row-level record of the review. It cannot be
regenerated; don't overwrite it.

### `guess_cost.py` — cost per guess form (task 2.8; task 7.1 extends it)

Times guess forms A (`LIKE`), B (range, shortest key), C (range +
`deconstructor != ''`) and D (keys from the index, then rows one at a time)
for the five test words' attempt-2 and attempt-4 prefixes plus `sam` and
`saṁ`. Each form runs cold (the file is evicted with
`posix_fadvise(DONTNEED)`) and warm. It reports ms, bytes requested (`rchar`)
and bytes read from disk (`read_bytes`), from `/proc/self/io`, so it is
Linux-only.

``` sh
python3 -I guess_cost.py <path to dpd.sqlite3>
```

Task 7.1 runs it against a **scratch copy** of the `lookup` table that has the
partial index, never the dist DB. It adds `par`, `paṭ`, `anu` and `upa` to the
prefix set.

### `analyse.py` — structural counts over the sample (task list "3.0 findings")

Reads `guess_sample_30k.tsv` from the current directory. Prints how often
Phase 8 would answer, the `LIKE` / range / `range_bd` comparison, attempt
counts, query lengths and the split by edition.

``` sh
cd tasks/data/2026-10-21-dictionary-lookup && python3 -I analyse.py
```

### `make_review.py` — builds the review file from the sample

``` sh
python3 -I make_review.py guess_sample_30k.tsv <out.tsv> all
```

**Never give it `guess_review.tsv` as the output:** it writes an unmarked file
and would erase the grades. Without the third argument it caps the list at 50
rows.

### `comps.py` — component meanings, used while grading

For each row of `guess_review.tsv`, prints every break-down component with the
DPD headwords and meanings it resolves to. It reads the file from the current
directory and the dist `dpd.sqlite3` by absolute path.

## When to remove this folder

Keep it until the PRD is finished. Task 7.1 reads the sample and extends
`guess_cost.py`, task 8.2 may re-run the harness, and a reopened Keep
decision (PRD §9 Q5) starts from `guess_review.tsv`. Remove it in task 8.9,
when the PRD and task list move to `tasks/archive/`. Do it in its own commit,
and record that commit's hash in the archived task list so the files can be
recovered with `git checkout <hash>^ -- tasks/data/2026-10-21-dictionary-lookup/`.
If a follow-up PRD needs any of these files, copy them into its own
`tasks/data/` folder first.


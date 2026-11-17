# Rebuilding the suttas-core database for a bootstrap

The bootstrap reads suttas-core's segmented translations from
`<bootstrap assets>/suttas-core-db/current/suttas.sqlite`
([suttas-core-import.md](./suttas-core-import.md) §1). This is the procedure that
produces that file before a bootstrap: bring the local `simsapa` branch of
suttas-core up to date, re-pin it to the checked-out sc-data, build, and file
the result in a dated folder.

Paths below are relative to the bootstrap assets folder,
`~/prods/apps/simsapa-ng-project/bootstrap-assets-resources`:

| folder | what it is |
|---|---|
| `suttas-core/` | clone of `https://git.unxr.net/adicco/suttas-core`; we build on its local `simsapa` branch, never pushed |
| `sc-data/` | clone of SuttaCentral's sc-data; read-only input. `suttas-core/data/sc-data` is a symlink to it |
| `suttas-core-db/<date>/` | one build: `suttas.sqlite`, `build.log`, `BUILT-FROM.txt` |
| `suttas-core-db/current` | symlink to the dated folder the bootstrap uses |

## 1. Bring the `simsapa` branch up to date

`simsapa` is upstream `master` plus our feature branches that upstream has not
merged yet. Start by finding out which of them upstream has taken.

``` sh
cd suttas-core
git fetch origin
git branch -vv                                   # our branches and their tips
git branch --no-merged origin/master             # not merged as-is ...
git log --oneline origin/master..<branch>        # ... their commits
git cherry -v origin/master <branch>             # '-' = same patch upstream
```

Upstream usually **rebases** our work before merging it, so `git cherry` and
`--no-merged` report it as missing even when it is there. Look for each commit's
subject upstream, then compare the two versions:

``` sh
git log --oneline origin/master -i --grep="<commit subject>"
git range-diff <ours>^! <upstream>^!            # '=' identical, '!' adapted
```

A `!` whose diff only shows lines that differ because of the surrounding code
(another branch's change, a later edit), with none of our own lines dropped,
means the work is merged. Our "re-pin sc-data" commits never need to survive:
the re-pin in §2 replaces them.

**Every branch merged upstream:** re-branch `simsapa` from upstream and delete
the merged branches (their tips stay in the reflog):

``` sh
git switch master && git merge --ff-only origin/master
git branch -f simsapa origin/master
git branch --unset-upstream simsapa              # -f sets tracking; simsapa is never pushed
git branch -D <merged-branch> ...
git switch simsapa
```

**Some branches still unmerged:** rebuild `simsapa` from upstream and merge the
remaining ones back in. `rerere` is on in this clone (`autoupdate` off), so a
conflict resolved before is replayed into the working tree unstaged: check it,
`git add`, commit.

``` sh
git switch master && git merge --ff-only origin/master
git branch -f simsapa origin/master && git branch --unset-upstream simsapa
git switch simsapa
git merge <unmerged-branch>                      # one per branch
```

Then update the branch list in [suttas-core-import.md](./suttas-core-import.md) §1.

## 2. Re-pin sc-data to our checkout

suttas-core reads sc-data only at the commit in `data/sc-data.lock.json`, and
refuses to read it unless the checkout is clean and at that commit. Its own
procedure is in `suttas-core/docs/bilara.md`, "Syncing a new sc-data commit".

``` sh
git -C ../sc-data status --short                 # must be clean
git -C ../sc-data log --oneline -1               # the commit we will pin
# git -C ../sc-data pull                         # only to take a newer sc-data

bun run src/bilara/sync.ts --show 10             # report only: locked commit → sc-data HEAD
bun run src/bilara/sync.ts --remap               # only if the report lists remappable files
bun run src/bilara/sync.ts --write               # pin HEAD in data/sc-data.lock.json
bun run src/check/stale.ts                       # every translator: stale 0
```

Read the report before `--write`. `0 with id changes` and `need re-segmenting:
0 files` means nothing else to do. Files that need re-segmenting go through
`src/segment/run.ts`, an LLM run. That is upstream's work, so report it to them
instead of running it here. `--remap` needs the old commit, so it runs before or
together with `--write`.

The lock change is a one-line diff on `simsapa`. Commit it there when the build
is accepted (`re-pin sc-data to <commit>`, with the report's counts in the body).

## 3. Build into a new dated folder

The build is light (about 30 s, well under 1 GB), but run it capped anyway.

``` sh
D=$(date +%F)
mkdir -p ../suttas-core-db/$D
systemd-run --user --scope --quiet -p MemoryMax=8G -p MemorySwapMax=1G -- \
  bun run src/build/build.ts --verse-indent --emphasis \
    --out ../suttas-core-db/$D/suttas.sqlite \
  >| ../suttas-core-db/$D/build.log 2>&1
tail -5 ../suttas-core-db/$D/build.log
```

`--verse-indent` and `--emphasis` are required: without them Ṭhānissaro's verse
layout and italics are missing and the import tests in §5 fail.

Compare the new log with the last one. The table counts should move only where
the upstream changes explain it:

``` sh
diff ../suttas-core-db/current/build.log ../suttas-core-db/$D/build.log
```

## 4. Record where it came from, repoint `current`

``` sh
cat >| ../suttas-core-db/$D/BUILT-FROM.txt <<EOF
suttas-core branch simsapa @ $(git rev-parse --short HEAD) (origin/master $(git rev-parse --short origin/master); merged: <branches, or none>)
sc-data pinned at $(jq -r .commit data/sc-data.lock.json)
bun run src/build/build.ts --verse-indent --emphasis --out ../suttas-core-db/$D/suttas.sqlite
EOF
ln -sfn $D ../suttas-core-db/current
```

The older dated folders stay as they are, so pointing `current` back at one undoes
the switch.

## 5. Check the import against it

The `suttas_core` real-data tests read `suttas-core-db/current/suttas.sqlite`
and the dist `appdata.sqlite3` read-only. A first run compiles `bootstrap_cli`
from scratch, so cap it:

``` sh
cd ../../simsapa/bootstrap_cli
systemd-run --user --scope --quiet -p MemoryMax=12G -p MemorySwapMax=2G -- \
  env CARGO_BUILD_JOBS=4 cargo test suttas_core::tests::test_real_data
```

That runs the four that are not `#[ignore]`: Dhp titles and ranges, verse
indentation, emphasis and opening dashes, titles without note markers. The
`#[ignore]` ones (`real_data_builds`, `real_data_footnote_links`,
`real_data_thanissaro_map`, `real_data_replacement`) go further. Their doc
comments give the commands to run them.

Then run the bootstrap. Its preflight fails early if `current/suttas.sqlite` is
missing.

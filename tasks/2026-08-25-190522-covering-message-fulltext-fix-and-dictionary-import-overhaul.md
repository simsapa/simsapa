# Covering message for the reporting user (task 8.7)

Attach: `dist/Simsapa-<version>-beta.apk`

---

Thank you for running those two tests — they told me exactly what I needed, and
I have fixed both problems. Here is a test build with the fixes in it.

This one installs **alongside** your normal Simsapa rather than replacing it —
it is called **"Simsapa (beta)"** and has its own icon, so you will see two
Simsapa apps on your device. That is expected. Your existing app and its data
are untouched.

Could you install it and do these three things, in this order?

**1. Import the dictionary.** Open **Dictionaries** and press
**Import StarDict/GoldenDict...**, then choose **`all-dictionaries-gd.zip`** from
`Documenti/Dizionari`.

Please use the **`-gd`** one, **not** the `mdict` one. The `mdict` file is a
different dictionary format that Simsapa cannot read at all — that is a real
limitation, not a bug, and this build now says so clearly if you pick it. Mixing
the two is what made the earlier attempts hard to interpret.

Two things will look different from before, and both are intended:

- The file chooser may open, close, and then a short message may appear saying
  Simsapa is going to try a different file chooser. If that happens, just say
  yes. That message is deliberate — it also tells me which chooser worked, which
  is one of the things I am trying to find out.
- **That zip contains many dictionaries, not one.** After it finishes examining
  the file you will get a list with a row for each dictionary inside it — a dozen
  or so. That is correct. You can tick all of them, or just the ones you want.
  Each becomes its own dictionary in Simsapa, with its own name. Before this
  build, Simsapa could only ever import **one** of them, which is part of what
  was going wrong.

A large file takes a while to copy and then a while to import. There is a
progress bar for both now, and a Cancel button.

**2. Try a fulltext search.** Search for a word you would expect to find in the
suttas — anything you know is in there. Previously only the "contains" style of
search worked on your device; the fulltext one silently found nothing, every
time. It should work now.

**3. Send me the log — last, after doing steps 1 and 2.** Open **Help → About**,
scroll down to the list of log files, press **Copy Contents** on the most recent
one and paste it into your reply. (If it is too long to paste, use **Save As…**
and attach the file instead.)

The order matters: the log is what records what happened in steps 1 and 2, so
copying it first would send me an empty one.

If either step does not work, that is still a useful result — please send the log
anyway and tell me roughly where it stopped.

---

## What I am reading in the returned log (not for the user)

| Grep | Tells me |
|---|---|
| `DICTIONARY-IMPORT-PICK:` | which picker path was used, its `filter_config`, and — the key question — **whether the fallback fired**. `raw_branch:` present means Qt's `FileDialog` returned nothing even without the `.zip` filter (open question 1 answered "no, the filter was not the whole cause"); absent means dropping `nameFilters` alone fixed it, and the private-Qt dependency can be removed again. |
| `outcome_line` sentence | whether the read actually succeeded, now that it is a function of the result and not of the input. |
| `FulltextSearcher opened:` | non-zero counts for sutta / dict / library. Fix-PRD success metric 1. |
| absence of `Failed to acquire Lockfile` | the wrapper is taking the fallback rather than failing. |
| `storage capability verdicts` | both `flock` and `mmap` verdicts against the real index directory (FR-33). |
| `scan_source: rejected` | anything in the bundle that was refused, and why. |
| `sweep_orphaned_*` | the two startup sweeps reclaiming anything left by the earlier failed attempts. |

Fix-PRD success metrics 1–6 and 9, and picker-PRD phase-1b's decision gate, are
all settled by this one round trip.

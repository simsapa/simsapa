# Investigation: the bottom column bar stops sticking to the bottom (Android)

**Status: OPEN — the root cause is not determined.** Nothing here is settled
knowledge yet. The code described below is in the tree, but it is a *candidate*
fix plus the instrumentation needed to judge it. Do not fold any of this into
the permanent docs until a reproduction on the affected device has been read;
§8 says where each part should go when it is.

Opened 2026-08-10. The other docs carry only one-line pointers here on purpose.

---

## 1. The issue as reported

An Android user, reading a sutta with the bottom column bar shown ("Pāli (ms)"),
looked up a word (*etadavoca*), which opens the **WordSummary** panel below the
reader. After closing the panel, the column bar **stopped sticking to the bottom
of the page**: it sat at a fixed mid-screen height and stayed there while the
reader scrolled.

Three further details from the user, all load-bearing:

1. sutta text was painted **above and below** the stranded bar — the bar looked
   like a layer floating over continuous text, not like the end of the page;
2. **scrolling** the page did not recover it;
3. **re-opening and re-closing** the WordSummary did not recover it either.

The same bar behaves correctly on desktop, and we have **not reproduced it on
any device we own** — which is the fact that shapes everything below.

### The mechanism it touches

The WordSummary panel is the second pane of the vertical `SplitView` in
`SuttaSearchWindow.qml` (`word_summary_wrap`). Opening it **shrinks** the
reader's webview; closing it **grows the webview back**. On mobile that is a
resize of the *native* Android `WebView`, not of a Qt item.

---

## 2. Ruled out: the CSS

Checked and cleared — **do not "fix" `.column-bar`**:

- `.column-bar` is a plain `position: fixed; bottom: 0`
  (`assets/sass/_display_settings.scss`), and so is `.footnote-bottom-bar`
  (`_footnote_bottom_bar.scss`).
- `#columnBar` sits directly inside `#ssp_main` (`assets/templates/page.html`),
  and **no ancestor** has `transform` / `filter` / `backdrop-filter` /
  `will-change` / `contain` — any of which would hijack the containing block,
  and which is the usual explanation for a misplaced fixed element. (The
  `backdrop-filter` in `_footnote_bottom_bar.scss` is on a *sibling* bar.)
- No media query and no runtime inline style touches the bar's position
  (`column_bar.ts` only toggles `.show` and rewrites `#columnBarItems`).

A `bottom: 0` fixed element resolves against the viewport height *as the page
understands it*, so the symptom says the page is still laid out against the
shorter, summary-open viewport. It also explains why only the bottom chrome is
affected: the top-anchored fixed chrome (`.ssp-toolbar`, menu, find bar) is
`top`-anchored and cannot show this error.

---

## 3. What the symptom narrows it to (reasoning, not yet confirmed)

- **(1) text above and below** rules out the whole viewport being short — the
  page occupies the full height. It is what a wrong **initial containing block**
  looks like: block content height is content-driven and reflows fine, while
  `position: fixed` resolves against the ICB, so only the viewport-anchored
  chrome lands wrong.
- **(2) scrolling doesn't help** rules out a merely dirty paint: a scroll of a
  fixed element is handled on the compositor and re-runs no layout, so a stale
  ICB survives it untouched.
- **(3) a repeat open/close doesn't help** says the resize itself is what is not
  being acted on — the second one is ignored for the same reason as the first.

**Leading hypothesis: Chromium never acts on the native view's new size.** If
so, everything the page can observe is stale too, `window.innerHeight`
included — meaning **no JavaScript can repair it from inside the page**, and it
has to be addressed from the Qt side.

**Still possible, not excluded:** a compositor-only fault, where the main
thread's layout is correct and only the fixed layer is pinned to the old
viewport bottom. That variant is invisible to `getBoundingClientRect()`, so the
instrumentation is built so that its *absence of evidence* is itself
recognisable (§5).

### An incidental finding

`word_summary_wrap.handle_summary_close()` was already calling
`window.word_summary_closed()`, but that function had not existed since
`tasks/archive/2026-04-21-061133-tasks-mobile-single-tap-dictionary-lookup.md`
task 2.1 removed the old implementation. The QML `typeof … === 'function'` guard
swallowed every call silently. That dead hook is now where the fix lives — but
note it means **no in-page code had run on summary close for months**, which is
worth remembering if the timeline of the user's reports ever matters.

---

## 4. What was implemented

Two candidate fixes at different levels, deliberately kept separable so the log
can say which one (if either) is doing the work.

| # | Where | What |
|---|---|---|
| 1 | `bridges/assets/qml/SuttaHtmlView_Mobile.qml` — `nudge_webview_geometry()` | 1px `anchors.bottomMargin` jiggle, so the native view re-sends a size Chromium must observe. Same remedy `WebEngineRepaintNudge.qml` uses for the desktop stale-frame bug. Exposed through the `SuttaHtmlView.qml` Loader; a no-op in `SuttaHtmlView_Desktop.qml`. **It fires at 250 ms, not immediately** — see §5.1; firing it early made the log credit it for work the ordinary resize had done. |
| 2 | `src-ts/viewport_nudge.ts` — `force_relayout()` | In-page repair: pins `documentElement` to `window.innerHeight` in px for one layout pass (defeating a possibly-stale `height: 100%`), toggles each bottom bar out of and back into the box tree (discarding a stale layer), scrolls 1px and back, dispatches a synthetic `resize`. |

`handle_summary_close()` calls the jiggle first, then the page hook.

The page-facing hooks, all installed by `init_viewport_nudge()`:

| hook | called from | purpose |
|---|---|---|
| `window.word_summary_closed(qt_h0)` | `handle_summary_close()` | starts a run |
| `window.ssp_report_qt_geometry(h, dpr, phase)` | the `pre_jiggle` / `post_jiggle` timers in `SuttaHtmlView_Mobile.qml` | reports Qt's settled geometry **and** drives the page's measurement phases |
| `window.ssp_word_summary_opened()` | `set_summary_query()` | ends any run still measuring, so a re-open cannot be recorded as a fault (§5.2) |
| `window.ssp_viewport_nudge(reason)` | nothing yet | for any future caller that resizes the webview |

**Fix 2's first pass is unconditional on purpose.** A compositor-only
discrepancy cannot be measured from inside the page, so acting only on a
measured mismatch would mean never addressing that variant.

**Why both:** the jiggle addresses "Chromium never learned the new size" (what
the symptom points at, and what JS cannot touch); the in-page relayout addresses
a stale ICB or a stale layer *once the size has arrived*. Neither is known to be
necessary yet.

Files touched: `bridges/assets/qml/SuttaSearchWindow.qml` (`handle_summary_close()` and
`set_summary_query()`), `bridges/assets/qml/SuttaHtmlView.qml` (Loader pass-throughs),
`bridges/assets/qml/SuttaHtmlView_Mobile.qml` (the jiggle + timer chain),
`bridges/assets/qml/SuttaHtmlView_Desktop.qml` (no-op counterparts),
`src-ts/viewport_nudge.ts` (+ `.test.ts`, 18 tests), `src-ts/simsapa.ts` (init).

**No new QML component file and no new Rust bridge function was added**, so
`bridges/build.rs` and the `qmllint` stubs in
`bridges/assets/qml/com/profoundlabs/simsapa/` need no entry.

---

## 5. The instrumentation, and how to read it

Since we cannot reproduce it, **one reproduction on the affected phone has to
settle three things**: the root cause, which fix helped, and which can be
deleted. Two kinds of line are logged per close — QML phase markers, and one
summary line from the page.

```
VIEWPORT-NUDGE-QT: phase=close item=360x291 web=360x291 dpr=3
VIEWPORT-NUDGE-QT: phase=pre_jiggle web=360x582 dpr=3
VIEWPORT-NUDGE-QT: phase=post_jiggle web=360x582 dpr=3
VIEWPORT-NUDGE: run=3 reason=word_summary_closed result=ok resolved_at=natural
  qt_h=582 qt_dpr=3 qt_h0=291 dpr=3 scroll_h=3631 bar=columnBar rs_at=[38,301,352]
  close[t=0 rs=0 inner=291 … bar_bottom=291 off=0 vgap=873]
  natural[t=251 rs=1 inner=582 … bar_bottom=582 off=0 vgap=0]
  jiggle[t=402 …] relayout[t=405 …] settled[t=1106 …]
```

Ask the reporting user for lines matching `VIEWPORT-NUDGE` in `log.txt`.

**`resolved_at=natural` is the healthy reading** — the ordinary resize did the
work and neither fix was needed. Anything else names the phase that put the page
right.

### 5.1 Dry run on a healthy device (2026-08-10) — what it changed

Run on a Galaxy S23 (Android 16, WebView 150.0.7871.181, 1080×2340 @ density
480) that has **never shown the bug**, purely to check the instrumentation. 19
closes across Lines and Solo layouts, portrait and landscape, scrolled and
unscrolled. Visual behaviour was correct throughout — including no scroll jump
in the scrolled case.

**It found a false positive that would have sent us the wrong way.** All 19 runs
logged `result=fixed fixed_by=jiggle`. The baseline was measured at t=0, before
the *ordinary* resize had been delivered (`rs=0`, `inner` still the summary-open
291), so it always looked broken; by the next phase everything was right —
whether because the jiggle did it or because the resize simply arrived. The two
were indistinguishable, and the log would have told us to keep the jiggle and
delete the in-page relayout **on no evidence at all**.

The fix is the `natural` phase plus QML-driven timing, and the jiggle now fires
at 250 ms instead of 50 ms so there is a clean window before it. The healthy
expectation is now `result=ok resolved_at=natural`.

Three things the run did confirm:

- **`qt_h0` ≠ `qt_h` on every single run** (291 vs 582) — the deferred geometry
  report is load-bearing, not theoretical. Reading `web.height` inside
  `handle_summary_close()` really does return the summary-open height.
- **The device-pixel-ratio assumption holds.** `dpr=3` on both sides, `vgap=0`
  once settled, and `inner` within 1 px of `qt_h0` (424 vs 425) — comfortably
  inside the 8 px tolerance. This was listed as an open question; it is closed.
- **The hidden-bar path works**: Solo layout gave `bar=none bar_bottom=n/a` with
  `probe=` still measured.

Also noted: `run=` restarts at 1 on every page load, since the module state goes
with the page. Group log lines by timestamp, not by run number alone.

### 5.2 Second dry run, after the `natural` phase (2026-08-11)

Same healthy Galaxy S23, 25 closes. **The false positive is gone**: all 25 read
`result=ok resolved_at=natural`, with the ordinary resize delivered at
**9–16 ms** — so the 250 ms pre-jiggle window is generously sized. `qt_h0`
differed from `qt_h` on all 25 again, `vgap` settled to 0, nothing logged at
error level, and Solo gave `bar=none` as expected.

It exposed two further measurement defects, both now fixed. Neither was visible
without running it, and either would have produced a wrong verdict on the
affected device:

**(a) The `jiggle` phase was measured mid-jiggle — 21 of 25 runs.** It read
`inner=581` (1 px short, the `bottomMargin: 1` state) with `vgap=3`. The cause is
native resize **delivery latency of 90–127 ms**, against the 50 ms the QML timer
allowed: `rs_at` showed the restore's resize arriving 11–15 ms *after* the phase
had already been measured. Harmless here only because the 8 px tolerance
absorbed it — but on a device where the jiggle is what fixes things, the
mid-jiggle transient is precisely the measurement that must not be trusted. Fixed
by measuring **once the engine has gone quiet** (100 ms without a resize, capped
at 500 ms) instead of after a fixed delay, which self-tunes to the device.

**(b) The `settled` phase was polluted by the next open — 9 of 25 runs.** It read
the *summary-open* height (`inner=398` or `291`, `vgap=552`/`873`) because the
user re-opened the summary inside the 700 ms settle window; `rs_at` carried a
fourth resize at 590–1100 ms. The verdict survived only because `natural` had
already resolved and the first healthy phase wins — on the affected device that
luck runs out and the run would report a fault the user never saw. Fixed by
`supersede_active_runs()`: re-opening the summary (`ssp_word_summary_opened()`,
called from `set_summary_query()`) or starting another close ends any run still
measuring, which then logs `result=aborted superseded=… resolved_at=n/a` at info
level. **A superseded run is reported, never judged.**

### 5.3 Third dry run — both defects confirmed fixed (2026-08-11)

Same device, 25 closes. **Both defects from §5.2 are gone**, and no line was
logged at error level:

- **(a) fixed:** all 14 uninterrupted runs measured the `jiggle` phase with
  `vgap=0` and `inner` exactly equal to `qt_h` — 14/14, against 21/25 reading
  the 1 px-short transient before. The quiet-wait moved that measurement from
  t≈420 ms to t≈533–553 ms, which is the latency adapting as intended.
- **(b) fixed:** the 11 interrupted runs were caught as
  `superseded=summary_reopened`, and their `rs_at` carries **only three**
  resizes — the re-open's resize never reached the measurement, so the
  pollution is gone at the source.
- Unchanged and still true: ordinary resize delivered at **8–17 ms**, `qt_h0`
  ≠ `qt_h` on 14/14, every non-`close` phase `vgap=0`, Solo gives `bar=none`.

**11 of 25 runs interrupted is the finding worth acting on.** The settle window
is ~1.26 s, and looking up another word inside it is *normal reading behaviour* —
so on the affected device, discarding those runs would throw away nearly half the
evidence. The phases captured *before* the interruption are as good as any other
run's; only `settled` is taken afterwards. So an interrupted run now keeps its
verdict when an earlier phase had already resolved (logged with
`superseded=…`, and the final measurement renamed `settled_after_interrupt` so
it cannot be misread), and reports `result=aborted resolved_at=unknown` only
when the interruption came before anything resolved.

Found while deciding whether a fourth run was warranted, and fixed before it:
**on desktop every close would have logged `result=fixed resolved_at=late`.**
The geometry nudge is a no-op there, so no phase report arrives, no phase is
measured, and the in-page repair — which runs on the `post_jiggle` report —
never executes either; the verdict chain then fell through to "`settled` is
fine" and credited a fix for work nobody did. Now reported honestly as
`result=unreported resolved_at=n/a`, with an interruption taking precedence over
it (a run superseded before its first report is explained by the interruption,
not by the platform).

Also learned: `superseded=new_run` never fires in practice — a second close is
always preceded by an open, which supersedes first. It is kept as a safety net.

### 5.4 Fourth dry run — clean, and the instrumentation is considered done (2026-08-11)

Same device, 18 closes: **6 uninterrupted, 12 interrupted, 0 lines at error
level, and no new defect found.** This is the first run that changed nothing,
which is the signal to stop.

- **Uninterrupted (6/6):** `result=ok resolved_at=natural`, every non-`close`
  phase at `vgap=0`, `jiggle` reading `inner == qt_h`, plain `settled` present.
  Unchanged from §5.3.
- **Interrupted (12/12): all kept their verdict** — `result=ok
  resolved_at=natural superseded=summary_reopened`, each with a
  `settled_after_interrupt` phase and **no** plain `settled`. Under the previous
  build all twelve would have been discarded as `aborted`.
- **`rs_at` has exactly three entries in all 12**, so the re-open's resize never
  reached a measurement: the supersede lands before the pollution, not after.

Two things learned that are worth writing down rather than re-deriving:

**`settled_after_interrupt` is not, in practice, polluted.** All 12 read
`inner=582 vgap=0`. Superseding fires at the moment of the open, and the shrink
resize takes ~90 ms to be delivered, so the measurement is taken before the
webview has actually moved. The rename is therefore *precautionary* — it stops
the value being read as "the state the reader was left in" — not a filter for
observed garbage. Keep it anyway: the margin is 90 ms of luck.

**`result=aborted` is close to unreachable by hand.** The 12 interrupts landed at
t=403–1150 ms, never inside the 250 ms pre-jiggle window, so `natural` had
always been captured. A human apparently cannot close and re-open a panel in
under 250 ms. The branch stays as a safety net (like `superseded=new_run`, which
also never fires), but do not expect to see it, and do not treat its absence as a
sign the abort path is broken.

### The fields

| field | what it settles |
|---|---|
| `vgap=` | **the root cause.** Device-pixel gap between the page's viewport and the native view (`qt_h`). Large ⇒ the engine did not act on the resize, and no in-page code can help. `n/a` means Qt never reported (desktop). |
| `off=` | the page's internal consistency: bar and probe against `innerHeight`. Large ⇒ a stale bar box or layer, with the viewport itself correct. |
| `rs=` | `resize` events the engine actually delivered, per phase. `rs=0` at `jiggle` means even a forced geometry change did not get through. The synthetic `resize` that `force_relayout()` dispatches is deliberately **not** counted. |
| `resolved_at=` | **which fix earned its place.** `natural` (neither — the ordinary resize did it; the healthy reading) / `jiggle` / `relayout` / `late` / `never`. |
| `result=` | `ok` (= `resolved_at=natural`), `fixed`, `persists` (**error** level), `aborted` (interrupted before anything resolved — never judged, never an error), `unreported` (no QML phase report arrived, so nothing was measured *and nothing was repaired* — the desktop path). |
| `superseded=` | present when the user re-opened the summary (or closed again) mid-run. The run still carries a verdict if an earlier phase had resolved; its last measurement is renamed `settled_after_interrupt` and must not be read as the state the reader was left in. |
| `rs_at=` | ms offsets, from the start of the run, at which the engine delivered a resize. Read against the phase timestamps this says outright whether the engine acted on its own (an entry before ~250 ms) or only once forced. |
| `dpr`, `qt_dpr` | the two device pixel ratios `vgap` is computed through; both logged so a suspicious `vgap` can be re-derived by hand. |
| `qt_h0` | Qt's height at call time, before its layout settled. Evidence that the deferred geometry report is needed — never compared against. |

### Two traps the design exists to avoid

**`off` alone would have missed the bug.** `off` compares the bar and a probe
against `innerHeight` — all read from the same layout. Under the leading
hypothesis all three agree *with each other* at the stale height, giving `off=0`
while the bar is visibly stranded. `vgap` is what goes large. Both feed the
verdict; neither alone is sufficient.

**Qt's geometry is reported late, on purpose.** `handle_summary_close()` cannot
read a usable height: the `SplitView` re-lays out in the polish pass, so
`web.height` read there is still the *summary-open* height, and comparing
against it would state the opposite of the truth. It is reported instead from
the jiggle's restore timer via `window.ssp_report_qt_geometry(height, dpr)`, and
applied to every phase at log time (legitimate — the native height is constant
across a run). The call-time height is still logged as `qt_h0`, because
`qt_h0` ≪ `qt_h` is the evidence that the deferred report is necessary.

### Why five phases, and why QML drives them

`close` (t=0, nothing has happened yet) → `natural` (the ordinary resize has had
its chance, the jiggle has **not** fired) → `jiggle` (the jiggle landed and was
restored, before the in-page repair) → `relayout` (right after
`force_relayout()`) → `settled` (700 ms later).

**`natural` exists because of a false positive caught on a healthy device — see
§5.1.** The timing is owned by `SuttaHtmlView_Mobile.qml`, which calls
`ssp_report_qt_geometry(h, dpr, phase)` at `pre_jiggle` and `post_jiggle`; the
page measures a phase on each report. Two independently maintained sets of
timing constants would drift, and the whole attribution depends on the order
being exact.

`result=ok` is logged too, deliberately: a user reporting a stranded bar over a
run of `ok` lines is itself the diagnosis (the discrepancy is not in the page's
layout), and silence would look like "the hook never ran".

### The two bottoms within a phase

`probe` is a throwaway `position: fixed; bottom: 0` element created at
measurement time, so it reports where the page *currently* thinks the viewport
ends; `bar_bottom` is the real bar already in the page.

| `probe` | `bar_bottom` | reading |
|---|---|---|
| short | short | the page's viewport height is stale — only the geometry jiggle can help |
| correct | short | only the bar's box/layer is stale — the in-page relayout is the right level |
| correct | correct, user still sees it stranded | not in the page's layout; compositor- or engine-side |

---

## 6. Decision table — what to keep once the evidence arrives

Aggregate `resolved_at` across the session:

| evidence | conclusion |
|---|---|
| `resolved_at=natural` throughout, **and the user sees no bug** | the ordinary resize is working on that device too; the report was situational and neither fix is proven necessary. This is what the healthy dry run produces (§5.1). |
| `resolved_at=natural` throughout, **but the user still sees a stranded bar** | not in the page's layout at all — a compositor-only fault. Neither fix addresses it; next step is a Qt/Chromium-level workaround. |
| all `resolved_at=jiggle` | the ordinary resize really is being dropped and the jiggle recovers it. Keep the jiggle; the in-page `force_relayout()` never contributed, so delete fix 2's repair (keep its logging until confident). |
| all `resolved_at=relayout` | the size does arrive, and the fault is a stale ICB or layer in the page. Delete `nudge_webview_geometry()` and its timers. |
| mixed | both are load-bearing; keep both and record why. |
| `resolved_at=never`, `vgap` large, `rs_at` empty or all after 250 ms | root cause confirmed as "the engine never learned the size", and the jiggle is too weak. Escalate to re-creating or re-parenting the native view rather than resizing it. |

---

## 7. Cost

All of it runs **once per WordSummary close** — a user action, never on a
scroll, a timer or a page load. Per run: 4 measurements (each appends a 1px
hidden fixed probe, reads a rect, removes it), one `force_relayout()` (two
forced layouts from the root-height pin, two per bottom bar, a 1px scroll pair,
one synthetic `resize`), one backend log POST, two QML log lines. The synthetic
`resize` reaches one listener, `sbs_blocks.update_pane_height()`, which is
rAF-throttled, idempotent and a no-op outside the block-fallback layout.

Three properties were checked rather than assumed — **re-check them if you add
page-level listeners**:

- **nothing in the page listens for `scroll`** (`grep addEventListener src-ts/`),
  so the 1px scroll pair cascades into nothing. The footnote bar tracks
  visibility with an `IntersectionObserver`, whose callback is asynchronous and
  therefore only ever sees the restored offset;
- **no stylesheet sets `scroll-behavior: smooth`**, so the scroll pair cannot
  animate or fight the reader for the viewport;
- the scroll offset is captured **before** the box tree is touched and restored
  after, so a momentarily shorter scroll range cannot clamp the reader's place.

The forced layouts are synchronous and the document is often ~9000 px tall, so
this is a few milliseconds rather than microseconds — acceptable once per close,
and the reason none of it is on a repeating timer.

---

## 8. Where this should be re-integrated when it is resolved

Deliberately **not** merged into the permanent docs yet. When the root cause is
known:

- **§1–§3 (symptom, CSS exoneration, mechanism)** → a short subsection of
  [mobile-webview-visibility-management.md](./mobile-webview-visibility-management.md),
  which already owns "what the native mobile webview does that Qt items do not".
  The "do not fix the CSS" line belongs there and in the column-bar section of
  [sutta-display-settings-and-multi-column-view.md](./sutta-display-settings-and-multi-column-view.md).
- **§4, whichever fix survives** → same doc, next to the existing overlay layers,
  cross-referenced from
  [webengine-stale-black-frame-workaround.md](./webengine-stale-black-frame-workaround.md)
  since the jiggle is the same technique.
- **§5 (the log format)** → keep only if the instrumentation is kept. If the fix
  is confirmed and the logging removed, this section becomes history and should
  be dropped rather than archived.
- **§5.1–§5.4 (the dry runs)** → drop. They are the record of how the
  instrumentation was corrected, not facts about the app — **except** two
  measured numbers worth carrying into whatever survives: native resize delivery
  on this hardware is **90–127 ms**, and the ordinary resize after a close
  arrives in **8–17 ms**. Anyone tempted to re-tune a timing constant needs
  those.
- **§6, §7** → drop once decided; they are scaffolding for the decision, not
  facts about the app.
- The `AGENTS.md` bullet and the `PROJECT_MAP.md` clauses (on
  `src-ts/viewport_nudge.ts` and on `SuttaHtmlView.qml`) should end up one or
  two sentences: the symptom, that the CSS is not the cause, and the surviving
  fix. **Both currently point here and must be updated when this file goes.**

Delete this file when that is done — its whole purpose is to keep an unproven
investigation out of the docs that record settled behaviour.

---

## 9. Open questions

- Is the leading hypothesis (§3) right at all? Nothing has confirmed it.
- Does the jiggle actually reach Chromium on the affected device, or is a 1px
  resize also swallowed? `rs=` at the `jiggle` phase answers this.
- ~~Is `vgap` computed through the right pair of device pixel ratios on a real
  Android device?~~ **Closed by the dry run (§5.1):** both ratios read 3,
  `vgap=0` when settled, `inner` within 1 px of Qt's height.
- Is 250 ms always enough for the ordinary resize to land? On the dry-run device
  it arrived at ~38 ms. A slower device could push it past the window and be
  credited to the jiggle — `rs_at=` is what exposes that, since an engine resize
  logged before the jiggle's 250 ms mark means the engine acted on its own.
- Does the bug also affect the footnote bottom bar (also `bottom: 0`)? Nobody
  has reported it, and confirming it either way would test the diagnosis for
  free.
- Which Android version / WebView version / device? Not captured. Worth asking
  the reporting user alongside the log.

## Related

- [mobile-webview-visibility-management.md](./mobile-webview-visibility-management.md)
  — the native mobile webview and the five layers that hide it.
- [webengine-stale-black-frame-workaround.md](./webengine-stale-black-frame-workaround.md)
  — the desktop 1px resize jiggle this borrows from.
- [sutta-display-settings-and-multi-column-view.md](./sutta-display-settings-and-multi-column-view.md)
  — the column bar itself.

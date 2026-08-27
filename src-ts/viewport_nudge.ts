// Re-resolve the layout viewport after the embedding native view is resized.
//
// The bottom chrome (`.column-bar` in _display_settings.scss, the
// `.footnote-bottom-bar` in _footnote_bottom_bar.scss) is `position: fixed;
// bottom: 0`, so it resolves against the viewport height. On Android the page
// lives in a native `QtWebView` that the QML `SplitView` shrinks when the
// WordSummary panel opens and grows back when it closes
// (bridges/assets/qml/SuttaSearchWindow.qml, `word_summary_wrap`). A user reported the
// column bar staying pinned mid-height after such a close — still anchored to
// the shorter, summary-open viewport, with sutta text painting above and below
// it, and recovering neither on scroll nor on a repeat open/close.
//
// The CSS itself is plain and has no transformed ancestor that could hijack the
// containing block; see docs/mobile-webview-visibility-management.md for why
// the declaration is not the fault. Two fixes are applied together, at
// different levels:
//
//   1. `nudge_webview_geometry()` (SuttaHtmlView_Mobile.qml) — a 1px geometry
//      jiggle, so the native view re-sends a size Chromium must act on. This is
//      the only half that can help if Chromium never learned the new size,
//      because then every quantity JS reads is stale too.
//   2. `force_relayout()` here — re-resolves the page's own layout, for when the
//      size *has* arrived but the containing block or the bar's layer is stale.
//
// We cannot reproduce the bug, so each run logs one greppable `VIEWPORT-NUDGE:`
// line to the backend logger (and so to log.txt). The line is built to answer
// three questions from a single reproduction on the affected device:
//
//   - **What is the root cause?** `qt_h`/`qt_dpr` (the native view's size,
//     passed in by QML) against the page's own `inner`, plus `rs=` — the number
//     of `resize` events the page received. Zero resizes with a grown `qt_h` is
//     "Chromium never acted on the size", stated rather than inferred.
//   - **Which fix helped?** The phases are measured in order with nothing else
//     interleaved: `close` (baseline) → `jiggle` (the QML fix alone) →
//     `relayout` (the JS fix alone) → `settled`. `fixed_by=` names the phase
//     that made it right.
//   - **What can be removed?** A run of `fixed_by=jiggle` lines says the in-page
//     relayout never contributed; a run of `fixed_by=relayout` says the geometry
//     jiggle did not. Either is grounds for deleting the other half.

import * as h from "./helpers";

// A fixed bottom edge more than this far from the viewport bottom counts as a
// mismatch. Sub-pixel rounding on fractional device pixel ratios is expected.
const MISMATCH_PX = 2;

// A last look after the in-page repair, late enough to catch a native resize
// that arrives after everything else has run.
const SETTLE_MS = 700;
// If QML never reports its phases (desktop, where the jiggle is a no-op), finish
// the run anyway rather than leaking it. Comfortably longer than the QML timer
// chain in SuttaHtmlView_Mobile.qml.
const REPORT_TIMEOUT_MS = 2000;
// The jiggle phase is measured once the engine has gone this long without
// delivering a resize, instead of after a fixed delay: measured native delivery
// latency was 90-127 ms, so any fixed constant is a guess about the device.
const QUIET_MS = 100;
const QUIET_POLL_MS = 30;
// Never wait longer than this for quiet — on a device that never resizes at all
// (the failure being chased) it would otherwise never measure.
const QUIET_CAP_MS = 500;

// How far the page's viewport may be from the native view's height, in *device*
// pixels, before they count as disagreeing. The two are derived through
// different device pixel ratios, so a couple of pixels of rounding is expected;
// the discrepancy being chased is hundreds.
const NATIVE_GAP_PX = 8;

interface Measurement {
  phase: string;
  t_ms: number;
  resizes: number;
  inner_height: number;
  client_height: number;
  visual_height: number | null;
  probe_bottom: number | null;
  bar_id: string | null;
  bar_bottom: number | null;
  // The page's own device-pixel ratio at measurement time, so `inner_height`
  // can be converted to device pixels and compared with the native view.
  dpr: number;
}

// `resize` events seen since the page loaded. The *delta* across a run is the
// direct evidence of whether Chromium acted on the native view's resize at all
// — the question no in-page measurement can answer on its own.
let resize_count = 0;
// Wall-clock time of each engine-delivered resize, so a run can report *when*
// they arrived relative to the jiggle. This is what separates "the engine acted
// on its own" from "it only moved after we forced it" — the distinction a bare
// count cannot make.
let resize_times: number[] = [];
// Set while `force_relayout()` dispatches its own synthetic `resize`, so that
// event is not counted. Counting it would destroy the whole point of `rs=`:
// the number has to mean "resizes the engine delivered", not "resizes we
// manufactured".
let dispatching_synthetic_resize = false;
// Runs are numbered so several closes in one session can be told apart in the
// log, and so a missing run is visible.
let run_seq = 0;
// Idempotence guard for init_viewport_nudge(): a second registration would make
// every `resize` count twice and quietly corrupt `rs=`.
let listeners_registered = false;

// The native view's height and device pixel ratio, as Qt sees them, reported by
// `ssp_report_qt_geometry()` from the QML jiggle's restore timer.
//
// It is reported from there, not read at close time, because the QML
// `SplitView` re-lays out in the polish pass: reading `web.height` synchronously
// inside `handle_summary_close()` returns the *summary-open* height, which would
// have made the whole native-vs-page comparison state the opposite of the truth.
// Since the native view's height does not change during a run (bar the 1px
// jiggle), the reported value is treated as a run constant and applied to every
// phase at log time — including phases measured before it arrived.
let qt_height: number | null = null;
let qt_dpr: number | null = null;

/**
 * Bottom-anchored fixed chrome present in the page, most relevant first. Only
 * elements that are actually displayed are of interest — a hidden bar has no
 * meaningful rect and nothing for the user to notice.
 */
function bottom_fixed_bars(): HTMLElement[] {
  const bars: HTMLElement[] = [];
  const column_bar = document.getElementById("columnBar");
  if (column_bar && column_bar.classList.contains("show")) {
    bars.push(column_bar);
  }
  const footnote_bar = document.getElementById("footnoteBottomBar");
  if (footnote_bar && footnote_bar.classList.contains("show")) {
    bars.push(footnote_bar);
  }
  return bars;
}

/**
 * Where the page currently thinks the viewport bottom is, measured with a
 * freshly created fixed element (so it cannot itself be a stale layer).
 * Returns null if the DOM is not usable.
 */
function probe_viewport_bottom(): number | null {
  if (!document.body) {
    return null;
  }
  const probe = document.createElement("div");
  probe.style.cssText =
    "position:fixed;bottom:0;left:0;width:1px;height:1px;visibility:hidden;pointer-events:none;";
  document.body.appendChild(probe);
  try {
    return probe.getBoundingClientRect().bottom;
  } finally {
    probe.remove();
  }
}

function measure(phase: string, t_start: number): Measurement {
  const bars = bottom_fixed_bars();
  const bar = bars.length > 0 ? bars[0] : null;
  const visual = (window as any).visualViewport;
  return {
    phase,
    t_ms: Math.round(Date.now() - t_start),
    resizes: resize_count,
    inner_height: window.innerHeight,
    client_height: document.documentElement
      ? document.documentElement.clientHeight
      : 0,
    visual_height: visual ? visual.height : null,
    probe_bottom: probe_viewport_bottom(),
    bar_id: bar ? bar.id : null,
    bar_bottom: bar ? bar.getBoundingClientRect().bottom : null,
    dpr: window.devicePixelRatio || 1,
  };
}

/**
 * How far the page's viewport is from the native view's height, in device
 * pixels, or null when Qt has not reported its geometry (desktop, or the report
 * never arrived). This — not `mismatch_px` — is what detects the failure we
 * believe is happening: when the whole page is laid out against the old height,
 * the bar, the probe and `innerHeight` all agree with each other and only
 * disagree with the view they are drawn into.
 */
function native_gap_px(m: Measurement): number | null {
  if (qt_height === null || !qt_dpr || !m.dpr) {
    return null;
  }
  return Math.abs(m.inner_height * m.dpr - qt_height * qt_dpr);
}

/**
 * How far the measured bottoms are from the viewport bottom. A positive number
 * is the size of the discrepancy in px; 0 means everything agrees (or there was
 * nothing measurable).
 */
function mismatch_px(m: Measurement): number {
  let worst = 0;
  if (m.probe_bottom !== null) {
    worst = Math.max(worst, Math.abs(m.inner_height - m.probe_bottom));
  }
  if (m.bar_bottom !== null) {
    worst = Math.max(worst, Math.abs(m.inner_height - m.bar_bottom));
  }
  return worst;
}

/**
 * A phase is healthy when the page is internally consistent *and* agrees with
 * the view it is drawn into. Both halves are needed: the first alone would call
 * the suspected failure "ok" (everything in the page agrees, at the wrong
 * height), and the second alone would miss a stale bar layer.
 */
function is_ok(m: Measurement): boolean {
  if (mismatch_px(m) > MISMATCH_PX) {
    return false;
  }
  const gap = native_gap_px(m);
  return gap === null || gap <= NATIVE_GAP_PX;
}

function num(value: number | null): string {
  return value === null ? "n/a" : String(Math.round(value));
}

/**
 * One phase as a compact block. `rs` is the running `resize` count, so the
 * phase at which the page finally heard about the resize is visible.
 */
function format(m: Measurement, resizes_at_start: number): string {
  const parts = [
    `t=${m.t_ms}`,
    `rs=${m.resizes - resizes_at_start}`,
    `inner=${num(m.inner_height)}`,
    `client=${num(m.client_height)}`,
    `visual=${num(m.visual_height)}`,
    `probe=${num(m.probe_bottom)}`,
    `bar_bottom=${num(m.bar_bottom)}`,
    `off=${Math.round(mismatch_px(m))}`,
    // Device-pixel gap between the page's viewport and the native view. This is
    // the field to read first for the root cause.
    `vgap=${num(native_gap_px(m))}`,
  ];
  return `${m.phase}[${parts.join(" ")}]`;
}

/**
 * Force the page to re-resolve its layout viewport and repaint the
 * bottom-anchored fixed chrome. Every step is a no-op on a page that is already
 * correct, so this is safe to run unconditionally.
 */
export function force_relayout(): void {
  const de = document.documentElement;
  // Captured before anything is touched and restored at the end: steps 1 and 2
  // change the box tree, and if either momentarily shortened the scrollable
  // range the browser would clamp the offset — reading it later would then
  // "restore" the clamped value and lose the reader's place.
  const scroll_y = window.scrollY;

  // 1. Pin the root to the *current* viewport height for one layout pass. The
  //    stylesheet gives html/body `height: 100%`, which is a percentage of a
  //    possibly-stale initial containing block; an explicit px height forces the
  //    box tree to be rebuilt against the height the page reports now. (If that
  //    height is itself stale, this cannot help — which is what the geometry
  //    jiggle on the QML side is for.)
  if (de) {
    const prev_height = de.style.height;
    de.style.height = `${window.innerHeight}px`;
    void de.offsetHeight; // read back to flush layout
    de.style.height = prev_height;
    void de.offsetHeight;
  }

  // 2. Take each bottom bar out of the box tree and put it back, so a stale
  //    composited layer is discarded. Clearing the inline value restores the
  //    class-driven display (`.column-bar.show { display: flex }`).
  for (const bar of bottom_fixed_bars()) {
    const prev_display = bar.style.display;
    bar.style.display = "none";
    void bar.offsetHeight;
    bar.style.display = prev_display;
    void bar.offsetHeight;
  }

  // 3. A 1px scroll and back: on Android WebView a scroll is what reliably makes
  //    Chromium re-position fixed elements. Restores the offset captured above.
  //    Nothing in the page listens for `scroll` (checked: only sbs_blocks
  //    listens for `resize`, and the footnote bar uses an IntersectionObserver
  //    whose callback is async and sees only the restored state), and no
  //    stylesheet sets `scroll-behavior: smooth`, so this cannot animate or
  //    cascade.
  window.scrollTo(0, scroll_y + 1);
  window.scrollTo(0, scroll_y);

  // 4. Let anything that sizes itself from the viewport recompute (the
  //    block-fallback panes in sbs_blocks.ts listen for this). Harmless when
  //    nothing changed — those handlers are idempotent. Flagged so our own
  //    event does not inflate the `rs=` count.
  dispatching_synthetic_resize = true;
  try {
    window.dispatchEvent(new Event("resize"));
  } finally {
    dispatching_synthetic_resize = false;
  }
}

interface Run {
  seq: number;
  reason: string;
  t_start: number;
  resizes_at_start: number;
  qt_height_at_call?: number;
  close: Measurement;
  natural: Measurement | null;
  jiggle: Measurement | null;
  relayout: Measurement | null;
  done: boolean;
  // Whether any phase report arrived from QML at all. False on desktop, where
  // the geometry nudge is a no-op and nothing reports.
  reported: boolean;
  // Set when the user's next action cut the observation short — the summary was
  // re-opened, or another close started — so no verdict is drawn from it.
  superseded: string | null;
}

// Runs awaiting their phase reports from QML. An array rather than a single
// slot because two closes inside half a second overlap; each report is
// delivered to every run still in flight.
let active_runs: Run[] = [];

/**
 * Walk the phases, repair the page, and log one line attributing the outcome.
 *
 * **The phases are driven by QML, not by timers here.** An earlier version
 * measured a `close` baseline and then a post-jiggle phase 150 ms later, and a
 * dry run on a healthy device reported `fixed_by=jiggle` on all 19 closes: at
 * `close` the native resize has not been delivered yet (`rs=0`, `inner` still
 * the summary-open height), so the baseline *always* looks broken, and by the
 * next phase it is always right — whether the jiggle did it or the ordinary
 * resize simply arrived. The two were indistinguishable, so the attribution was
 * a false positive.
 *
 * Hence the `natural` phase, measured *after the ordinary resize has had time
 * to land* and *before the jiggle fires*, and hence QML driving the timing:
 * `SuttaHtmlView_Mobile.qml` calls `ssp_report_qt_geometry(h, dpr, phase)` at
 * `pre_jiggle` and `post_jiggle`, so the two sides cannot drift apart the way
 * two independently maintained constants would.
 *
 * `qt_height_at_call` is the native view's height as QML saw it at the moment of
 * the call — logged as `qt_h0` but never compared against, because the
 * `SplitView` has not re-laid out yet. The dry run confirmed this is real, not
 * theoretical: `qt_h0` was 291 against a settled `qt_h` of 582 on every run.
 */
export function nudge_viewport(
  reason: string,
  qt_height_at_call?: number,
): void {
  const t_start = Date.now();
  const run: Run = {
    seq: ++run_seq,
    reason,
    t_start,
    resizes_at_start: resize_count,
    qt_height_at_call,
    // Baseline: nothing has happened yet — not the engine's own resize, not the
    // jiggle, not the in-page repair. Recorded for the record, never treated as
    // evidence of a fault.
    close: measure("close", t_start),
    natural: null,
    jiggle: null,
    relayout: null,
    done: false,
    reported: false,
    superseded: null,
  };
  // A second close while one is still measuring resizes the webview under it.
  supersede_active_runs("new_run");
  active_runs.push(run);

  // If QML never reports (desktop, where the jiggle is a no-op, or a report that
  // failed to arrive), finish anyway rather than leaking the run.
  window.setTimeout(() => finish_run(run), REPORT_TIMEOUT_MS);
}

/**
 * `pre_jiggle`: the ordinary resize has had its chance and the jiggle has not
 * fired. This phase alone says whether anything was wrong at all.
 */
function on_pre_jiggle(run: Run): void {
  if (run.done || run.natural) {
    return;
  }
  run.reported = true;
  run.natural = measure("natural", run.t_start);
}

/**
 * Measure once the engine has stopped delivering resizes, rather than after a
 * fixed delay.
 *
 * The jiggle is two size changes in quick succession (1px out, then back), and
 * a device run measured the native delivery latency at **90–127 ms** — far more
 * than the 50 ms the QML timer allowed. The result was that 21 of 25 runs
 * measured the `jiggle` phase *between* the two, catching the transient
 * 1px-short state (`inner=581`, `vgap=3`). Harmless there only because the
 * tolerance absorbed it; on a device where the jiggle is what fixes things, a
 * measurement taken mid-jiggle is exactly the one that must not be trusted.
 *
 * Waiting for quiet self-tunes to whatever the device's latency is, which a
 * hand-picked constant cannot.
 */
function measure_after_quiet(
  phase: string,
  run: Run,
  done: (m: Measurement) => void,
): void {
  const started = Date.now();
  const check = (): void => {
    if (run.done) {
      return;
    }
    const last_resize = resize_times.length
      ? resize_times[resize_times.length - 1]
      : 0;
    const quiet = Date.now() - last_resize >= QUIET_MS;
    const capped = Date.now() - started >= QUIET_CAP_MS;
    if (quiet || capped) {
      done(measure(phase, run.t_start));
      return;
    }
    window.setTimeout(check, QUIET_POLL_MS);
  };
  check();
}

/**
 * `post_jiggle`: the jiggle has landed and been restored. Measure it, then run
 * the in-page repair and measure that, so the two fixes stay separable.
 */
function on_post_jiggle(run: Run): void {
  if (run.done || run.jiggle) {
    return;
  }
  run.reported = true;
  measure_after_quiet("jiggle", run, (m) => {
    run.jiggle = m;
    apply_relayout(run);
  });
}

function apply_relayout(run: Run): void {
  try {
    // Unconditional: a discrepancy that lives only in the compositor — a fixed
    // layer pinned to the old viewport bottom while the main thread's layout is
    // correct — is invisible to `getBoundingClientRect()`, so acting only on a
    // measured mismatch would never address that variant.
    force_relayout();
  } catch (error) {
    h.log_error(
      `VIEWPORT-NUDGE: run=${run.seq} reason=${run.reason} relayout failed: ${error}`,
    );
  }
  run.relayout = measure("relayout", run.t_start);
  window.setTimeout(() => finish_run(run), SETTLE_MS);
}

/**
 * End every run still in flight without drawing a verdict from it. Called when
 * the user re-opens the summary or closes it again: either resizes the webview
 * under a run that is still measuring, and a device run showed this polluting
 * the `settled` phase in 9 of 25 cases (it read the *summary-open* height, with
 * a `vgap` of 552–873). The verdict survived only because an earlier phase had
 * already resolved — on the affected device that luck runs out and the run
 * would report a fault the user never saw.
 */
function supersede_active_runs(cause: string): void {
  // Note `new_run` is a safety net that a device run showed never fires in
  // practice: a second close is always preceded by an open, which supersedes
  // first. Kept because the cost is one line and the alternative is a run
  // measuring across a resize nobody accounted for.
  for (const run of active_runs.slice()) {
    run.superseded = cause;
    finish_run(run);
  }
}

function finish_run(run: Run): void {
  if (run.done) {
    return;
  }
  run.done = true;
  active_runs = active_runs.filter((r) => r !== run);

  const settled = measure("settled", run.t_start);

  // The first phase at which the page agreed with the view it is drawn into.
  // `natural` means neither fix was needed — the expected reading on a healthy
  // device, and the baseline against which the affected one is judged.
  //
  // A superseded run keeps every phase it managed to capture: those were taken
  // before the user's next action moved the webview, so they are as good as any
  // other run's. Only `settled` is discarded, since it is the one measured
  // after the interruption — that is the phase a device run caught reading the
  // *summary-open* height, with a `vgap` of 552-873.
  let resolved_at: string;
  if (run.natural && is_ok(run.natural)) {
    resolved_at = "natural";
  } else if (run.jiggle && is_ok(run.jiggle)) {
    resolved_at = "jiggle";
  } else if (run.relayout && is_ok(run.relayout)) {
    resolved_at = "relayout";
  } else if (!run.superseded && is_ok(settled)) {
    resolved_at = "late";
  } else {
    // Nothing resolved it before the interruption. Whether it *would* have is
    // unknowable, so this is reported as aborted rather than as a fault.
    resolved_at = run.superseded ? "unknown" : "never";
  }

  // No phase report ever arrived, so no phase was measured and — since the
  // in-page repair runs on the post_jiggle report — nothing was repaired
  // either. This is the desktop path, where `nudge_webview_geometry()` is a
  // no-op. Without this the chain above falls through to "settled is fine" and
  // logs `result=fixed resolved_at=late` on every desktop close: a fix credited
  // for work nobody did.
  //
  // An interruption takes precedence over it, because a run superseded before
  // its first report is explained by the interruption, not by the platform.
  if (!run.reported && !run.superseded) {
    resolved_at = "n/a";
  }

  const result = run.superseded
    ? resolved_at === "unknown"
      ? "aborted"
      : resolved_at === "natural"
        ? "ok"
        : "fixed"
    : !run.reported
      ? "unreported"
      : resolved_at === "never"
        ? "persists"
        : resolved_at === "natural"
          ? "ok"
          : "fixed";

  // Offsets, in ms from the start of the run, at which the engine delivered a
  // resize. Read against the phase timestamps this says outright whether the
  // engine acted on its own or only after the jiggle.
  const rs_at = resize_times
    .filter((t) => t >= run.t_start)
    .map((t) => Math.round(t - run.t_start));

  // A superseded run's `settled` is kept in the line but renamed, so nobody
  // reads it as a measurement of the state the reader was left in.
  if (run.superseded) {
    settled.phase = "settled_after_interrupt";
  }
  const phases = [run.close, run.natural, run.jiggle, run.relayout, settled]
    .filter((m): m is Measurement => m !== null)
    .map((m) => format(m, run.resizes_at_start))
    .join(" ");

  const msg =
    `VIEWPORT-NUDGE: run=${run.seq} reason=${run.reason} result=${result} resolved_at=${resolved_at} ` +
    (run.superseded ? `superseded=${run.superseded} ` : "") +
    `qt_h=${num(qt_height)} qt_dpr=${qt_dpr === null ? "n/a" : qt_dpr} ` +
    `qt_h0=${run.qt_height_at_call === undefined ? "n/a" : Math.round(run.qt_height_at_call)} ` +
    `dpr=${window.devicePixelRatio} scroll_h=${Math.round(document.documentElement ? document.documentElement.scrollHeight : 0)} ` +
    `bar=${run.close.bar_id === null ? "none" : run.close.bar_id} rs_at=[${rs_at.join(",")}] ` +
    phases;

  // Always logged, `result=ok` included: a user reporting a stranded bar over a
  // run of `ok` lines is itself the diagnosis, and silence would look like the
  // hook never ran.
  if (result === "persists") {
    h.log_error(msg);
  } else {
    h.log_info(msg);
  }
}

/**
 * Install the QML-facing hooks and the `resize` bookkeeping.
 *
 * `window.word_summary_closed(qt_h0)` is called from
 * `word_summary_wrap.handle_summary_close()` in SuttaSearchWindow.qml — the
 * moment the reader's WebView is given its height back.
 *
 * `window.ssp_report_qt_geometry(height, dpr, phase)` is called twice per close
 * from the QML timer chain in `SuttaHtmlView_Mobile.qml`: at `pre_jiggle`, once
 * the ordinary resize has had time to land and before the jiggle fires, and at
 * `post_jiggle`, once the jiggle has been restored. QML owns the timing so the
 * two sides cannot drift apart.
 */
export function init_viewport_nudge(): void {
  if (!listeners_registered) {
    listeners_registered = true;
    window.addEventListener("resize", () => {
      if (!dispatching_synthetic_resize) {
        resize_count += 1;
        resize_times.push(Date.now());
        // Unbounded growth would be a slow leak in a long reading session; only
        // the current run's entries are ever read.
        if (resize_times.length > 64) {
          resize_times = resize_times.slice(-32);
        }
      }
    });
  }

  (window as any).word_summary_closed = (qt_height_at_call?: number) =>
    nudge_viewport("word_summary_closed", qt_height_at_call);
  (window as any).ssp_viewport_nudge = (reason?: string) =>
    nudge_viewport(reason || "manual");

  // Called when the WordSummary is (re-)opened. The webview shrinks again, so
  // any run still measuring must stop rather than record the new, smaller
  // viewport as a fault.
  (window as any).ssp_word_summary_opened = () =>
    supersede_active_runs("summary_reopened");

  (window as any).ssp_report_qt_geometry = (
    height: number,
    dpr: number,
    phase?: string,
  ) => {
    qt_height = typeof height === "number" && height > 0 ? height : null;
    qt_dpr = typeof dpr === "number" && dpr > 0 ? dpr : null;
    // Delivered to every run still in flight: two closes within half a second
    // overlap, and each needs the phase marked.
    for (const run of active_runs.slice()) {
      if (phase === "post_jiggle") {
        on_post_jiggle(run);
      } else {
        on_pre_jiggle(run);
      }
    }
  };
}

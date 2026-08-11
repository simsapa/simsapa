/**
 * Tests for the viewport re-resolution helper (src-ts/viewport_nudge.ts).
 *
 * jsdom does no layout: every element's `getBoundingClientRect()` returns
 * zeroes, so a "stale viewport" is simulated by stubbing the rect of the bottom
 * bar and of the throwaway probe element that `measure()` appends to the body.
 * The point of the tests is the decision and attribution logic — does it repair,
 * and does the log name the right phase — not the browser behaviour the repair
 * relies on.
 *
 * The attribution is the part worth guarding, and it has already been wrong
 * once: with no `natural` phase, a healthy Galaxy S23 credited the geometry
 * jiggle on all 19 measured closes, because the baseline was taken before the
 * ordinary resize had been delivered. `resolved_at=natural` on a page that was
 * never broken is the case that regression-guards that.
 */

import * as h from "./helpers";
import * as vn from "./viewport_nudge";

jest.mock("./helpers", () => ({
  log_info: jest.fn(),
  log_error: jest.fn(),
}));

const INNER_HEIGHT = 800;

/**
 * Make every fixed-bottom rect report `bottom`. The probe is created and
 * removed inside `measure()`, so the stub goes on the prototype and keys off
 * the element rather than on a specific node.
 */
function stub_bottoms(probe_bottom: number, bar_bottom: number): void {
  HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
    const bottom = this.id === "" ? probe_bottom : bar_bottom;
    return { top: 0, left: 0, right: 0, bottom, width: 1, height: 1, x: 0, y: 0,
             toJSON: () => ({}) } as DOMRect;
  };
}

/**
 * Report Qt's geometry the way the QML timer chain does. Heights are in Qt
 * logical px; with both device pixel ratios at 1 they compare directly with
 * `window.innerHeight`. The phase argument is what drives the page's
 * measurements — `pre_jiggle` before the jiggle fires, `post_jiggle` after it
 * has been restored.
 */
function report(
  phase: "pre_jiggle" | "post_jiggle",
  height = INNER_HEIGHT,
  dpr = 1,
): void {
  (window as any).ssp_report_qt_geometry(height, dpr, phase);
}

function last_log(mock: jest.Mock): string {
  expect(mock).toHaveBeenCalled();
  return mock.mock.calls[mock.mock.calls.length - 1][0] as string;
}

describe("nudge_viewport", () => {
  beforeEach(() => {
    jest.useFakeTimers();
    jest.clearAllMocks();
    Object.defineProperty(window, "innerHeight", {
      value: INNER_HEIGHT,
      configurable: true,
    });
    Object.defineProperty(window, "devicePixelRatio", {
      value: 1,
      configurable: true,
    });
    document.body.innerHTML = `
      <div class="column-bar show" id="columnBar"></div>
      <div id="footnoteBottomBar" class="footnote-bottom-bar"></div>`;
    window.scrollTo = jest.fn() as any;
    vn.init_viewport_nudge();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  test("reports 'ok' and credits nothing when the ordinary resize did the work", () => {
    // The healthy case, and the regression guard for the false positive that a
    // device run exposed: by the pre-jiggle report everything already agrees,
    // so neither fix may be credited.
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT);

    vn.nudge_viewport("word_summary_closed");
    report("pre_jiggle");
    report("post_jiggle");
    jest.runAllTimers();

    expect(h.log_error).not.toHaveBeenCalled();
    const msg = last_log(h.log_info as jest.Mock);
    expect(msg).toContain("result=ok");
    expect(msg).toContain("resolved_at=natural");
  });

  test("runs the relayout even when no mismatch is measurable", () => {
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT);

    vn.nudge_viewport("word_summary_closed");
    report("pre_jiggle");
    report("post_jiggle");
    jest.runAllTimers();

    // The scroll nudge is the last step of force_relayout(); its presence is
    // how we know the sequence ran on a page that measured correct. A
    // compositor-only fault cannot be measured, so it must run regardless.
    expect(window.scrollTo).toHaveBeenCalled();
  });

  test("credits the jiggle only when the page was still wrong before it fired", () => {
    stub_bottoms(INNER_HEIGHT, 500);
    vn.nudge_viewport("word_summary_closed");
    report("pre_jiggle"); // still wrong here — the ordinary resize did not do it
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT);
    report("post_jiggle");
    jest.runAllTimers();

    const msg = last_log(h.log_info as jest.Mock);
    expect(msg).toContain("result=fixed");
    expect(msg).toContain("resolved_at=jiggle");
  });

  test("credits the relayout when only it changes anything", () => {
    stub_bottoms(INNER_HEIGHT, 500);
    // The relayout's scroll step stands in for the browser reacting to
    // force_relayout(), which runs right after the post_jiggle measurement.
    window.scrollTo = jest.fn(() => {
      stub_bottoms(INNER_HEIGHT, INNER_HEIGHT);
    }) as any;

    vn.nudge_viewport("word_summary_closed");
    report("pre_jiggle");
    report("post_jiggle");
    jest.runAllTimers();

    const msg = last_log(h.log_info as jest.Mock);
    expect(msg).toContain("result=fixed");
    expect(msg).toContain("resolved_at=relayout");
  });

  test("credits neither fix for a late recovery", () => {
    stub_bottoms(INNER_HEIGHT, 500);
    vn.nudge_viewport("word_summary_closed");
    report("pre_jiggle");
    report("post_jiggle");
    // Comes right only after both fixes have been measured.
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT);
    jest.runAllTimers();

    const msg = last_log(h.log_info as jest.Mock);
    expect(msg).toContain("resolved_at=late");
  });

  test("reports 'persists' at error level when nothing helps", () => {
    stub_bottoms(INNER_HEIGHT, 500);

    vn.nudge_viewport("word_summary_closed", 320);
    report("pre_jiggle");
    report("post_jiggle");
    jest.runAllTimers();

    expect(h.log_info).not.toHaveBeenCalled();
    const msg = last_log(h.log_error as jest.Mock);
    expect(msg).toContain("result=persists");
    expect(msg).toContain("resolved_at=never");
    // `qt_h0` is the unsettled height read at call time, kept only as evidence
    // that the deferred report is needed — on device it read 291 against a
    // settled 582.
    expect(msg).toContain("qt_h=800");
    expect(msg).toContain("qt_h0=320");
    // All five phases present.
    for (const phase of ["close[", "natural[", "jiggle[", "relayout[", "settled["]) {
      expect(msg).toContain(phase);
    }
    expect(msg).toContain("off=300");
  });

  test("detects the suspected failure, where the page is self-consistent at the wrong height", () => {
    // Chromium never acted on the resize: `innerHeight`, the probe and the bar
    // all agree with each other and all disagree with the native view. An
    // in-page-only check calls this "ok", which is the trap this guards.
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT);

    vn.nudge_viewport("word_summary_closed");
    report("pre_jiggle", 1600);
    report("post_jiggle", 1600);
    jest.runAllTimers();

    const msg = last_log(h.log_error as jest.Mock);
    expect(msg).toContain("result=persists");
    expect(msg).toContain("off=0");
    expect(msg).toContain("vgap=800");
  });

  test("waits for the resize storm to end before measuring the jiggle phase", () => {
    // A device run measured native resize delivery at 90-127 ms, so a fixed
    // delay caught the transient 1px-short state in 21 of 25 closes. The phase
    // must be measured only once the engine has gone quiet.
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT);
    vn.nudge_viewport("word_summary_closed");
    report("pre_jiggle");

    // The restore's resize is still arriving when post_jiggle is reported.
    report("post_jiggle");
    window.dispatchEvent(new Event("resize"));
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT - 1); // transient, mid-jiggle
    jest.advanceTimersByTime(60);
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT); // settled
    jest.runAllTimers();

    const msg = last_log(h.log_info as jest.Mock);
    // The jiggle phase must have been taken after the transient passed.
    const jiggle = /jiggle\[[^\]]*bar_bottom=(\d+)/.exec(msg);
    expect(jiggle).not.toBeNull();
    expect(Number(jiggle![1])).toBe(INNER_HEIGHT);
  });

  test("keeps the verdict of an interrupted run whose phases had already resolved", () => {
    // The re-open shrinks the webview again, and a device run showed this
    // polluting the settled phase in 9 of 25 closes — but the phases captured
    // *before* the interruption are as good as any other run's, and 11 of 25
    // runs in the following session were interrupted. Discarding those would
    // throw away nearly half the evidence.
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT);
    vn.nudge_viewport("word_summary_closed");
    report("pre_jiggle");
    report("post_jiggle");
    jest.advanceTimersByTime(200);

    // The re-open makes the page short again — exactly the pollution that must
    // not reach the verdict.
    stub_bottoms(300, 300);
    (window as any).ssp_word_summary_opened();
    jest.runAllTimers();

    const msg = last_log(h.log_info as jest.Mock);
    expect(msg).toContain("result=ok");
    expect(msg).toContain("resolved_at=natural");
    expect(msg).toContain("superseded=summary_reopened");
    // The post-interruption measurement is kept but renamed, so it cannot be
    // read as the state the reader was left in.
    expect(msg).toContain("settled_after_interrupt[");
    expect(h.log_error).not.toHaveBeenCalled();
  });

  test("withholds a verdict when the interruption came before anything resolved", () => {
    stub_bottoms(INNER_HEIGHT, 500);
    vn.nudge_viewport("word_summary_closed");
    (window as any).ssp_word_summary_opened();
    jest.runAllTimers();

    const msg = last_log(h.log_info as jest.Mock);
    expect(msg).toContain("result=aborted");
    expect(msg).toContain("resolved_at=unknown");
    // Never an error: we cannot tell whether it would have resolved.
    expect(h.log_error).not.toHaveBeenCalled();
  });

  test("aborts an in-flight run when another close starts", () => {
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT);
    vn.nudge_viewport("word_summary_closed");
    jest.advanceTimersByTime(50);
    vn.nudge_viewport("word_summary_closed");
    report("pre_jiggle");
    report("post_jiggle");
    jest.runAllTimers();

    const calls = (h.log_info as jest.Mock).mock.calls.map((c) => c[0] as string);
    expect(calls[0]).toContain("superseded=new_run");
    expect(calls[1]).toContain("result=ok");
  });

  test("records when the engine delivered resizes, not just how many", () => {
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT);

    vn.nudge_viewport("word_summary_closed");
    window.dispatchEvent(new Event("resize"));
    report("pre_jiggle");
    report("post_jiggle");
    jest.runAllTimers();

    const msg = last_log(h.log_info as jest.Mock);
    // One engine resize; the relayout's own synthetic one must not appear,
    // since `rs_at` has to mean "resizes the engine delivered".
    expect(msg).toMatch(/rs_at=\[\d+\]/);
  });

  test("credits nothing when QML never reports (desktop)", () => {
    // On desktop the geometry nudge is a no-op, so no phase is measured and the
    // in-page repair — which runs on the post_jiggle report — never executes.
    // Before this was handled, the chain fell through to "settled is fine" and
    // logged `result=fixed resolved_at=late` on every desktop close: a fix
    // credited for work nobody did.
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT);

    vn.nudge_viewport("word_summary_closed");
    jest.runAllTimers();

    const msg = last_log(h.log_info as jest.Mock);
    expect(msg).toContain("result=unreported");
    expect(msg).toContain("resolved_at=n/a");
    expect(msg).toContain("close[");
    expect(msg).not.toContain("natural[");
    expect(msg).not.toContain("jiggle[");
    // Nothing was repaired, so the relayout must not have run.
    expect(window.scrollTo).not.toHaveBeenCalled();
    expect(h.log_error).not.toHaveBeenCalled();
  });

  test("skips the native comparison when Qt reports nothing usable", () => {
    (window as any).ssp_report_qt_geometry(0, 0, "pre_jiggle");
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT);

    vn.nudge_viewport("word_summary_closed");
    report("pre_jiggle", 0, 0);
    report("post_jiggle", 0, 0);
    jest.runAllTimers();

    const msg = last_log(h.log_info as jest.Mock);
    expect(msg).toContain("qt_h=n/a");
    expect(msg).toContain("vgap=n/a");
  });

  test("ignores a hidden column bar and measures only the viewport probe", () => {
    document.body.innerHTML = `<div class="column-bar" id="columnBar"></div>`;
    stub_bottoms(500, 500);

    vn.nudge_viewport("test");
    report("pre_jiggle");
    report("post_jiggle");
    jest.runAllTimers();

    const msg = last_log(h.log_error as jest.Mock);
    expect(msg).toContain("bar=none");
    expect(msg).toContain("bar_bottom=n/a");
    expect(msg).toContain("probe=500");
  });

  test("removes the probe element it appends", () => {
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT);
    const before = document.body.childElementCount;

    vn.nudge_viewport("test");
    report("pre_jiggle");
    report("post_jiggle");
    jest.runAllTimers();

    expect(document.body.childElementCount).toBe(before);
  });

  test("keeps two overlapping closes separate", () => {
    stub_bottoms(INNER_HEIGHT, INNER_HEIGHT);

    vn.nudge_viewport("word_summary_closed");
    vn.nudge_viewport("word_summary_closed");
    report("pre_jiggle");
    report("post_jiggle");
    jest.runAllTimers();

    const calls = (h.log_info as jest.Mock).mock.calls.map((c) => c[0] as string);
    const seqs = calls
      .map((m) => /run=(\d+)/.exec(m))
      .filter((m): m is RegExpExecArray => m !== null)
      .map((m) => Number(m[1]));
    expect(seqs.length).toBe(2);
    expect(seqs[1]).toBe(seqs[0] + 1);
  });
});

describe("init_viewport_nudge", () => {
  test("installs the QML-facing hooks", () => {
    vn.init_viewport_nudge();
    expect(typeof (window as any).word_summary_closed).toBe("function");
    expect(typeof (window as any).ssp_viewport_nudge).toBe("function");
    expect(typeof (window as any).ssp_report_qt_geometry).toBe("function");
    expect(typeof (window as any).ssp_word_summary_opened).toBe("function");
  });
});

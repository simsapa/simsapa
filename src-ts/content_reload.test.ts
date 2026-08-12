/**
 * Tests for the content-block re-render module (src-ts/content_reload.ts):
 * URL building, swap + re-init sequence, error handling.
 */

import * as cr from "./content_reload";
import * as ds from "./display_settings";

describe("build_content_block_url", () => {
  beforeEach(() => {
    (globalThis as any).API_URL = "http://localhost:4848";
  });

  test("encodes uids and keeps the pipe separator", () => {
    const url = cr.build_content_block_url(
      "mn1/en/sujato",
      "sidebyside",
      ["mn1/en/sujato", "mn1/pli/ms"],
      false,
    );
    expect(url).toBe(
      "http://localhost:4848/sutta_content_block"
      + "?uid=mn1%2Fen%2Fsujato"
      + "&layout=sidebyside"
      + "&columns=mn1%2Fen%2Fsujato|mn1%2Fpli%2Fms"
      + "&show_references=false"
      + "&repeat_pali=off",
    );
  });

  test("includes show_references=true", () => {
    const url = cr.build_content_block_url("mn1/en/sujato", "linebyline", [], true);
    expect(url).toContain("&show_references=true");
    expect(url).toContain("&columns=&");
  });

  test("includes the repeat_pali parameter", () => {
    const url = cr.build_content_block_url("mn1/en/sujato", "sidebyside", [], false, "atend");
    expect(url).toContain("&repeat_pali=atend");
  });
});

describe("fetch_content_block", () => {
  let fetch_mock: jest.Mock;
  let rebind_mock: jest.Mock;

  beforeEach(() => {
    ds.reset_module_state_for_tests();
    document.body.innerHTML = "<div id='ssp_content'><p>old</p></div>";
    (globalThis as any).API_URL = "http://localhost:4848";
    (globalThis as any).SUTTA_UID = "mn1/en/sujato";
    (globalThis as any).SUTTA_DISPLAY = {
      layout: "linebyline",
      columns: [
        { uid: "mn1/en/sujato", label: "sujato", author: "sujato", is_pali: false },
        { uid: "mn1/pli/ms", label: "Pāli", author: "pali", is_pali: true },
      ],
      show_references: false,
      defaults: null,
    };
    rebind_mock = jest.fn();
    (globalThis as any).ssp_rebind_content_handlers = rebind_mock;
    document.SSP = {
      attach_link_handlers: jest.fn(),
      show_bottom_footnotes: false,
      find: { hide: jest.fn() },
    };
    (window as any).scrollTo = jest.fn();
    // helpers.log_error fire-and-forgets a POST to /logger; that call must
    // always succeed or it becomes an unhandled rejection. Per-test failure
    // modes are set via mock_content_response below.
    fetch_mock = jest.fn();
    set_content_response({
      ok: true,
      status: 200,
      headers: columns_header([
        { uid: "mn1/en/sujato", label: "sujato", author: "sujato", is_pali: false },
        { uid: "mn1/pli/ms", label: "Pāli", author: "pali", is_pali: true },
      ]),
      text: async () => "<div class='suttacentral bilara-text layout-columns cols-2'>new</div>",
    });
    (globalThis as any).fetch = fetch_mock;
  });

  afterEach(() => {
    document.getElementById("sspDropNotice")?.remove();
  });

  // The X-SSP-Columns response header: percent-encoded JSON, as the Rust
  // route emits it.
  function columns_header(cols: any[] | null): any {
    const encoded = cols === null ? null : encodeURIComponent(JSON.stringify(cols));
    return { get: (name: string) => (name === "X-SSP-Columns" ? encoded : null) };
  }

  function set_content_response(response: any, reject?: Error): void {
    fetch_mock.mockImplementation((url: any) => {
      if (String(url).includes("/logger")) {
        return Promise.resolve({ ok: true, status: 200, text: async () => "" });
      }
      if (reject) {
        return Promise.reject(reject);
      }
      return Promise.resolve(response);
    });
  }

  test("swaps #ssp_content and runs the re-init sequence", async () => {
    const ok = await cr.fetch_content_block("sidebyside", ["mn1/en/sujato", "mn1/pli/ms"], false);
    expect(ok).toBe(true);

    const content = document.getElementById("ssp_content")!;
    expect(content.innerHTML).toContain("layout-columns cols-2");
    expect(document.SSP.attach_link_handlers).toHaveBeenCalled();
    expect(document.SSP.find.hide).toHaveBeenCalled();
    expect(rebind_mock).toHaveBeenCalled();
    expect((window as any).scrollTo).toHaveBeenCalled();
    // The injected page state follows the new render parameters.
    expect((globalThis as any).SUTTA_DISPLAY.layout).toBe("sidebyside");
  });

  test("keeps current content on a non-200 response", async () => {
    set_content_response({ ok: false, status: 404, text: async () => "not found" });
    const ok = await cr.fetch_content_block("sidebyside", ["bad/uid"], false);
    expect(ok).toBe(false);

    const content = document.getElementById("ssp_content")!;
    expect(content.innerHTML).toContain("old");
    expect(rebind_mock).not.toHaveBeenCalled();
    expect((globalThis as any).SUTTA_DISPLAY.layout).toBe("linebyline");
  });

  test("keeps current content when fetch rejects", async () => {
    set_content_response(null, new Error("connection refused"));
    const ok = await cr.fetch_content_block("sidebyside", [], false);
    expect(ok).toBe(false);
    expect(document.getElementById("ssp_content")!.innerHTML).toContain("old");
  });

  test("refetch_with_params uses the current page state", async () => {
    (globalThis as any).SUTTA_DISPLAY.show_references = true;
    const ok = await cr.refetch_with_params("sidebyside", "off");
    expect(ok).toBe(true);

    const url = String(fetch_mock.mock.calls.find((c) =>
      String(c[0]).includes("sutta_content_block"))![0]);
    expect(url).toContain("layout=sidebyside");
    expect(url).toContain("columns=mn1%2Fen%2Fsujato|mn1%2Fpli%2Fms");
    expect(url).toContain("show_references=true");
    expect(url).toContain("repeat_pali=off");
  });

  test("refetch_with_params sends an explicit show_references over the page state", async () => {
    // The page was rendered with references on (an anchor navigation); the
    // reader has just switched them off. The re-fetch must carry the new
    // value, not read the old one back out of SUTTA_DISPLAY.
    (globalThis as any).SUTTA_DISPLAY.show_references = true;
    const ok = await cr.refetch_with_params("linebyline", "off", false);
    expect(ok).toBe(true);

    const url = String(fetch_mock.mock.calls.find((c) =>
      String(c[0]).includes("sutta_content_block"))![0]);
    expect(url).toContain("show_references=false");
    // The post-swap write-back records the new effective value.
    expect((globalThis as any).SUTTA_DISPLAY.show_references).toBe(false);
  });

  test("adopts the server-resolved columns from the X-SSP-Columns header", async () => {
    // The server applied an atend Repeat-Pāli arrangement; the client adopts
    // it verbatim (no client-side mirroring).
    set_content_response({
      ok: true,
      status: 200,
      headers: columns_header([
        { uid: "mn1/pli/ms", label: "Pāli", author: "pali", is_pali: true },
        { uid: "mn1/en/sujato", label: "sujato", author: "sujato", is_pali: false },
        { uid: "mn1/pli/ms", label: "Pāli", author: "pali", is_pali: true },
      ]),
      text: async () => "<div>new</div>",
    });
    const ok = await cr.fetch_content_block("sidebyside", ["mn1/en/sujato", "mn1/pli/ms"], false, "atend");
    expect(ok).toBe(true);

    const sd = (globalThis as any).SUTTA_DISPLAY;
    expect(sd.repeat_pali).toBe("atend");
    expect(sd.columns.map((c: any) => c.uid)).toEqual([
      "mn1/pli/ms", "mn1/en/sujato", "mn1/pli/ms",
    ]);
    // Nothing was dropped: no notice.
    expect(document.getElementById("sspDropNotice")?.classList.contains("show")).toBeFalsy();
  });

  test("keeps the client columns when the header is missing", async () => {
    set_content_response({
      ok: true,
      status: 200,
      headers: columns_header(null),
      text: async () => "<div>new</div>",
    });
    const previous = (globalThis as any).SUTTA_DISPLAY.columns;
    const ok = await cr.fetch_content_block("sidebyside", ["mn1/en/sujato", "mn1/pli/ms"], false);
    expect(ok).toBe(true);
    expect((globalThis as any).SUTTA_DISPLAY.columns).toBe(previous);
  });

  test("a server-dropped column shows the one-line notice", async () => {
    // Lines-mode request includes a non-segmented text; the server drops it.
    (globalThis as any).SUTTA_DISPLAY.columns = [
      { uid: "mn1/en/sujato", label: "sujato", author: "sujato", is_pali: false },
      { uid: "mn1/pli/ms", label: "Pāli", author: "pali", is_pali: true },
      { uid: "mn1/en/horner", label: "horner", author: "horner", is_pali: false },
    ];
    set_content_response({
      ok: true,
      status: 200,
      headers: columns_header([
        { uid: "mn1/en/sujato", label: "sujato", author: "sujato", is_pali: false },
        { uid: "mn1/pli/ms", label: "Pāli", author: "pali", is_pali: true },
      ]),
      text: async () => "<div>new</div>",
    });
    const ok = await cr.fetch_content_block(
      "linebyline",
      ["mn1/en/sujato", "mn1/pli/ms", "mn1/en/horner"],
      false,
    );
    expect(ok).toBe(true);

    const sd = (globalThis as any).SUTTA_DISPLAY;
    expect(sd.columns.map((c: any) => c.uid)).toEqual(["mn1/en/sujato", "mn1/pli/ms"]);
    const notice = document.getElementById("sspDropNotice")!;
    expect(notice.classList.contains("show")).toBe(true);
    // Named by its pre-swap label, not the uid.
    expect(notice.textContent).toContain("horner");
    expect(notice.textContent).toContain("Columns layout");
  });
});

describe("parse_columns_header", () => {
  test("decodes a percent-encoded JSON array", () => {
    const cols = [{ uid: "mn1/pli/ms", label: "Pāli", author: "pali", is_pali: true }];
    expect(cr.parse_columns_header(encodeURIComponent(JSON.stringify(cols)))).toEqual(cols);
  });

  test("null / unparsable / non-array values return null", () => {
    expect(cr.parse_columns_header(null)).toBeNull();
    expect(cr.parse_columns_header("%zz")).toBeNull();
    expect(cr.parse_columns_header("not-json")).toBeNull();
    expect(cr.parse_columns_header(encodeURIComponent("{\"a\":1}"))).toBeNull();
  });
});

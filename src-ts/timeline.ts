// The Timeline page of the Sutta Navigation window: spaces the cluster blocks
// and draws the axis beside them. The markup is written by
// backend/src/navigation_timeline.rs. See docs/sutta-navigation-tree.md.
//
// The cluster blocks stay in normal document flow. This script sets their
// margin-top and positions nothing but the drawing, so text wrapping and the
// find bar's scrolling need no help from it.

import { scaleLinear } from "d3-scale";
import { select, Selection } from "d3-selection";

import {
  bar_lanes, bar_spans, cluster_form, cluster_gaps, extend_scale_points, gutter_geometry,
  latest_to_year, minimap_scroll_target, minimap_view, minimap_y, scale_points, tick_label, tick_years,
  visible_ticks,
  BarSpan, ClusterDates, ClusterForm, GutterGeometry,
} from "./timeline_layout";

/** Page length of a year in "Scale spacing with time" mode. */
const PX_PER_YEAR = 2;
/** Room between two clusters when time does not ask for more. */
const BASE_GAP = 24;
/** Distance between two extent-bar lanes. */
const LANE_WIDTH = 11;
/** Length of a bar that has only one bound. */
const OPEN_BAR_LENGTH = 48;
/** A bounded bar is at least this long, so a short period is still a bar. */
const MIN_BAR_LENGTH = 6;
const MARKER_RADIUS = 6;
/** Room for a year label such as "1200 CE", left of the axis. */
const TICK_LABEL_WIDTH = 46;
const TICK_STEP_YEARS = 100;
const TICK_MIN_DISTANCE = 14;
const TICK_LENGTH = 4;
/** Room between the drawing and the text. */
const GUTTER_PADDING = 10;
/** The drawing never takes more than this share of the page width. */
const GUTTER_MAX_SHARE = 0.3;
const RESIZE_DEBOUNCE_MS = 100;

/** Length of a legend sample's bar: a whole number of dashes, ending on one. */
const LEGEND_BAR_LENGTH = 36;
const LEGEND_SAMPLE_HEIGHT = 14;

/** The minimap is a strip of this width, at most this tall and at most this
 *  share of the viewport height. */
const MINIMAP_WIDTH = 50;
const MINIMAP_MAX_HEIGHT = 340;
const MINIMAP_MAX_VIEWPORT_SHARE = 0.55;
/** x of the minimap's axis, the distance between its lanes, and where the
 *  cluster blocks begin. */
const MINIMAP_AXIS_X = 5;
const MINIMAP_LANE_WIDTH = 3.5;
const MINIMAP_BLOCK_X = 25;
const MINIMAP_VIEW_MIN_HEIGHT = 6;

const SVG_NS = "http://www.w3.org/2000/svg";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type SvgSelection = Selection<any, unknown, null, undefined>;

type Point = [number, number];

let next_def_id = 0;

function new_def_id(name: string): string {
  next_def_id += 1;
  return `timeline_${name}_${next_def_id}`;
}

function read_year(el: HTMLElement, name: string): number | null {
  const value = el.getAttribute(name);
  if (value === null || value === "") {
    return null;
  }
  const year = Number(value);
  return Number.isFinite(year) ? year : null;
}

function read_cluster_dates(el: HTMLElement): ClusterDates | null {
  const sort_year = read_year(el, "data-sort-year");
  if (sort_year === null) {
    return null;
  }
  return {
    from: read_year(el, "data-from"),
    to: read_year(el, "data-to"),
    circa: el.getAttribute("data-circa") === "true",
    sort_year,
  };
}

// ---- Drawing: shared by the axis and the legend, so the two cannot drift apart. ----

/** `color` is the cluster colour, or null for the text colour. */
function append_arrowhead_def(defs: SvgSelection, color: string | null): string {
  const id = new_def_id("arrow");
  defs.append("marker")
    .style("color", color)
    .attr("id", id)
    .attr("viewBox", "0 0 8 8")
    .attr("refX", 7)
    .attr("refY", 4)
    .attr("markerWidth", 11)
    .attr("markerHeight", 11)
    .attr("markerUnits", "userSpaceOnUse")
    .attr("orient", "auto")
    .append("path")
    .attr("class", "timeline-arrowhead")
    .attr("d", "M0,0 L8,4 L0,8 Z");
  return id;
}

function draw_marker(parent: SvgSelection, x: number, y: number, form: ClusterForm,
                     color: string | null): void {
  parent.append("circle")
    .style("color", color)
    .attr("class", form === "dated" ? "timeline-marker" : "timeline-marker timeline-marker-hollow")
    .attr("cx", x)
    .attr("cy", y)
    .attr("r", MARKER_RADIUS);
}

/** An extent bar along `points`. A bar with one bound fades out along its last
 *  segment and ends in an arrowhead. */
function draw_bar(parent: SvgSelection, defs: SvgSelection, points: Point[], form: ClusterForm,
                  color: string | null): void {
  const d = points.map(([x, y], i) => `${i === 0 ? "M" : "L"}${x},${y}`).join(" ");
  const bar = parent.append("path")
    .attr("class", form === "dated" ? "timeline-bar" : "timeline-bar timeline-bar-dashed")
    .style("color", color)
    .attr("d", d);

  if (form !== "open") {
    return;
  }

  // The gradient vector is given in user space: the bounding box of a straight
  // line has no area, so a box-relative gradient would not be drawn at all.
  const [x1, y1] = points[points.length - 2];
  const [x2, y2] = points[points.length - 1];
  const gradient_id = new_def_id("fade");
  // The gradient and the arrowhead are not children of the bar and do not
  // inherit its colour: each gets it directly.
  const gradient = defs.append("linearGradient")
    .attr("id", gradient_id)
    .style("color", color)
    .attr("gradientUnits", "userSpaceOnUse")
    .attr("x1", x1).attr("y1", y1)
    .attr("x2", x2).attr("y2", y2);
  gradient.append("stop").attr("class", "timeline-fade-start").attr("offset", 0);
  gradient.append("stop").attr("class", "timeline-fade-end").attr("offset", 1);

  bar.style("stroke", `url(#${gradient_id})`)
    .attr("marker-end", `url(#${append_arrowhead_def(defs, color)})`);
}

// ---- Legend ----

function draw_legend(): void {
  const items = document.querySelectorAll<HTMLElement>(".timeline-legend [data-form]");
  items.forEach((item) => {
    if (item.querySelector("svg")) {
      return;
    }
    const form = item.getAttribute("data-form") as ClusterForm;
    const svg_el = document.createElementNS(SVG_NS, "svg");
    item.insertBefore(svg_el, item.firstChild);

    const y = LEGEND_SAMPLE_HEIGHT / 2;
    const x = MARKER_RADIUS + 1;
    const bar_start = x + MARKER_RADIUS;
    const bar_end = bar_start + LEGEND_BAR_LENGTH;

    const svg = select(svg_el)
      .attr("class", "timeline-legend-sample")
      .attr("aria-hidden", "true")
      .attr("width", bar_end + 3)
      .attr("height", LEGEND_SAMPLE_HEIGHT);
    const defs = svg.append("defs");

    draw_bar(svg, defs, [[bar_start, y], [bar_end, y]], form, null);
    draw_marker(svg, x, y, form, null);
  });
}

// ---- The button on each entry ----

/** A tree: a root line, a trunk, two branches. */
const NAVIGATION_ICON_PATH = "M2 3h12 M4 3v9 M4 7.5h3 M4 12h3 M9 7.5h5 M9 12h5";

/** Give every entry a button that opens the Sutta Navigation window at its
 *  node. The button is an icon with no text node, so the find bar does not
 *  match it once per entry. */
function add_navigation_buttons(root: HTMLElement): void {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const api_url = (globalThis as any).API_URL || "http://localhost:4848";

  root.querySelectorAll<HTMLElement>(".timeline-entry[data-node-uid]").forEach((entry) => {
    const node_uid = entry.getAttribute("data-node-uid");
    if (!node_uid || entry.querySelector(".timeline-nav-button")) {
      return;
    }
    const button = document.createElement("button");
    button.type = "button";
    button.className = "timeline-nav-button";
    button.title = "Show in Sutta Navigation";
    button.setAttribute("aria-label", button.title);

    const svg_el = document.createElementNS(SVG_NS, "svg");
    select(svg_el)
      .attr("viewBox", "0 0 16 16")
      .attr("aria-hidden", "true")
      .append("path")
      .attr("d", NAVIGATION_ICON_PATH);
    button.appendChild(svg_el);

    button.addEventListener("click", () => {
      fetch(`${api_url}/open_sutta_navigation?node_uid=${encodeURIComponent(node_uid)}`).catch(() => {
        // The app is the server: nothing to do in the page if it is not there.
      });
    });
    entry.insertBefore(button, entry.firstChild);
  });
}

// ---- The axis ----

interface TimelinePage {
  root: HTMLElement;
  body: HTMLElement;
  clusters_el: HTMLElement;
  svg_el: SVGSVGElement;
  toggle: HTMLInputElement | null;
  minimap_toggle: HTMLInputElement | null;
  undated_el: HTMLElement | null;
  cluster_els: HTMLElement[];
  dates: ClusterDates[];
}

function find_page(): TimelinePage | null {
  const root = document.getElementById("timeline");
  if (!root) {
    return null;
  }
  const body = root.querySelector<HTMLElement>(".timeline-body");
  const clusters_el = root.querySelector<HTMLElement>(".timeline-clusters");
  const svg_el = root.querySelector<SVGSVGElement>("svg.timeline-axis");
  if (!body || !clusters_el || !svg_el) {
    return null;
  }

  const cluster_els: HTMLElement[] = [];
  const dates: ClusterDates[] = [];
  clusters_el.querySelectorAll<HTMLElement>(".timeline-cluster").forEach((el) => {
    const cluster_dates = read_cluster_dates(el);
    if (cluster_dates) {
      cluster_els.push(el);
      dates.push(cluster_dates);
    }
  });
  if (cluster_els.length === 0) {
    return null;
  }

  return {
    root, body, clusters_el, svg_el, cluster_els, dates,
    toggle: document.getElementById("timeline_scale_toggle") as HTMLInputElement | null,
    minimap_toggle: document.getElementById("timeline_minimap_toggle") as HTMLInputElement | null,
    undated_el: root.querySelector<HTMLElement>(".timeline-undated"),
  };
}

/** The vertical centre of each cluster's date heading, relative to the body. */
function measure_marker_ys(page: TimelinePage): number[] {
  const body_top = page.body.getBoundingClientRect().top;
  return page.cluster_els.map((el) => {
    const heading = el.querySelector<HTMLElement>(".timeline-period") || el;
    const rect = heading.getBoundingClientRect();
    return rect.top + rect.height / 2 - body_top;
  });
}

interface Measured {
  marker_ys: number[];
  spans: BarSpan[];
  lanes: number[];
  scale: (year: number) => number;
  domain: [number, number];
}

function measure(page: TimelinePage, scaled: boolean): Measured {
  const sort_years = page.dates.map((d) => d.sort_year);
  const heights = page.cluster_els.map((el) => el.offsetHeight);
  const gaps = cluster_gaps(heights, sort_years, scaled, PX_PER_YEAR, BASE_GAP);
  page.cluster_els.forEach((el, i) => {
    el.style.marginTop = `${gaps[i]}px`;
  });

  const marker_ys = measure_marker_ys(page);
  let points = scale_points(sort_years, marker_ys);
  const last_year = latest_to_year(page.dates);
  if (last_year !== null) {
    points = extend_scale_points(points, last_year, page.body.clientHeight);
  }

  let scale: (year: number) => number;
  if (points.years.length < 2) {
    scale = () => points.ys[0];
  } else {
    scale = scaleLinear().domain(points.years).range(points.ys).clamp(true);
  }

  const spans = bar_spans(page.dates, marker_ys, scale, OPEN_BAR_LENGTH, MIN_BAR_LENGTH);
  return {
    marker_ys, spans, scale,
    lanes: bar_lanes(spans),
    domain: [points.years[0], points.years[points.years.length - 1]],
  };
}

function draw_axis(page: TimelinePage, measured: Measured, geometry: GutterGeometry, scaled: boolean): void {
  const height = page.body.clientHeight;
  const svg = select(page.svg_el);
  svg.selectAll("*").remove();
  svg.attr("width", geometry.width).attr("height", height);

  const defs = svg.append("defs");
  const x = geometry.axis_x;
  // The colour the stylesheet gives each cluster, shared with its entries.
  const colors = page.cluster_els.map((el) => {
    return getComputedStyle(el).getPropertyValue("--timeline-color").trim() || null;
  });
  const { marker_ys, spans, lanes } = measured;

  // The lines are cut away inside every marker, so that a hollow marker stays
  // hollow where the axis or another cluster's bar passes behind it. The clip
  // is the whole drawing minus the marker discs (even-odd).
  const clip_id = new_def_id("clip");
  const r = MARKER_RADIUS;
  const discs = marker_ys
    .map((y) => `M${x - r},${y} a${r},${r} 0 1,0 ${2 * r},0 a${r},${r} 0 1,0 ${-2 * r},0 Z`)
    .join(" ");
  defs.append("clipPath")
    .attr("id", clip_id)
    .append("path")
    .attr("clip-rule", "evenodd")
    .attr("d", `M-1000,${-2 * OPEN_BAR_LENGTH} H${geometry.width + 1000} V${height + 2 * OPEN_BAR_LENGTH} H-1000 Z ${discs}`);

  const lines = svg.append("g").attr("clip-path", `url(#${clip_id})`);
  lines.append("line")
    .attr("class", "timeline-axis-line")
    .attr("x1", x).attr("x2", x)
    .attr("y1", Math.min(marker_ys[0], spans[0].y0))
    .attr("y2", height);

  page.dates.forEach((dates, i) => {
    const marker_y = marker_ys[i];
    // The end away from the marker: above it for a cluster with only `to`.
    const end_y = dates.from === null ? spans[i].y0 : spans[i].y1;
    const lane_x = x + lanes[i] * geometry.lane_width;
    const points: Point[] = lanes[i] === 0
      ? [[x, marker_y], [x, end_y]]
      : [[x, marker_y], [lane_x, marker_y], [lane_x, end_y]];
    draw_bar(lines, defs, points, cluster_form(dates), colors[i]);
  });

  // The year labels are not text nodes: the stylesheet shows `data-label` as
  // generated content. The find bar wraps every text node of the page content
  // it matches in a <span>, which inside an SVG <text> would erase the label,
  // and a label is not something to find.
  let labels = page.body.querySelector<HTMLElement>(".timeline-tick-labels");
  if (!labels) {
    labels = document.createElement("div");
    labels.className = "timeline-tick-labels";
    labels.setAttribute("aria-hidden", "true");
    page.body.appendChild(labels);
  }
  labels.textContent = "";

  if (scaled) {
    const years = tick_years(measured.domain[0], measured.domain[1], TICK_STEP_YEARS);
    const ys = years.map((year) => measured.scale(year));
    const visible = visible_ticks(ys, TICK_MIN_DISTANCE);
    const ticks = svg.append("g").attr("class", "timeline-ticks");
    years.forEach((year, i) => {
      if (!visible[i]) {
        return;
      }
      ticks.append("line")
        .attr("x1", x - r - TICK_LENGTH).attr("x2", x - r)
        .attr("y1", ys[i]).attr("y2", ys[i]);
      const label = document.createElement("span");
      label.className = "timeline-tick-label";
      label.setAttribute("data-label", tick_label(year));
      label.style.top = `${ys[i]}px`;
      label.style.width = `${x - r - TICK_LENGTH - 2}px`;
      labels.appendChild(label);
    });
  }

  const markers = svg.append("g");
  page.dates.forEach((dates, i) => {
    draw_marker(markers, x, marker_ys[i], cluster_form(dates), colors[i]);
  });
}

/** The width the last layout was made for. The layout changes the height of
 *  the element the ResizeObserver watches, so a re-layout on every
 *  notification would never end: only a new width is a reason to run again. */
let laid_out_width = -1;
let lane_count = 1;

function layout(page: TimelinePage, force: boolean): void {
  if (!force && page.clusters_el.clientWidth === laid_out_width) {
    return;
  }
  const scaled = page.toggle !== null && page.toggle.checked;

  // Room above the first marker when its bar points upward from it.
  page.body.style.paddingTop = page.dates[0].from === null ? `${OPEN_BAR_LENGTH}px` : "";

  // The gutter width depends on the number of lanes, the lanes on where the
  // blocks are, and that on the text width the gutter leaves. A second pass
  // settles it; the lane count of the previous layout is the first guess.
  let geometry = set_gutter(page, lane_count, scaled);
  let measured = measure(page, scaled);
  const needed = Math.max(...measured.lanes) + 1;
  if (needed !== lane_count) {
    lane_count = needed;
    geometry = set_gutter(page, lane_count, scaled);
    measured = measure(page, scaled);
    // Rewrapped text can move a bar into a lane the gutter was not sized for;
    // it is drawn there anyway and the next layout makes the room.
    lane_count = Math.max(lane_count, Math.max(...measured.lanes) + 1);
  }

  draw_axis(page, measured, geometry, scaled);
  laid_out_width = page.clusters_el.clientWidth;

  last_measured = measured;
  draw_minimap(page);
}

function set_gutter(page: TimelinePage, lanes: number, scaled: boolean): GutterGeometry {
  const geometry = gutter_geometry(
    lanes, scaled ? TICK_LABEL_WIDTH : 0, MARKER_RADIUS, LANE_WIDTH, GUTTER_PADDING,
    page.root.clientWidth * GUTTER_MAX_SHARE,
  );
  page.root.style.setProperty("--timeline-gutter", `${geometry.width}px`);
  return geometry;
}

// ---- The minimap ----
//
// A fixed strip in the corner with the whole page on it: page y maps linearly
// to strip y, so the band that shows the viewport and a click's scroll target
// are the same arithmetic in both directions. Each cluster is a block in its
// colour, as tall as its texts are on the page, beside its extent bar in
// miniature. Nothing in it is a text node.

let last_measured: Measured | null = null;
let minimap_el: HTMLElement | null = null;
let minimap_svg_el: SVGSVGElement | null = null;
let minimap_view_el: HTMLElement | null = null;
let minimap_height = 0;

function minimap_is_on(page: TimelinePage): boolean {
  return page.minimap_toggle !== null && page.minimap_toggle.checked;
}

function page_height(): number {
  return document.documentElement.scrollHeight;
}

function create_minimap(page: TimelinePage): void {
  minimap_el = document.createElement("div");
  minimap_el.className = "timeline-minimap";
  minimap_el.setAttribute("aria-hidden", "true");
  minimap_el.style.width = `${MINIMAP_WIDTH}px`;
  // The stylesheet keeps the page's right margin as wide as the strip.
  page.root.style.setProperty("--timeline-minimap-width", `${MINIMAP_WIDTH}px`);
  // The page background, which a theme sets on the body: the strip lies over
  // the text and has to cover it.
  minimap_el.style.backgroundColor = getComputedStyle(document.body).backgroundColor;

  minimap_svg_el = document.createElementNS(SVG_NS, "svg");
  minimap_el.appendChild(minimap_svg_el);
  minimap_view_el = document.createElement("div");
  minimap_view_el.className = "timeline-minimap-view";
  minimap_el.appendChild(minimap_view_el);
  page.root.appendChild(minimap_el);

  const scroll_to = (event: PointerEvent): void => {
    if (!minimap_el) {
      return;
    }
    const map_y = event.clientY - minimap_el.getBoundingClientRect().top;
    window.scrollTo(0, minimap_scroll_target(map_y, minimap_height, page_height(), window.innerHeight));
  };
  minimap_el.addEventListener("pointerdown", (event) => {
    event.preventDefault();
    // Capture makes a drag keep scrolling when the pointer leaves the strip.
    // Without it a click still works.
    try {
      minimap_el?.setPointerCapture(event.pointerId);
    } catch (_error) {
      // No capture for this pointer.
    }
    scroll_to(event);
  });
  minimap_el.addEventListener("pointermove", (event) => {
    if (minimap_el?.hasPointerCapture(event.pointerId)) {
      scroll_to(event);
    }
  });
}

function update_minimap_view(): void {
  if (!minimap_view_el || minimap_height <= 0) {
    return;
  }
  const view = minimap_view(window.scrollY, window.innerHeight, page_height(), minimap_height,
                            MINIMAP_VIEW_MIN_HEIGHT);
  minimap_view_el.style.top = `${view.top}px`;
  minimap_view_el.style.height = `${view.height}px`;
}

function draw_minimap(page: TimelinePage): void {
  if (!minimap_el || !minimap_svg_el || !last_measured || !minimap_is_on(page)) {
    return;
  }
  const measured = last_measured;
  const total = page_height();
  minimap_height = Math.round(Math.min(MINIMAP_MAX_HEIGHT, window.innerHeight * MINIMAP_MAX_VIEWPORT_SHARE));
  minimap_el.style.height = `${minimap_height}px`;

  const to_map = (y: number): number => minimap_y(y, total, minimap_height);
  const page_top = (el: HTMLElement): number => el.getBoundingClientRect().top + window.scrollY;
  const body_top = page_top(page.body);

  const svg = select(minimap_svg_el);
  svg.selectAll("*").remove();
  svg.attr("width", MINIMAP_WIDTH).attr("height", minimap_height);

  svg.append("line")
    .attr("class", "timeline-axis-line")
    .attr("x1", MINIMAP_AXIS_X).attr("x2", MINIMAP_AXIS_X)
    .attr("y1", to_map(body_top + measured.marker_ys[0]))
    .attr("y2", to_map(body_top + page.body.clientHeight));

  const block = (el: HTMLElement, color: string | null): void => {
    const y0 = to_map(page_top(el));
    const y1 = to_map(page_top(el) + el.offsetHeight);
    svg.append("rect")
      .attr("class", "timeline-minimap-block")
      .style("color", color)
      .attr("x", MINIMAP_BLOCK_X)
      .attr("y", y0)
      .attr("width", MINIMAP_WIDTH - MINIMAP_BLOCK_X - 3)
      .attr("height", Math.max(1, y1 - y0 - 1));
  };

  page.cluster_els.forEach((el, i) => {
    const color = getComputedStyle(el).getPropertyValue("--timeline-color").trim() || null;
    block(el, color);

    const span = measured.spans[i];
    const x = MINIMAP_AXIS_X + measured.lanes[i] * MINIMAP_LANE_WIDTH;
    svg.append("line")
      .attr("class", cluster_form(page.dates[i]) === "dated"
        ? "timeline-minimap-bar" : "timeline-minimap-bar timeline-minimap-bar-dashed")
      .style("color", color)
      .attr("x1", x).attr("x2", x)
      .attr("y1", to_map(body_top + span.y0))
      .attr("y2", to_map(body_top + span.y1));
  });
  if (page.undated_el) {
    block(page.undated_el, null);
  }

  update_minimap_view();
}

/** Show or hide the minimap. Shown, the page keeps a margin for it, which
 *  changes the text width: the caller re-lays out. */
function apply_minimap_toggle(page: TimelinePage): void {
  const on = minimap_is_on(page);
  page.root.classList.toggle("timeline-minimap-on", on);
  if (on && !minimap_el) {
    create_minimap(page);
  }
  if (minimap_el) {
    minimap_el.hidden = !on;
  }
}

/** Re-lay out for the toggle, keeping the cluster at the top of the viewport
 *  where it is. */
function relayout_in_place(page: TimelinePage): void {
  const anchor = page.cluster_els.find((el) => el.getBoundingClientRect().bottom > 0);
  const top_before = anchor ? anchor.getBoundingClientRect().top : 0;

  layout(page, true);

  if (anchor) {
    window.scrollBy(0, anchor.getBoundingClientRect().top - top_before);
  }
}

function init(): void {
  draw_legend();

  // Before the layout: the button is floated in the entry and can change how
  // a title wraps.
  const root = document.getElementById("timeline");
  if (root) {
    add_navigation_buttons(root);
  }

  const page = find_page();
  if (!page) {
    return;
  }

  if (page.toggle) {
    // The spacing mode is not kept: the page always opens evenly spaced, also
    // when the webview restores form state on a reload.
    page.toggle.checked = false;
    page.toggle.addEventListener("change", () => relayout_in_place(page));
  }
  if (page.minimap_toggle) {
    // Not kept either: the page always opens with the minimap.
    page.minimap_toggle.checked = true;
    page.minimap_toggle.addEventListener("change", () => {
      apply_minimap_toggle(page);
      relayout_in_place(page);
    });
  }
  apply_minimap_toggle(page);

  layout(page, true);

  window.addEventListener("scroll", update_minimap_view, { passive: true });
  // A new viewport height changes the strip's height and the band's share of
  // it, with no change of width to cause a layout.
  window.addEventListener("resize", () => draw_minimap(page));

  if (typeof ResizeObserver !== "undefined") {
    let timer: number | undefined;
    new ResizeObserver(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => layout(page, false), RESIZE_DEBOUNCE_MS);
    }).observe(page.clusters_el);
  }

  // Block heights change when the web fonts replace the fallback faces.
  if (document.fonts && document.fonts.ready) {
    document.fonts.ready.then(() => layout(page, true));
  }
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", init);
} else {
  init();
}

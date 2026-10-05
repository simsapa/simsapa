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
  latest_to_year, scale_points, tick_label, tick_years, visible_ticks,
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

// ---- The axis ----

interface TimelinePage {
  root: HTMLElement;
  body: HTMLElement;
  clusters_el: HTMLElement;
  svg_el: SVGSVGElement;
  toggle: HTMLInputElement | null;
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
}

function set_gutter(page: TimelinePage, lanes: number, scaled: boolean): GutterGeometry {
  const geometry = gutter_geometry(
    lanes, scaled ? TICK_LABEL_WIDTH : 0, MARKER_RADIUS, LANE_WIDTH, GUTTER_PADDING,
    page.root.clientWidth * GUTTER_MAX_SHARE,
  );
  page.root.style.setProperty("--timeline-gutter", `${geometry.width}px`);
  return geometry;
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

  layout(page, true);

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

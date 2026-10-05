// Layout arithmetic for the Timeline page. No DOM and no d3: the d3 packages
// are ESM-only and the jest setup cannot load them, so this module stays
// importable from a test. See docs/sutta-navigation-tree.md.

export interface ClusterDates {
  from: number | null;
  to: number | null;
  circa: boolean;
  sort_year: number;
}

/** How certain a cluster's period is, which decides how it is drawn. */
export type ClusterForm = "dated" | "approximate" | "open";

export function cluster_form(dates: ClusterDates): ClusterForm {
  if (dates.from === null || dates.to === null) {
    return "open";
  }
  return dates.circa ? "approximate" : "dated";
}

/** margin-top for each cluster. Index 0 is always 0.
 *
 *  Scaled: the distance between two markers is
 *  max(previous height + base_gap, px_per_year × years between them). */
export function cluster_gaps(heights: number[], sort_years: number[], scaled: boolean,
                             px_per_year: number, base_gap: number): number[] {
  return heights.map((_height, i) => {
    if (i === 0) {
      return 0;
    }
    if (!scaled) {
      return base_gap;
    }
    const years = sort_years[i] - sort_years[i - 1];
    return Math.max(base_gap, px_per_year * years - heights[i - 1]);
  });
}

export interface ScalePoints {
  years: number[];
  ys: number[];
}

/** Strictly increasing years with their y, for a piecewise-linear scale.
 *
 *  Clusters sharing a sort year contribute the LAST one's y. With that,
 *  scale(to) >= scale(from) >= the cluster's own marker y for every cluster,
 *  so an extent bar that starts at its own marker never points backwards. */
export function scale_points(sort_years: number[], marker_ys: number[]): ScalePoints {
  const years: number[] = [];
  const ys: number[] = [];
  sort_years.forEach((year, i) => {
    const last = years.length - 1;
    if (last >= 0 && years[last] === year) {
      ys[last] = marker_ys[i];
    } else {
      years.push(year);
      ys.push(marker_ys[i]);
    }
  });
  return { years, ys };
}

/** Add a final point, if it lies after the last one in both year and y. */
export function extend_scale_points(points: ScalePoints, year: number, y: number): ScalePoints {
  const last = points.years.length - 1;
  if (last >= 0 && (year <= points.years[last] || y <= points.ys[last])) {
    return points;
  }
  return { years: [...points.years, year], ys: [...points.ys, y] };
}

/** The latest `to` year of all clusters, or null when none has one. */
export function latest_to_year(clusters: ClusterDates[]): number | null {
  let latest: number | null = null;
  for (const c of clusters) {
    if (c.to !== null && (latest === null || c.to > latest)) {
      latest = c.to;
    }
  }
  return latest;
}

export interface BarSpan {
  y0: number;
  y1: number;
}

/** The vertical span of each cluster's extent bar. A bar starts at its own
 *  marker. An open-ended bar has a fixed length: downward with only `from`,
 *  upward with only `to`. */
export function bar_spans(clusters: ClusterDates[], marker_ys: number[], scale: (year: number) => number,
                          open_length: number, min_length: number): BarSpan[] {
  return clusters.map((c, i) => {
    const y = marker_ys[i];
    if (c.from !== null && c.to !== null) {
      return { y0: y, y1: Math.max(scale(c.to), y + min_length) };
    }
    if (c.from !== null) {
      return { y0: y, y1: y + open_length };
    }
    return { y0: y - open_length, y1: y };
  });
}

/** Lane (0 = on the axis) for each extent bar so that bars overlapping in y do
 *  not overlap in x. Each bar takes the lowest lane that is free for its
 *  whole span. */
export function bar_lanes(spans: BarSpan[]): number[] {
  const lanes: number[] = [];
  spans.forEach((span, i) => {
    let lane = 0;
    const is_taken = (candidate: number): boolean => {
      for (let j = 0; j < i; j++) {
        if (lanes[j] === candidate && span.y0 < spans[j].y1 && spans[j].y0 < span.y1) {
          return true;
        }
      }
      return false;
    };
    while (is_taken(lane)) {
      lane += 1;
    }
    lanes.push(lane);
  });
  return lanes;
}

/** Every multiple of `step` from `min_year` to `max_year`, inclusive. */
export function tick_years(min_year: number, max_year: number, step: number): number[] {
  const years: number[] = [];
  for (let year = Math.ceil(min_year / step) * step; year <= max_year; year += step) {
    years.push(year);
  }
  return years;
}

/** Whether each tick is drawn: one closer than `min_distance` to the previous
 *  drawn tick is dropped. */
export function visible_ticks(ys: number[], min_distance: number): boolean[] {
  let previous: number | null = null;
  return ys.map((y) => {
    if (previous !== null && y - previous < min_distance) {
      return false;
    }
    previous = y;
    return true;
  });
}

/** The data has no year 0: the tick on the era boundary is labelled 1 CE. */
export function tick_label(year: number): string {
  if (year < 0) {
    return `${-year} BCE`;
  }
  return `${year === 0 ? 1 : year} CE`;
}

export interface GutterGeometry {
  /** x of the axis line and of lane 0. */
  axis_x: number;
  /** Distance between two lanes; narrower than asked for once the cap is reached. */
  lane_width: number;
  /** Total width of the gutter. */
  width: number;
}

/** The gutter holds the tick labels, the axis and the lanes, and is never
 *  wider than `max_width`: beyond that the lanes share the room there is. */
export function gutter_geometry(lane_count: number, label_width: number, marker_radius: number,
                                lane_width: number, padding: number, max_width: number): GutterGeometry {
  const axis_x = label_width + marker_radius + 1;
  const fixed = axis_x + marker_radius + padding;
  const extra_lanes = Math.max(0, lane_count - 1);
  let width = lane_width;
  if (extra_lanes > 0 && fixed + extra_lanes * width > max_width) {
    width = Math.max(1, (max_width - fixed) / extra_lanes);
  }
  return { axis_x, lane_width: width, width: fixed + extra_lanes * width };
}

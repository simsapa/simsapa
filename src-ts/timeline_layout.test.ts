import {
  bar_lanes, bar_spans, cluster_form, cluster_gaps, extend_scale_points, gutter_geometry, latest_to_year,
  scale_points, tick_label, tick_years, visible_ticks, ClusterDates,
} from "./timeline_layout";

const BASE_GAP = 24;

function dates(from: number | null, to: number | null, circa = false): ClusterDates {
  return { from, to, circa, sort_year: from !== null ? from : (to as number) };
}

/** A piecewise-linear scale over the points, clamped: what the page builds with d3. */
function linear_scale(years: number[], ys: number[]): (year: number) => number {
  return (year: number) => {
    if (year <= years[0]) {
      return ys[0];
    }
    for (let i = 1; i < years.length; i++) {
      if (year <= years[i]) {
        const t = (year - years[i - 1]) / (years[i] - years[i - 1]);
        return ys[i - 1] + t * (ys[i] - ys[i - 1]);
      }
    }
    return ys[ys.length - 1];
  };
}

describe("cluster_gaps", () => {
  test("even gaps are all base_gap after the first", () => {
    expect(cluster_gaps([100, 300, 50], [-500, -300, 400], false, 2, BASE_GAP)).toEqual([0, BASE_GAP, BASE_GAP]);
  });

  test("a scaled gap grows with the year difference", () => {
    const gaps = cluster_gaps([100, 100, 100], [-500, -300, 400], true, 2, BASE_GAP);
    expect(gaps).toEqual([0, 2 * 200 - 100, 2 * 700 - 100]);
  });

  test("a scaled gap never goes below base_gap when the previous cluster is taller than the time distance", () => {
    const gaps = cluster_gaps([900, 100], [-500, -400], true, 2, BASE_GAP);
    expect(gaps).toEqual([0, BASE_GAP]);
  });

  test("clusters sharing a sort year are base_gap apart in scaled mode", () => {
    expect(cluster_gaps([10, 10], [-500, -500], true, 2, BASE_GAP)).toEqual([0, BASE_GAP]);
  });

  test("no clusters, no gaps", () => {
    expect(cluster_gaps([], [], true, 2, BASE_GAP)).toEqual([]);
  });
});

describe("scale_points", () => {
  test("clusters sharing a sort year produce one scale point, at the last one's y", () => {
    const points = scale_points([-500, -500, -500, -400], [10, 200, 900, 1000]);
    expect(points).toEqual({ years: [-500, -400], ys: [900, 1000] });
  });

  test("scale years are strictly increasing", () => {
    const { years } = scale_points([-500, -500, -300, -300, -50, -50, 400], [0, 1, 2, 3, 4, 5, 6]);
    for (let i = 1; i < years.length; i++) {
      expect(years[i]).toBeGreaterThan(years[i - 1]);
    }
  });

  test("no bar points backwards when clusters share a sort year", () => {
    // Two clusters start in 500 BCE; the second is far down the page.
    const clusters = [dates(-500, -300), dates(-500, -200), dates(-300, null), dates(-200, -1)];
    const marker_ys = [10, 200, 2400, 2600];
    const points = scale_points(clusters.map((c) => c.sort_year), marker_ys);
    const spans = bar_spans(clusters, marker_ys, linear_scale(points.years, points.ys), 48, 6);
    spans.forEach((span) => expect(span.y1).toBeGreaterThan(span.y0));
    expect(spans[0]).toEqual({ y0: 10, y1: 2400 });
    expect(spans[1]).toEqual({ y0: 200, y1: 2600 });
  });

  test("extend adds a point after the last one only", () => {
    const points = { years: [-500, 1100], ys: [0, 500] };
    expect(extend_scale_points(points, 1200, 700)).toEqual({ years: [-500, 1100, 1200], ys: [0, 500, 700] });
    expect(extend_scale_points(points, 1100, 700)).toBe(points);
    expect(extend_scale_points(points, 1200, 500)).toBe(points);
  });

  test("latest to year", () => {
    expect(latest_to_year([dates(-500, -200), dates(400, null), dates(1100, 1200)])).toBe(1200);
    expect(latest_to_year([dates(400, null)])).toBeNull();
  });
});

describe("bar_spans", () => {
  const scale = (year: number) => year;

  test("an open-ended bar has a fixed length, away from its one bound", () => {
    const spans = bar_spans([dates(100, null), dates(null, 300)], [100, 300], scale, 48, 6);
    expect(spans).toEqual([{ y0: 100, y1: 148 }, { y0: 252, y1: 300 }]);
  });

  test("a bounded bar is never shorter than the minimum", () => {
    expect(bar_spans([dates(100, 100)], [100], scale, 48, 6)).toEqual([{ y0: 100, y1: 106 }]);
  });
});

describe("bar_lanes", () => {
  test("two bars overlapping in y get different lanes", () => {
    expect(bar_lanes([{ y0: 0, y1: 100 }, { y0: 50, y1: 150 }])).toEqual([0, 1]);
  });

  test("two bars that do not overlap share lane 0", () => {
    expect(bar_lanes([{ y0: 0, y1: 100 }, { y0: 100, y1: 150 }])).toEqual([0, 0]);
  });

  test("a freed lane is reused", () => {
    const lanes = bar_lanes([{ y0: 0, y1: 300 }, { y0: 10, y1: 50 }, { y0: 60, y1: 90 }, { y0: 70, y1: 400 }]);
    expect(lanes).toEqual([0, 1, 1, 2]);
  });
});

describe("certainty and ticks", () => {
  test("the three forms", () => {
    expect(cluster_form(dates(-500, -200))).toBe("dated");
    expect(cluster_form(dates(-200, -1, true))).toBe("approximate");
    expect(cluster_form(dates(-300, null, true))).toBe("open");
    expect(cluster_form(dates(null, -50))).toBe("open");
  });

  test("a tick per century inside the domain", () => {
    expect(tick_years(-250, 120, 100)).toEqual([-200, -100, 0, 100]);
    expect(tick_years(-500, -300, 100)).toEqual([-500, -400, -300]);
  });

  test("a tick too close to the previous drawn one is dropped", () => {
    expect(visible_ticks([0, 10, 20, 40, 45], 14)).toEqual([true, false, true, true, false]);
  });

  test("tick labels", () => {
    expect(tick_label(-500)).toBe("500 BCE");
    expect(tick_label(1100)).toBe("1100 CE");
    expect(tick_label(0)).toBe("1 CE");
  });
});

describe("gutter_geometry", () => {
  test("lanes keep their width under the cap", () => {
    expect(gutter_geometry(4, 0, 5, 8, 10, 200)).toEqual({ axis_x: 6, lane_width: 8, width: 6 + 5 + 10 + 3 * 8 });
  });

  test("beyond the cap the lanes share the room", () => {
    const geometry = gutter_geometry(7, 46, 5, 8, 10, 108);
    expect(geometry.width).toBeCloseTo(108);
    expect(geometry.lane_width).toBeLessThan(8);
    expect(geometry.lane_width).toBeGreaterThanOrEqual(1);
  });

  test("a single lane needs no lane room", () => {
    expect(gutter_geometry(1, 0, 5, 8, 10, 20).width).toBe(21);
  });
});

// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

/**
 * Screen geometry for the parameter plots.
 *
 * One curve or a family of them are the same problem once the points are laid
 * out: every series shares one pair of scales, or the curves would not be
 * comparable to each other. Kept apart from the components so the arithmetic
 * can be tested without a browser.
 */

/**
 * Lay out one or more series in a box of `width` x `height`.
 *
 * Coordinates come out in real pixels rather than a fixed viewBox to be
 * stretched: the panes holding these plots resize, and a stretched viewBox
 * turns the point dots into ellipses.
 *
 * `yRange` pins the vertical scale to something wider than what is on screen.
 * Stepping through the planes of a cuboid otherwise re-scales every plane to
 * its own extent, and a flat plane comes out looking exactly like a varied
 * one — the same trap the cell shading avoids by measuring the whole object.
 *
 * @param {{x: number, y: number}[][]} series
 * @param {{width: number, height: number, pad: {t: number, r: number, b: number, l: number},
 *          yRange?: {lo: number, hi: number}|null}} box
 * @returns {{
 *   series: {index: number, points: {x: number, y: number, cx: number, cy: number}[], line: string}[],
 *   xLo: number, xHi: number, yLo: number, yHi: number, zero: number|null,
 * } | null} `null` when there is too little to draw.
 */
export function buildPlot(series, { width, height, pad, yRange = null }) {
  if (!Array.isArray(series) || width <= 0 || height <= 0) return null;

  // A point that is not finite would poison every extent it took part in.
  const clean = series.map((pts) =>
    (pts ?? []).filter((p) => Number.isFinite(p?.x) && Number.isFinite(p?.y)),
  );
  const all = clean.flat();
  if (all.length < 2) return null;

  const xs = all.map((p) => p.x);
  const ys = all.map((p) => p.y);
  const xLo = Math.min(...xs);
  const xHi = Math.max(...xs);
  // A pinned range still has to contain what is drawn, or points fall outside
  // the box; widening to fit is the honest reconciliation.
  const yLo = yRange ? Math.min(yRange.lo, ...ys) : Math.min(...ys);
  const yHi = yRange ? Math.max(yRange.hi, ...ys) : Math.max(...ys);

  const w = Math.max(1, width - pad.l - pad.r);
  const h = Math.max(1, height - pad.t - pad.b);

  // A constant series has no span to divide by; centre it rather than pinning
  // every point to one edge, which would read as if the values differed.
  const sx = (v) => (xHi === xLo ? pad.l + w / 2 : pad.l + ((v - xLo) / (xHi - xLo)) * w);
  const sy = (v) => (yHi === yLo ? pad.t + h / 2 : pad.t + h - ((v - yLo) / (yHi - yLo)) * h);

  const laid = clean
    .map((pts, index) => {
      const points = pts.map((p) => ({ ...p, cx: sx(p.x), cy: sy(p.y) }));
      return {
        index,
        points,
        line: points.map((p) => `${p.cx.toFixed(1)},${p.cy.toFixed(1)}`).join(' '),
      };
    })
    .filter((s) => s.points.length > 0);

  return { series: laid, xLo, xHi, yLo, yHi, zero: yLo < 0 && yHi > 0 ? sy(0) : null };
}

/**
 * The plotted point nearest `x` in screen pixels, across every series.
 *
 * Nearest in x alone rather than true distance: these plots are read by
 * sweeping across the axis, and matching on x keeps the readout from jumping
 * between curves that happen to cross.
 * @returns {{series: number, point: object} | null}
 */
export function nearestPoint(plot, x, seriesFilter = null) {
  if (!plot) return null;
  let best = null;
  let bestD = Infinity;
  for (const s of plot.series) {
    if (seriesFilter !== null && s.index !== seriesFilter) continue;
    for (const p of s.points) {
      const d = Math.abs(p.cx - x);
      if (d < bestD) {
        bestD = d;
        best = { series: s.index, point: p };
      }
    }
  }
  return best;
}

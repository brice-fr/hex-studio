// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

/**
 * Projecting a map onto a 3D surface.
 *
 * A map is a height field — `z = f(x, y)` over a grid — which is what makes
 * this tractable without WebGL. Cells are disjoint in the x-y plane, so two of
 * them can only overlap on screen when one is genuinely in front, and drawing
 * them back to front by the depth of their *footprint* is exact rather than an
 * approximation. No depth buffer, no triangles, no dependency.
 *
 * Orthographic rather than perspective: a calibration surface is read for the
 * shape of its values, and perspective would make equal steps look unequal.
 */

/** Where the camera starts: turned enough to see two sides, tilted to look down. */
export const DEFAULT_VIEW = { yaw: -0.6, pitch: 0.5 };

/**
 * Positions along one axis, in the units the axis is actually in.
 *
 * True breakpoints wherever they are numeric, so the mesh shows where the data
 * really sits. A verbal axis has no coordinate to place — the demo file's map
 * is indexed by `red, orange, yellow…` — so it falls back to even spacing, and
 * says so, rather than inventing numbers.
 *
 * @param {{phys: number, display: string}[]} points
 * @param {number} n
 * @returns {{positions: number[], even: boolean}}
 */
export function axisPositions(points, n) {
  const vals = Array.from({ length: n }, (_, i) => points?.[i]?.phys);
  const numeric =
    vals.length > 0 &&
    vals.every((v) => typeof v === 'number' && Number.isFinite(v));

  // A verbal breakpoint carries a number too, but it is a key into the
  // COMPU_METHOD's table rather than a position: an enumeration of 0, 1, 5, 100
  // would stretch the mesh for no reason the reader could see. Whether the
  // point *renders* as a number is the honest test.
  const labelled = Array.from({ length: n }, (_, i) => points?.[i]?.display).some(
    (d) => d !== undefined && String(d).trim() !== '' && Number.isNaN(Number(d)),
  );

  // Breakpoints that all share one value give no extent to spread over, which
  // is indistinguishable from having none.
  const spread = numeric && Math.max(...vals) > Math.min(...vals);
  if (numeric && spread && !labelled) return { positions: vals, even: false };
  return { positions: Array.from({ length: n }, (_, i) => i), even: true };
}

/** Rotate a point and return screen offsets plus depth, before fitting. */
function project(x, y, z, yaw, pitch) {
  const cy = Math.cos(yaw);
  const sy = Math.sin(yaw);
  const cp = Math.cos(pitch);
  const sp = Math.sin(pitch);
  // Yaw about the vertical axis, then pitch about the horizontal one.
  const x1 = x * cy - y * sy;
  const y1 = x * sy + y * cy;
  const y2 = y1 * cp - z * sp;
  const z2 = y1 * sp + z * cp;
  // Screen y grows downward, so height climbs the page as z2 rises.
  return { x: x1, y: -z2, depth: y2 };
}

/**
 * Lay a grid of values out as a surface.
 *
 * @param {{xs: number[], ys: number[], values: (number|null)[], dims: number[]}} grid
 *   `values` is flat and row-major with x varying fastest, exactly as the
 *   backend hands it over.
 * @param {{width: number, height: number, pad: number, yaw: number, pitch: number}} view
 * @returns {{
 *   quads: {ix: number, iy: number, points: {x: number, y: number}[], value: number, t: number}[],
 *   frame: {x: number, y: number}[][],
 *   zLo: number, zHi: number,
 * } | null} `null` when there is no surface to draw.
 */
export function buildSurface(grid, view) {
  const { xs, ys, values } = grid;
  const { width, height, pad, yaw, pitch } = view;
  const nx = xs?.length ?? 0;
  const ny = ys?.length ?? 0;
  if (nx < 2 || ny < 2 || width <= 0 || height <= 0) return null;

  const finite = values.filter((v) => typeof v === 'number' && Number.isFinite(v));
  if (finite.length < 2) return null;
  const zLo = Math.min(...finite);
  const zHi = Math.max(...finite);

  const xLo = Math.min(...xs);
  const xHi = Math.max(...xs);
  const yLo = Math.min(...ys);
  const yHi = Math.max(...ys);

  // Everything is normalised into a unit cube centred on the origin, so the
  // camera angles mean the same thing whatever units the axes carry.
  const span = (v, lo, hi) => (hi === lo ? 0.5 : (v - lo) / (hi - lo));
  const nrm = (i, j) => ({
    x: span(xs[i], xLo, xHi) - 0.5,
    y: span(ys[j], yLo, yHi) - 0.5,
    // Two thirds of the footprint, so a tall surface stays readable rather
    // than turning into a wall.
    z: (span(values[j * nx + i], zLo, zHi) - 0.5) * 0.66,
  });

  const at = (i, j) => {
    const p = nrm(i, j);
    return project(p.x, p.y, p.z, yaw, pitch);
  };

  // Fit whatever the rotation produced into the box, rather than reasoning
  // about how wide a given pair of angles happens to be.
  const all = [];
  for (let j = 0; j < ny; j++) for (let i = 0; i < nx; i++) all.push(at(i, j));
  const sxLo = Math.min(...all.map((p) => p.x));
  const sxHi = Math.max(...all.map((p) => p.x));
  const syLo = Math.min(...all.map((p) => p.y));
  const syHi = Math.max(...all.map((p) => p.y));
  const w = Math.max(1, width - 2 * pad);
  const h = Math.max(1, height - 2 * pad);
  const scale = Math.min(
    sxHi === sxLo ? w : w / (sxHi - sxLo),
    syHi === syLo ? h : h / (syHi - syLo),
  );
  const offX = pad + (w - (sxHi - sxLo) * scale) / 2 - sxLo * scale;
  const offY = pad + (h - (syHi - syLo) * scale) / 2 - syLo * scale;
  const screen = (p) => ({ x: offX + p.x * scale, y: offY + p.y * scale });

  /**
   * Depth of a cell's footprint, with height deliberately left out.
   *
   * Occlusion between disjoint footprints under an orthographic projection is
   * decided by the footprint alone — a taller cell reaches further up the page
   * but no closer to the camera. Including z here would reorder cells by how
   * high they happen to be and let a peak hide the ridge in front of it.
   */
  const footDepth = (i, j) => {
    const p = nrm(i, j);
    return project(p.x, p.y, 0, yaw, pitch).depth;
  };

  const quads = [];
  for (let j = 0; j < ny - 1; j++) {
    for (let i = 0; i < nx - 1; i++) {
      const corners = [
        [i, j],
        [i + 1, j],
        [i + 1, j + 1],
        [i, j + 1],
      ];
      const vs = corners.map(([a, b]) => values[b * nx + a]);
      if (vs.some((v) => typeof v !== 'number' || !Number.isFinite(v))) continue;
      const mid = vs.reduce((s, v) => s + v, 0) / 4;
      quads.push({
        ix: i,
        iy: j,
        points: corners.map(([a, b]) => screen(at(a, b))),
        value: mid,
        t: zHi === zLo ? 0.5 : (mid - zLo) / (zHi - zLo),
        depth: corners.reduce((s, [a, b]) => s + footDepth(a, b), 0) / 4,
      });
    }
  }
  // Far first, so nearer cells paint over what they hide.
  quads.sort((a, b) => b.depth - a.depth);

  // The base rectangle, to anchor the eye when the surface is nearly flat.
  const base = [
    [0, 0],
    [nx - 1, 0],
    [nx - 1, ny - 1],
    [0, ny - 1],
  ].map(([i, j]) => {
    const p = nrm(i, j);
    return screen(project(p.x, p.y, -0.5 * 0.66, yaw, pitch));
  });

  return { quads, frame: [base], zLo, zHi };
}

/**
 * The cell under a screen point, nearest first.
 *
 * Walks the painter order backwards so the cell drawn last — the one actually
 * visible there — wins.
 * @returns {{ix: number, iy: number} | null}
 */
export function cellAt(surface, x, y) {
  if (!surface) return null;
  for (let k = surface.quads.length - 1; k >= 0; k--) {
    if (inside(surface.quads[k].points, x, y)) {
      const { ix, iy } = surface.quads[k];
      return { ix, iy };
    }
  }
  return null;
}

/** Ray-crossing test, which needs no assumption that the quad stays convex. */
function inside(pts, x, y) {
  let hit = false;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const a = pts[i];
    const b = pts[j];
    if (a.y > y !== b.y > y && x < ((b.x - a.x) * (y - a.y)) / (b.y - a.y) + a.x) {
      hit = !hit;
    }
  }
  return hit;
}

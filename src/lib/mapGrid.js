// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

/**
 * Index arithmetic and shading for multi-dimensional parameters.
 *
 * The backend hands over `values` as one flat array in row-major presentation
 * order — the first dimension varies fastest — together with `dims`. Every
 * position in a grid, and every slice of a cuboid, is a matter of folding that
 * flat run back into shape, which is what lives here so it can be tested
 * without a browser.
 */

/**
 * Flat index of the cell at `subs`, first dimension varying fastest.
 *
 * This is the index `onEditPoint('value', …)` expects, so the same arithmetic
 * addresses a cell for reading and for writing.
 * @param {number[]} dims
 * @param {number[]} subs  One subscript per dimension; missing ones read as 0.
 * @returns {number}
 */
export function flatIndex(dims, subs) {
  let index = 0;
  let stride = 1;
  for (let d = 0; d < dims.length; d++) {
    index += (subs[d] ?? 0) * stride;
    stride *= dims[d];
  }
  return index;
}

/**
 * The flat indices of one 2D slice, row by row.
 *
 * `fixed` pins every dimension beyond the second; the first two are swept.
 * A one-dimensional object yields a single row, so a curve and a map can share
 * the same renderer.
 * @param {number[]} dims
 * @param {number[]} fixed  Subscripts for dimensions 2 and up.
 * @returns {number[][]}    Rows of flat indices, outer index = Y.
 */
export function sliceIndices(dims, fixed = []) {
  if (!dims.length) return [];
  const nx = dims[0] ?? 1;
  const ny = dims[1] ?? 1;
  const rows = [];
  for (let y = 0; y < ny; y++) {
    const row = [];
    for (let x = 0; x < nx; x++) {
      row.push(flatIndex(dims, [x, y, ...fixed]));
    }
    rows.push(row);
  }
  return rows;
}

/**
 * Lowest and highest finite value, or null when there is nothing to compare.
 *
 * Taken across the whole object rather than the visible slice, so stepping
 * through a cuboid does not silently re-scale the shading under the user.
 * @param {{phys: number}[]} values
 * @returns {{lo: number, hi: number} | null}
 */
export function valueExtent(values) {
  let lo = Infinity;
  let hi = -Infinity;
  for (const v of values ?? []) {
    const n = v?.phys;
    if (typeof n !== 'number' || !Number.isFinite(n)) continue;
    if (n < lo) lo = n;
    if (n > hi) hi = n;
  }
  return lo === Infinity ? null : { lo, hi };
}

/**
 * Where `v` sits in `[lo, hi]`, as 0…1.
 *
 * A flat object has no span to divide by; everything sits mid-ramp rather than
 * pinning to one end, which would read as if the values differed.
 * @returns {number}
 */
export function shadeOf(v, extent) {
  if (!extent || typeof v !== 'number' || !Number.isFinite(v)) return 0;
  const { lo, hi } = extent;
  if (hi === lo) return 0.5;
  return Math.min(1, Math.max(0, (v - lo) / (hi - lo)));
}

/**
 * Which axes a grid shows on screen, and why each is or is not writable.
 *
 * The two read-only cases are different and must not be presented alike: a
 * FIX_AXIS is computed from the A2L and occupies no image bytes, so there is
 * nothing to write anywhere; a shared axis is real data in the image that
 * simply belongs to another object, which the user can go and edit.
 * `why` reads as a tooltip on its own; `note` reads as a clause after the axis
 * letter. Lower-casing the tooltip at the call site instead would turn "A2L"
 * into "a2l".
 * @param {{kind: string, reference: string|null, editable: boolean}} axis
 * @returns {{editable: boolean, reference: string|null, why: string, note: string}}
 */
export function axisAccess(axis) {
  const none = { editable: false, reference: null, why: '', note: '' };
  if (!axis) return none;
  if (axis.editable) return { ...none, editable: true };
  if (axis.reference) {
    return {
      editable: false,
      reference: axis.reference,
      why: `Stored in ${axis.reference} — edit it there`,
      note: `stored in ${axis.reference}`,
    };
  }
  if (axis.kind === 'FIX_AXIS') {
    return {
      editable: false,
      reference: null,
      why: 'Computed from the A2L — not stored',
      note: 'computed from the A2L, not stored',
    };
  }
  return { editable: false, reference: null, why: 'Not editable', note: 'not editable' };
}

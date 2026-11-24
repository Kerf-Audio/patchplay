// Turns the backend's 1-based line/column diagnostics into editor offsets, pure.

/**
 * `[from, to)` offsets within a line of `length` characters for a diagnostic at
 * 1-based `col` (inclusive) to `endCol` (exclusive). A missing or empty range
 * marks one character, or the whole line when the column is 0 or unknown.
 */
export function diagnosticSpan(length: number, col: number, endCol: number): [number, number] {
  const start = Math.min(Math.max(col - 1, 0), length);
  const end = Math.min(Math.max(endCol - 1, 0), length);
  if (col <= 0) return [0, length];
  if (end > start) return [start, end];
  return [start, Math.min(start + 1, length)];
}

/// Test utilities shared across GPUIX test files.

import fs from "fs"

/** Compute byte-level similarity between two buffers (0..1).
 *  For PNGs from the same renderer, identical pixels → identical bytes
 *  (same encoder settings). Any pixel change cascades through compression,
 *  so even small visual diffs produce low byte similarity. */
export function bufferSimilarity(a: Buffer, b: Buffer): number {
  const len = Math.max(a.length, b.length)
  if (len === 0) return 1
  let matching = 0
  for (let i = 0; i < len; i++) {
    if (a[i] === b[i]) matching++
  }
  return matching / len
}

/** Assert two screenshot PNGs exist, are non-empty, and are visually
 *  different (less than 98% byte similarity). */
export function expectScreenshotsDiffer(beforePath: string, afterPath: string) {
  expect(fs.existsSync(beforePath)).toBe(true)
  expect(fs.existsSync(afterPath)).toBe(true)
  expect(fs.statSync(beforePath).size).toBeGreaterThan(0)
  expect(fs.statSync(afterPath).size).toBeGreaterThan(0)

  const before = fs.readFileSync(beforePath)
  const after = fs.readFileSync(afterPath)
  const similarity = bufferSimilarity(before, after)
  expect(similarity).toBeLessThan(0.98)
}

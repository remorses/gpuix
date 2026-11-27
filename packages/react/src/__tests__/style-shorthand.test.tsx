// CSS shorthand strings in style objects: a precise, actionable error instead
// of an opaque `setStyle parse error: invalid type: string, expected f64` that
// rejected the whole batch and blanked the tree.

import React from "react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

describeNative("style shorthand validation", () => {
  const consoleError = vi.spyOn(console, "error").mockImplementation(() => {})
  afterEach(() => {
    consoleError.mockClear()
  })

  function lastErrorMessage(): string {
    const calls = consoleError.mock.calls.flat()
    const last = calls[calls.length - 1]
    return last instanceof Error ? last.message : String(last ?? "")
  }

  it("rejects padding shorthand strings naming the property and value", () => {
    const { render } = createTestRoot()
    render(<div style={{ padding: "8px 12px" }}>hi</div>)
    expect(lastErrorMessage()).toMatch(
      /style\.padding = "8px 12px" is not a number.*paddingTop[\s/]*paddingRight/
    )
  })

  it("rejects margin shorthand strings naming the property and value", () => {
    const { render } = createTestRoot()
    render(<div style={{ margin: "auto" }}>hi</div>)
    expect(lastErrorMessage()).toMatch(/style\.margin = "auto" is not a number/)
  })

  it("rejects per-side multi-value strings naming that field", () => {
    const { render } = createTestRoot()
    render(<div style={{ marginTop: "4px 8px" }}>hi</div>)
    expect(lastErrorMessage()).toMatch(/style\.marginTop = "4px 8px" is not a number/)
  })

  it("accepts numbers and single-value px strings on per-side fields", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ paddingTop: 8, marginTop: "4px" }}>
        <text>ok</text>
      </div>
    )
    expect(renderer.getAllText()).toEqual(["ok"])
  })
})

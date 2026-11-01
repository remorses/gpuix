/// The `highlight` prop, through the real GPUI paint pipeline.
///
/// A highlight is a quad, so `getPaintedText()` cannot see it. Every assertion
/// here goes through `getPaintedHighlights()`, which reports the painted range
/// in UTF-16 units plus the boxes it actually drew.

import React from "react"
import { describe, expect, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"
import { findRanges, useTextSearch } from "../hooks/use-text-search.js"

// The GPU test renderer is macOS and Windows only.
const describeNative = hasNativeTestRenderer ? describe : describe.skip
const itNative = hasNativeTestRenderer ? it : it.skip

/** The matched substrings, in paint order. */
function matched(renderer: { getPaintedHighlights(): Array<{ text: string; start: number; end: number }> }) {
  return renderer.getPaintedHighlights().map((hit) => hit.text.slice(hit.start, hit.end))
}

describeNative("highlight", () => {
  it("paints nothing when no element declares one", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ padding: 20 }}>
        <text>the quick brown fox</text>
      </div>
    )
    expect(renderer.getPaintedHighlights()).toEqual([])
  })

  it("matches a query in a descendant and reports the geometry", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ padding: 20 }} highlight={{ query: "quick" }}>
        <text style={{ fontSize: 20 }}>the quick brown fox</text>
      </div>
    )

    const hits = renderer.getPaintedHighlights()
    expect(hits).toHaveLength(1)
    expect(hits[0].text.slice(hits[0].start, hits[0].end)).toBe("quick")
    expect(hits[0].active).toBe(false)
    expect(hits[0].rects).toHaveLength(1)
    expect(hits[0].rects[0].width).toBeGreaterThan(0)
    expect(hits[0].rects[0].height).toBeGreaterThan(0)
  })

  it("is case-insensitive by default and case-sensitive on request", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div highlight={{ query: "fox" }}>
        <text>Fox fox FOX</text>
      </div>
    )
    expect(matched(renderer)).toEqual(["Fox", "fox", "FOX"])

    render(
      <div highlight={{ query: "fox", caseSensitive: true }}>
        <text>Fox fox FOX</text>
      </div>
    )
    expect(matched(renderer)).toEqual(["fox"])
  })

  it("only searches inside the declaring subtree", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column" }}>
        <div highlight={{ query: "fox" }}>
          <text>searched fox</text>
        </div>
        <div>
          <text>ignored fox</text>
        </div>
      </div>
    )

    const hits = renderer.getPaintedHighlights()
    expect(hits).toHaveLength(1)
    expect(hits[0].text).toBe("searched fox")
  })

  it("lets the nearest declaration win over an ancestor", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column" }} highlight={{ query: "outer" }}>
        <text>outer only here</text>
        <div highlight={{ query: "inner" }}>
          <text>outer and inner</text>
        </div>
      </div>
    )
    expect(matched(renderer)).toEqual(["outer", "inner"])
  })

  // React makes a separate host node for every interpolated string, so this
  // is three painted runs of one logical line.
  it("matches across the host nodes React split apart", () => {
    const { render, renderer } = createTestRoot()
    const name = "Tommy"
    render(
      <div highlight={{ query: "Hello Tommy" }}>
        <text>Hello {name}!</text>
      </div>
    )

    const hits = renderer.getPaintedHighlights()
    expect(hits).toHaveLength(2)
    expect(hits.map((hit) => hit.text.slice(hit.start, hit.end))).toEqual(["Hello ", "Tommy"])
  })

  it("does not match across a line boundary", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column" }} highlight={{ query: "quick brown" }}>
        <text>quick </text>
        <text>brown</text>
      </div>
    )
    expect(renderer.getPaintedHighlights()).toEqual([])
  })

  it("accepts explicit ranges in UTF-16 units", () => {
    const { render, renderer } = createTestRoot()
    const name = "Tommy"
    render(
      <div highlight={{ ranges: [[6, 11]] }}>
        <text>Hello {name}!</text>
      </div>
    )
    expect(matched(renderer)).toEqual(["Tommy"])
  })

  it("rejects a range that splits a surrogate pair", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div highlight={{ ranges: [[1, 2]] }}>
        <text>a👋b</text>
      </div>
    )
    expect(renderer.getPaintedHighlights()).toEqual([])
  })

  it("recolours exactly one match through activeIndex", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div highlight={{ query: "fox", activeIndex: 1 }}>
        <text>fox fox fox</text>
      </div>
    )

    const hits = renderer.getPaintedHighlights()
    expect(hits).toHaveLength(3)
    expect(hits.map((hit) => hit.active)).toEqual([false, true, false])
  })

  it("accepts several specs at once", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div highlight={[{ query: "fox" }, { query: "dog" }]}>
        <text>fox and dog</text>
      </div>
    )
    expect(matched(renderer).sort()).toEqual(["dog", "fox"])
  })

  it("clears when the prop is removed", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div highlight={{ query: "fox" }}>
        <text>a fox</text>
      </div>
    )
    expect(renderer.getPaintedHighlights()).toHaveLength(1)

    render(
      <div>
        <text>a fox</text>
      </div>
    )
    expect(renderer.getPaintedHighlights()).toEqual([])
  })

  it("still paints under userSelect none, like browser find does", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ userSelect: "none" }} highlight={{ query: "label" }}>
        <text>a label</text>
      </div>
    )
    expect(matched(renderer)).toEqual(["label"])
  })

  it("reports the match count through onHighlight, only when it changes", () => {
    const { render, renderer } = createTestRoot()
    const counts: number[] = []
    const app = (query: string) => (
      <div highlight={{ query }} onHighlight={(event) => counts.push(event.matchCount ?? -1)}>
        <text>fox fox dog</text>
      </div>
    )

    render(app("fox"))
    renderer.dispatchNativeEvents()
    expect(counts).toEqual([2])
    // Same tree, same query: nothing new to report.
    render(app("fox"))
    renderer.dispatchNativeEvents()
    expect(counts).toEqual([2])
    render(app("dog"))
    renderer.dispatchNativeEvents()
    expect(counts).toEqual([2, 1])
    expect(renderer.getPaintedHighlights()).toHaveLength(1)
  })

  // `<code>` builds its lines inside render(), so they never reach the
  // retained tree. It matches the exact string it paints instead.
  it("matches inside a native <code> element", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div highlight={{ query: "const" }}>
        <code code={"const a = 1\nlet b = 2\nconst c = 3"} language="typescript" />
      </div>
    )
    expect(matched(renderer)).toEqual(["const", "const"])
  })

  // A native element paints many strings. Numbering restarted per string
  // before, so activeIndex marked one match active on EVERY line.
  it("marks one active match across the lines of a <code> block", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div highlight={{ query: "x", activeIndex: 2 }}>
        <code code={"x x\nx x"} language="typescript" showHeader={false} />
      </div>
    )

    const actives = renderer.getPaintedHighlights().map((hit) => hit.active)
    expect(actives).toEqual([false, false, true, false])
  })

  // Ordinals follow PAINT order, so `activeIndex` means "the nth match in the
  // document" no matter which kind of element painted it. Numbering retained
  // matches first made activeIndex: 0 mark the <text> even when <code> came
  // first in the document.
  it("numbers matches in document order across both kinds", () => {
    const { render, renderer } = createTestRoot()
    const app = (first: "text" | "code") => (
      <div
        style={{ display: "flex", flexDirection: "column" }}
        highlight={{ query: "x", activeIndex: 0 }}
      >
        {first === "text" ? <text>x here</text> : null}
        <code code={"x again"} language="typescript" showHeader={false} />
        {first === "code" ? <text>x here</text> : null}
      </div>
    )

    render(app("text"))
    let hits = renderer.getPaintedHighlights()
    expect(hits).toHaveLength(2)
    expect(hits[0].text).toContain("here")
    expect(hits.map((hit) => hit.active)).toEqual([true, false])

    render(app("code"))
    hits = renderer.getPaintedHighlights()
    expect(hits).toHaveLength(2)
    expect(hits[0].text).toContain("again")
    expect(hits.map((hit) => hit.active)).toEqual([true, false])
  })

  it("reports a count to a listener added after the first render", () => {
    const { render, renderer } = createTestRoot()
    const counts: number[] = []

    render(
      <div highlight={{ query: "fox" }}>
        <text>fox and fox</text>
      </div>
    )
    renderer.dispatchNativeEvents()

    render(
      <div highlight={{ query: "fox" }} onHighlight={(e) => counts.push(e.matchCount ?? -1)}>
        <text>fox and fox</text>
      </div>
    )
    renderer.dispatchNativeEvents()
    expect(counts).toEqual([2])
  })

  it("keeps native content searchable under userSelect none", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ userSelect: "none" }} highlight={{ query: "const" }}>
        <code code={"const a = 1"} language="typescript" showHeader={false} />
      </div>
    )
    expect(matched(renderer)).toEqual(["const"])
  })

  it("does not search element chrome such as a code gutter", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div highlight={{ query: "2" }}>
        <code code={"let a = 1\nlet b = 3"} language="typescript" showLineNumbers showHeader={false} />
      </div>
    )
    // The gutter prints "1" and "2"; only content may match, and there is no
    // "2" in the code itself.
    expect(renderer.getPaintedText()).toContain("2")
    expect(renderer.getPaintedHighlights()).toEqual([])
  })

  it("matches inside a native <markdown> element", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div highlight={{ query: "needle" }}>
        <markdown source={"a paragraph with a needle in it"} />
      </div>
    )
    expect(matched(renderer)).toEqual(["needle"])
  })

  // A `<virtual-list>` mounts a window of its rows, so native can only number
  // the matches inside that window. `matchIndexOffset` is how the app says how
  // many matches came before it, which is what keeps `activeIndex` meaning "the
  // nth match in the document".
  it("numbers matches from matchIndexOffset in a windowed virtual list", () => {
    const { render, renderer } = createTestRoot()
    const windowed = (start: number, activeIndex: number) => (
      <div
        style={{ width: 400, height: 160 }}
        highlight={{ query: "fox", activeIndex, matchIndexOffset: start }}
      >
        <virtual-list
          itemCount={1000}
          windowStart={start}
          overdraw={0}
          estimatedItemHeight={40}
          style={{ width: 400, height: 160 }}
        >
          {Array.from({ length: 4 }, (_, offset) => (
            <div key={start + offset} style={{ height: 40, flexShrink: 0 }}>
              <text>{`fox ${start + offset}`}</text>
            </div>
          ))}
        </virtual-list>
      </div>
    )

    // One match per row, so the row index is also the document match index.
    render(windowed(50, 52))
    renderer.scrollToItem(renderer.findByType("virtual-list")[0].id, 50)
    expect(renderer.getPaintedText()).toContain("fox 50")

    const active = renderer.getPaintedHighlights().filter((hit) => hit.active)
    expect(active).toHaveLength(1)
    expect(active[0].text).toBe("fox 52")

    // A cursor outside the mounted window paints no active wash at all.
    render(windowed(50, 900))
    expect(renderer.getPaintedHighlights().filter((hit) => hit.active)).toEqual([])
  })

  // The offset counts MATCHES, not rows, and the two are deliberately
  // different here: the window starts at row 2, but 4 matches sit above it.
  // Passing `windowStart` instead would mark the wrong row every time.
  it("counts matches and not rows in the offset", () => {
    const { render, renderer } = createTestRoot()
    const rows = ["fox fox", "fox fox", "fox", "", "fox fox fox"]
    const windowStart = 2
    const above = rows
      .slice(0, windowStart)
      .reduce((n, text) => n + findRanges({ text, query: "fox" }).length, 0)
    expect(above).toBe(4)
    expect(above).not.toBe(windowStart)

    // Mounted rows 2..4 hold 1, 0 and 3 matches, numbered 4, then 5, 6 and 7.
    const activeAt = (activeIndex: number) => {
      render(
        <div
          style={{ width: 400, height: 200 }}
          highlight={{ query: "fox", activeIndex, matchIndexOffset: above }}
        >
          <virtual-list
            itemCount={rows.length}
            windowStart={windowStart}
            overdraw={0}
            estimatedItemHeight={40}
            style={{ width: 400, height: 200 }}
          >
            {rows.slice(windowStart).map((text, offset) => (
              <div key={windowStart + offset} style={{ height: 40, flexShrink: 0 }}>
                <text>{text || " "}</text>
              </div>
            ))}
          </virtual-list>
        </div>
      )
      renderer.scrollToItem(renderer.findByType("virtual-list")[0].id, windowStart)
      return renderer
        .getPaintedHighlights()
        .map((hit) => hit.active)
    }

    // Row 2 holds the single match numbered 4.
    expect(activeAt(4)).toEqual([true, false, false, false])
    // Row 4 holds matches 5, 6 and 7.
    expect(activeAt(6)).toEqual([false, false, true, false])
    expect(activeAt(7)).toEqual([false, false, false, true])
    // The row index is not the match index. With `windowStart` as the offset,
    // ordinal 4 would be the third wash rather than the first.
    expect(activeAt(2)).toEqual([false, false, false, false])
  })

  it("leaves activeIndex zero-based when the offset is zero", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div highlight={{ query: "fox", activeIndex: 1, matchIndexOffset: 0 }}>
        <text>fox fox fox</text>
      </div>
    )
    expect(renderer.getPaintedHighlights().map((hit) => hit.active)).toEqual([
      false,
      true,
      false,
    ])
  })
})

describeNative("useTextSearch", () => {
  function FindBar({ query }: { query: string }) {
    const search = useTextSearch({ query })
    return (
      <div style={{ display: "flex", flexDirection: "column" }}>
        <text>{`count:${search.total}`}</text>
        <div {...search.props}>
          <text>fox and fox</text>
        </div>
      </div>
    )
  }

  it("reports the count and clears it when the query is cleared", () => {
    const { render, renderer } = createTestRoot()

    render(<FindBar query="fox" />)
    renderer.dispatchNativeEvents()
    expect(renderer.getAllText()).toContain("count:2")
    expect(renderer.getPaintedHighlights()).toHaveLength(2)

    // An empty query paints nothing and therefore reports nothing. The count
    // must not survive from the previous query.
    render(<FindBar query="" />)
    renderer.dispatchNativeEvents()
    expect(renderer.getAllText()).toContain("count:0")
    expect(renderer.getPaintedHighlights()).toEqual([])
  })

  it("marks the first match active by default", () => {
    const { render, renderer } = createTestRoot()
    render(<FindBar query="fox" />)
    renderer.dispatchNativeEvents()
    expect(renderer.getPaintedHighlights().map((hit) => hit.active)).toEqual([
      true,
      false,
    ])
  })
})

describe("findRanges", () => {
  itNative("matches Unicode ranges identically in JS, retained text, and native code", () => {
    const { render, renderer } = createTestRoot()
    const cases: Array<{ text: string; query: string; wholeWord?: boolean; expected: number[][] }> = [
      { text: "ΟΣ", query: "ΟΣ", expected: [[0, 2]] },
      { text: "ΟΣ ΟΣ", query: "ος", expected: [[0, 2], [3, 5]] },
      { text: "ΟΣΑ", query: "οσα", expected: [[0, 3]] },
      { text: "İ", query: "i", expected: [[0, 1]] },
      { text: "xİx", query: "xi", expected: [[0, 2]] },
      { text: "İ", query: "\u0307", expected: [[0, 1]] },
      { text: "İİİ", query: "\u0307i", expected: [[0, 2]] },
      { text: "\u{10400}İ fox", query: "i", expected: [[2, 3]] },
      { text: "cafe\u0301", query: "cafe", wholeWord: true, expected: [] },
      { text: "cafe\u093e", query: "cafe", wholeWord: true, expected: [] },
      { text: "cafe\u20dd", query: "cafe", wholeWord: true, expected: [] },
      { text: "cafe\u0301", query: "cafe\u0301", wholeWord: true, expected: [[0, 5]] },
    ]
    for (const { text, query, wholeWord, expected } of cases) {
      expect(findRanges({ text, query, wholeWord })).toEqual(expected)
      for (const kind of ["text", "code"] as const) {
        render(
          <div highlight={{ query, wholeWord }}>
            {kind === "text" ? <text>{text}</text> : <code code={text} />}
          </div>
        )
        const ranges = renderer.getPaintedHighlights().map(({ start, end }) => [start, end])
        expect(ranges, JSON.stringify({ kind, text, query, wholeWord })).toEqual(expected)
      }
    }
  })

  it("mirrors the native matcher for the common cases", () => {
    expect(findRanges({ text: "Fox fox FOX", query: "fox" })).toEqual([
      [0, 3],
      [4, 7],
      [8, 11],
    ])
    expect(
      findRanges({ text: "Fox fox FOX", query: "fox", caseSensitive: true })
    ).toEqual([[4, 7]])
    expect(
      findRanges({ text: "foo food _foo foo!", query: "foo", wholeWord: true })
    ).toEqual([
      [0, 3],
      [14, 17],
    ])
    expect(findRanges({ text: "aaaa", query: "aa" })).toEqual([
      [0, 2],
      [2, 4],
    ])
    expect(findRanges({ text: "anything", query: "" })).toEqual([])
  })

  // Lowercasing changes UTF-16 length for some characters, so a naive
  // toLowerCase() on the whole string shifts every index after them.
  it("keeps offsets correct when folding changes length", () => {
    const text = "İstanbul and fox"
    expect(findRanges({ text, query: "fox" })).toEqual([[13, 16]])
    expect(text.slice(13, 16)).toBe("fox")
  })

  // An astral letter is 2 UTF-16 units. Reading one unit sees a lone
  // surrogate, calls it a boundary, and disagrees with the native matcher,
  // which reads whole scalars.
  it("reads whole code points at a word boundary", () => {
    expect(
      findRanges({ text: "\u{10400}foo", query: "foo", wholeWord: true })
    ).toEqual([])
    expect(
      findRanges({ text: "foo\u{10400}", query: "foo", wholeWord: true })
    ).toEqual([])
    expect(
      findRanges({ text: "\u{1F44B}foo", query: "foo", wholeWord: true })
    ).toEqual([[2, 5]])
  })

  // Rust uses char::is_alphanumeric, which is the Unicode Alphabetic property.
  // With \p{L} the JS side called a combining mark a word boundary and the two
  // matchers disagreed.
  it("treats combining marks as word characters, like the native matcher", () => {
    expect(
      findRanges({ text: "foo\u0345", query: "foo", wholeWord: true })
    ).toEqual([])
    expect(
      findRanges({ text: "foo\u05B0", query: "foo", wholeWord: true })
    ).toEqual([])
  })

  itNative("agrees with the native matcher on the same input", () => {
    const { render, renderer } = createTestRoot()
    const text = "İstanbul and fox and Fox"
    render(
      <div highlight={{ query: "fox" }}>
        <text>{text}</text>
      </div>
    )

    const native = renderer
      .getPaintedHighlights()
      .map((hit) => [hit.start, hit.end])
    expect(findRanges({ text, query: "fox" })).toEqual(native)
  })
})

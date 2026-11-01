import { EventEmitter } from "node:events"
import { describe, expect, it } from "vitest"
import type { EventPayload } from "../../index.js"
import {
  createMutationQueue,
  createRendererState,
  findRanges,
  observeWindowInsets,
  observeWindowSize,
  observeSelectedText,
  readWindowInsets,
  readWindowSize,
} from "../host.js"
import type { MutationHost, WindowInsetsHost, WindowSizeHost } from "../host.js"

class RecordingRenderer implements MutationHost, WindowSizeHost, WindowInsetsHost {
  batches: unknown[][] = []
  fail = false
  width = 320
  height = 200

  applyBatch(json: string): number[] {
    if (this.fail) throw new Error("native failure")
    this.batches.push(JSON.parse(json))
    return [9]
  }

  getWindowSize() {
    return { width: this.width, height: this.height }
  }

  getWindowInsets() {
    return {
      safeArea: { top: 1, right: 2, bottom: 3, left: 4 },
      ime: { top: 0, right: 0, bottom: 20, left: 0 },
      effective: { top: 1, right: 2, bottom: 20, left: 4 },
    }
  }
}

describe("shared host runtime", () => {
  it("queues mutations from an applyBatch-only host", () => {
    const batches: unknown[][] = []
    const renderer: MutationHost = {
      applyBatch(json) {
        batches.push(JSON.parse(json))
        return []
      },
    }
    const mutations = createMutationQueue(renderer)
    mutations.createElement(1, "div")
    mutations.flushMutations()
    expect(batches).toEqual([[["createElement", 1, "div"]]])
  })

  it("reads window size from a size-only host", () => {
    const renderer: WindowSizeHost = {
      getWindowSize: () => ({ width: 12, height: 8 }),
    }
    expect(readWindowSize(renderer)).toEqual({ width: 12, height: 8 })
  })

  it("keeps the last window size when a read fails", () => {
    let error: unknown = null
    const renderer: WindowSizeHost = {
      getWindowSize() {
        if (error) throw error
        return { width: 300, height: 200 }
      },
    }
    expect(readWindowSize(renderer)).toEqual({ width: 300, height: 200 })

    error = new Error("timed out")
    expect(readWindowSize(renderer)).toEqual({ width: 300, height: 200 })
    expect(readWindowSize({ getWindowSize: () => { throw error } })).toEqual({ width: 800, height: 600 })
  })

  it("reads insets from a size-and-insets host", () => {
    const renderer: WindowSizeHost & WindowInsetsHost = {
      getWindowSize: () => ({ width: 100, height: 50 }),
      getWindowInsets: () => ({
        safeArea: { top: 1, right: 0, bottom: 0, left: 0 },
        ime: { top: 0, right: 0, bottom: 10, left: 0 },
        effective: { top: 1, right: 0, bottom: 10, left: 0 },
      }),
    }
    expect(readWindowInsets(renderer)).toMatchObject({
      keyboardTop: 40,
      keyboardVisible: true,
      visibleHeight: 39,
    })
  })

  it("returns independent fallback and host geometry snapshots", () => {
    const failingHost = {
      getWindowSize() { throw new Error("opening") },
      getWindowInsets() { throw new Error("opening") },
    }
    for (const host of [null, failingHost]) {
      const size = readWindowSize(host)
      size.width = 17
      expect(readWindowSize(host)).toEqual({ width: 800, height: 600 })

      const insets = readWindowInsets(host)
      insets.safeArea.bottom = 23
      expect(insets.ime.bottom).toBe(0)
      expect(insets.effective.bottom).toBe(0)
      insets.ime.bottom = 42
      expect(readWindowInsets(host)).toMatchObject({
        safeArea: { bottom: 0 }, ime: { bottom: 0 }, effective: { bottom: 0 },
        keyboardTop: 600, keyboardVisible: false, visibleHeight: 600,
      })
    }

    const size = { width: 100, height: 50 }
    const edges = { top: 0, right: 0, bottom: 10, left: 0 }
    const host = {
      getWindowSize: () => size,
      getWindowInsets: () => ({ safeArea: edges, ime: edges, effective: edges }),
    }
    readWindowSize(host).width = 1
    const snapshot = readWindowInsets(host)
    snapshot.safeArea.bottom = 99
    expect(snapshot.ime.bottom).toBe(10)
    expect(snapshot.effective.bottom).toBe(10)
    edges.bottom = 12
    expect(snapshot.ime.bottom).toBe(10)
    expect(readWindowSize(host)).toEqual({ width: 100, height: 50 })
    expect(readWindowInsets(host)).toMatchObject({
      safeArea: { bottom: 12 }, ime: { bottom: 12 }, effective: { bottom: 12 },
      keyboardTop: 38, keyboardVisible: true, visibleHeight: 38,
    })
  })

  it("retains failed mutations and cleans handlers after success", () => {
    const renderer = new RecordingRenderer()
    const removed: number[] = []
    const mutations = createMutationQueue(renderer, (ids) => removed.push(...ids))

    mutations.createElement(1, "div")
    mutations.destroyElement(9)
    renderer.fail = true
    expect(() => mutations.flushMutations()).toThrow("native failure")
    expect(mutations.pending).toBe(2)
    expect(removed).toEqual([])

    renderer.fail = false
    mutations.flushMutations()
    expect(renderer.batches).toMatchInlineSnapshot(`
      [
        [
          [
            "createElement",
            1,
            "div",
          ],
          [
            "destroyElement",
            9,
          ],
        ],
      ]
    `)
    expect(removed).toEqual([9])
  })

  it("owns one live root and rejects stale window events", () => {
    const renderer = new RecordingRenderer()
    const state = createRendererState(renderer)
    const received: string[] = []
    const first = state.attach({
      onWindowKeyDown: (event) => received.push(event.key ?? ""),
    })

    expect(
      state.dispatch({
        elementId: first.windowKeyEventId,
        eventType: "windowKeyDown",
        key: "a",
      })
    ).toBe(true)
    first.detach()

    const second = state.attach({
      onWindowKeyDown: (event) => received.push(event.key ?? ""),
    })
    expect(
      state.dispatch({
        elementId: first.windowKeyEventId,
        eventType: "windowKeyDown",
        key: "stale",
      })
    ).toBe(false)
    expect(second.windowKeyEventId).not.toBe(first.windowKeyEventId)
    expect(received).toEqual(["a"])
  })

  it("observes window geometry and stops polling", async () => {
    const renderer = new RecordingRenderer()
    const sizes: unknown[] = []
    const insets: unknown[] = []
    const stopSize = observeWindowSize(renderer, (value) => sizes.push(value), {
      intervalMs: 16,
    })
    const stopInsets = observeWindowInsets(renderer, (value) => insets.push(value), {
      intervalMs: false,
    })

    renderer.width = 640
    await new Promise((resolve) => setTimeout(resolve, 25))
    stopSize()
    stopInsets()

    expect(sizes).toEqual([
      { width: 320, height: 200 },
      { width: 640, height: 200 },
    ])
    expect(insets).toMatchInlineSnapshot(`
      [
        {
          "effective": {
            "bottom": 20,
            "left": 4,
            "right": 2,
            "top": 1,
          },
          "ime": {
            "bottom": 20,
            "left": 0,
            "right": 0,
            "top": 0,
          },
          "keyboardTop": 180,
          "keyboardVisible": true,
          "safeArea": {
            "bottom": 3,
            "left": 4,
            "right": 2,
            "top": 1,
          },
          "visibleHeight": 179,
        },
      ]
    `)
  })

  it("shares the native-compatible text matcher", () => {
    expect(findRanges({ text: "One one stone", query: "one", wholeWord: true }))
      .toEqual([[0, 3], [4, 7]])
  })

  it("maps contextual lowercase and expanded characters to complete original ranges", () => {
    const cases: Array<{ text: string; query: string; expected: Array<[number, number]> }> = [
      { text: "ΟΣ", query: "ΟΣ", expected: [[0, 2]] },
      { text: "ΟΣ ΟΣ", query: "ος", expected: [[0, 2], [3, 5]] },
      { text: "ΟΣΑ", query: "οσα", expected: [[0, 3]] },
      { text: "İ", query: "i", expected: [[0, 1]] },
      { text: "xİx", query: "xi", expected: [[0, 2]] },
      { text: "İ", query: "\u0307", expected: [[0, 1]] },
      { text: "İ", query: "i\u0307", expected: [[0, 1]] },
      { text: "İİİ", query: "\u0307i", expected: [[0, 2]] },
      { text: "\u{10400}İ fox", query: "i", expected: [[2, 3]] },
      { text: "\u{10400}İ fox", query: "\u{10428}", expected: [[0, 2]] },
      { text: "ﬀ", query: "ff", expected: [] },
      { text: "İstanbul", query: "istanbul", expected: [] },
    ]
    for (const { text, query, expected } of cases) {
      expect(findRanges({ text, query }), JSON.stringify({ text, query })).toEqual(expected)
    }
    expect(findRanges({ text: "İ", query: "i", caseSensitive: true })).toEqual([])
  })

  it("keeps all Unicode mark categories inside a whole word", () => {
    for (const mark of ["\u0301", "\u093e", "\u20dd"]) {
      expect(findRanges({ text: `cafe${mark}`, query: "cafe", wholeWord: true })).toEqual([])
      expect(findRanges({ text: `${mark}cafe`, query: "cafe", wholeWord: true })).toEqual([])
      expect(findRanges({ text: `cafe${mark}`, query: `cafe${mark}`, wholeWord: true })).toEqual([[0, 5]])
    }
    expect(findRanges({ text: "cafe\u0301", query: "cafe", wholeWord: true, caseSensitive: true })).toEqual([])
  })

  it("observes selected text and removes its callback", () => {
    const renderer = new RecordingRenderer() as RecordingRenderer & {
      getSelectedText(): string | null
    }
    renderer.getSelectedText = () => "initial"
    let listener: ((event: { value?: string }) => void) | undefined
    const values: Array<string | null> = []
    const stop = observeSelectedText(
      renderer,
      (next) => {
        listener = next
        return () => { listener = undefined }
      },
      (value) => values.push(value)
    )
    listener?.({ value: "changed" })
    stop()
    listener?.({ value: "ignored" })
    expect(values).toEqual(["initial", "changed"])
  })

  it("subscribes before the initial selection callback and cleans up initialization errors", () => {
    const events = new EventEmitter()
    let selected: string | null = "old"
    const renderer = { getSelectedText: () => selected }
    const subscribe = (listener: (event: EventPayload) => void) => {
      events.on("selectionChange", listener)
      return () => { events.off("selectionChange", listener) }
    }
    const values: Array<string | null> = []
    const stop = observeSelectedText(renderer, subscribe, (value) => {
      values.push(value)
      if (value !== "old") return
      selected = "new"
      events.emit("selectionChange", { elementId: 0, eventType: "selectionChange", value: selected })
    })
    expect(values).toEqual(["old", "new"])
    selected = null
    events.emit("selectionChange", { elementId: 0, eventType: "selectionChange" })
    stop()
    events.emit("selectionChange", { elementId: 0, eventType: "selectionChange", value: "ignored" })
    expect(values).toEqual(["old", "new", null])
    expect(events.listenerCount("selectionChange")).toBe(0)

    const failure = new Error("initial selection failed")
    expect(() => observeSelectedText(renderer, subscribe, () => { throw failure })).toThrow(failure)
    expect(events.listenerCount("selectionChange")).toBe(0)
    expect(() => observeSelectedText({ getSelectedText() { throw failure } }, subscribe, () => {})).toThrow(failure)
    expect(events.listenerCount("selectionChange")).toBe(0)
  })
})

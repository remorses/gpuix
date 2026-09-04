/// Style routing in the reconciler host config.
///
/// These tests drive the host config directly with a recording renderer, so
/// they run on any machine with no GPU and no Metal toolchain. What they check
/// is which style the reconciler sends, which is JavaScript logic and does not
/// need a real frame.
///
/// Driving `hostConfig` by hand is deliberate. React calls `hideInstance` for
/// hidden Activity trees and for Suspense retries, and the pinned
/// react-reconciler is older than the React that exports `Activity`, so there
/// is no way to reach those paths from a normal render here.

import { describe, it, expect } from "vitest"
import { hostConfig } from "../reconciler/host-config"
import { createClassNameCache } from "../reconciler/class-names"
import type {
  ClassNameResolver,
  Container,
  HostContext,
  NativeRenderer,
  Props,
  StyleDesc,
} from "../types/host"

interface StyleCall {
  id: number
  style: Record<string, unknown>
}

/** Records every style the reconciler sends. */
function recordingRenderer(): NativeRenderer & { styles: StyleCall[] } {
  const styles: StyleCall[] = []
  return {
    styles,
    createElement() {},
    destroyElement: () => [],
    appendChild() {},
    removeChild() {},
    insertBefore() {},
    setStyle(id: number, styleJson: string | object) {
      const style = typeof styleJson === "string" ? JSON.parse(styleJson) : styleJson
      styles.push({ id, style: style as Record<string, unknown> })
    },
    setText() {},
    setEventListener() {},
    setRoot() {},
    commitMutations() {},
    setCustomProp() {},
  }
}

function setup(props: Props, resolve?: ClassNameResolver) {
  const renderer = recordingRenderer()
  const container: Container = {
    renderer,
    ids: { nextElementId: 0 },
    eventHandlers: new Map(),
    classNames: resolve ? createClassNameCache(resolve) : null,
    warnedAboutClassName: false,
  }
  const instance = hostConfig.createInstance(
    "div",
    props,
    container,
    null as unknown as HostContext
  )
  renderer.styles.length = 0
  return { renderer, instance, container }
}

/** The style of the last setStyle call. */
function lastStyle(renderer: { styles: StyleCall[] }): Record<string, unknown> | null {
  return renderer.styles.at(-1)?.style ?? null
}

describe("host config style routing", () => {
  const props: Props = {
    style: { width: 100, height: 40, backgroundColor: "#ff0000" },
  }

  it("keeps the element style when React hides the element", () => {
    const { renderer, instance } = setup(props)

    hostConfig.hideInstance(instance)

    // `visibility: hidden` skips the paint and keeps the layout box. Sending
    // only the visibility would drop the box and every other style source.
    expect(lastStyle(renderer)).toMatchObject({
      visibility: "hidden",
      width: 100,
      height: 40,
      backgroundColor: "#ff0000",
    })
  })

  it("puts the style back when React shows the element again", () => {
    const { renderer, instance } = setup(props)

    hostConfig.hideInstance(instance)
    hostConfig.unhideInstance(instance, props)

    const last = lastStyle(renderer)
    expect(last).toMatchObject({ width: 100, height: 40, backgroundColor: "#ff0000" })
    expect(last?.visibility).toBeUndefined()
  })

  it("resends the full style on update", () => {
    const { renderer, instance } = setup(props)

    hostConfig.commitUpdate(
      instance,
      "div",
      props,
      { style: { width: 200 } },
      null
    )

    expect(lastStyle(renderer)).toEqual({ width: 200 })
  })
})

/// A resolver over a fixed table, counting what it was asked.
function tableResolver(table: Record<string, StyleDesc>) {
  const asked: string[] = []
  const resolve: ClassNameResolver = (token) => {
    asked.push(token)
    return table[token] ?? null
  }
  return { resolve, asked }
}

describe("className", () => {
  it("sends the style a class declares", () => {
    const { resolve } = tableResolver({ "p-4": { padding: 16 } })
    const { renderer, instance } = setup({ className: "p-4" }, resolve)

    hostConfig.commitUpdate(instance, "div", {}, { className: "p-4" }, null)

    expect(lastStyle(renderer)).toEqual({ padding: 16 })
  })

  it("takes the later token when two write the same key", () => {
    const { resolve } = tableResolver({
      "bg-red": { backgroundColor: "#ff0000" },
      "bg-blue": { backgroundColor: "#0000ff" },
    })
    const props = { className: "bg-red bg-blue" }
    const { renderer, instance } = setup(props, resolve)

    hostConfig.commitUpdate(instance, "div", {}, props, null)

    expect(lastStyle(renderer)).toEqual({ backgroundColor: "#0000ff" })
  })

  it("drops a token the resolver does not know", () => {
    const { resolve } = tableResolver({ "p-4": { padding: 16 } })
    const props = { className: "p-4 not-a-real-class" }
    const { renderer, instance } = setup(props, resolve)

    hostConfig.commitUpdate(instance, "div", {}, props, null)

    expect(lastStyle(renderer)).toEqual({ padding: 16 })
  })

  it("lets the style prop beat a class key by key", () => {
    const { resolve } = tableResolver({
      "p-4": { padding: 16, backgroundColor: "#ff0000" },
    })
    const props = { className: "p-4", style: { backgroundColor: "#0000ff" } }
    const { renderer, instance } = setup(props, resolve)

    hostConfig.commitUpdate(instance, "div", {}, props, null)

    expect(lastStyle(renderer)).toEqual({ padding: 16, backgroundColor: "#0000ff" })
  })

  it("lets the style prop beat a class in the hover state too", () => {
    // The style attribute outranks any selector, so an element declaring a
    // background inline keeps it while hovered. Only the key the style prop
    // set goes: the rest of the hover style stays.
    const { resolve } = tableResolver({
      "hover-blue": { hover: { backgroundColor: "#0000ff", padding: 8 } },
    })
    const props = { className: "hover-blue", style: { backgroundColor: "#ff0000" } }
    const { renderer, instance } = setup(props, resolve)

    hostConfig.commitUpdate(instance, "div", {}, props, null)

    expect(lastStyle(renderer)).toEqual({
      backgroundColor: "#ff0000",
      hover: { padding: 8 },
    })
  })

  it("merges the hover style of the prop over the hover style of a class", () => {
    const { resolve } = tableResolver({
      "hover-blue": { hover: { backgroundColor: "#0000ff", padding: 8 } },
    })
    const props = { className: "hover-blue", style: { hover: { backgroundColor: "#00ff00" } } }
    const { renderer, instance } = setup(props, resolve)

    hostConfig.commitUpdate(instance, "div", {}, props, null)

    expect(lastStyle(renderer)).toEqual({
      hover: { backgroundColor: "#00ff00", padding: 8 },
    })
  })

  it("keeps the class style when React hides and shows the element", () => {
    // This is the pair React drives for Suspense. Before `computeStyle` knew
    // about `className`, hiding an element dropped every class it had and
    // showing it again brought back only the inline prop.
    const { resolve } = tableResolver({
      "p-4": { padding: 16 },
      "bg-red": { backgroundColor: "#ff0000" },
    })
    const props = { className: "p-4 bg-red" }
    const { renderer, instance } = setup(props, resolve)

    hostConfig.hideInstance(instance)
    expect(lastStyle(renderer)).toMatchObject({
      visibility: "hidden",
      padding: 16,
      backgroundColor: "#ff0000",
    })

    hostConfig.unhideInstance(instance, props)
    const last = lastStyle(renderer)
    expect(last).toEqual({ padding: 16, backgroundColor: "#ff0000" })
    expect(last?.visibility).toBeUndefined()
  })

  it("merges two tokens on the same selector into one rule", () => {
    const { resolve } = tableResolver({
      "first-pad": { selectors: [{ on: ":first-child", style: { padding: 8 } }] },
      "first-red": {
        selectors: [{ on: ":first-child", style: { backgroundColor: "#ff0000" } }],
      },
      spaced: {
        selectors: [{ on: "& > :not(:last-child)", style: { marginBottom: 4 } }],
      },
    })
    const props = { className: "first-pad first-red spaced" }
    const { renderer, instance } = setup(props, resolve)

    hostConfig.commitUpdate(instance, "div", {}, props, null)

    expect(lastStyle(renderer)).toEqual({
      selectors: [
        { on: ":first-child", style: { padding: 8, backgroundColor: "#ff0000" } },
        { on: "& > :not(:last-child)", style: { marginBottom: 4 } },
      ],
    })
  })

  it("lets the style prop beat an index selector, and leaves child rules alone", () => {
    // `:first-child` styles this element, so a key the style prop sets goes.
    // `& > *` styles the children, and the inline style of this element says
    // nothing about those.
    const { resolve } = tableResolver({
      "first-red": {
        selectors: [
          { on: ":first-child", style: { backgroundColor: "#ff0000", padding: 8 } },
        ],
      },
      "children-red": {
        selectors: [{ on: "& > *", style: { backgroundColor: "#ff0000" } }],
      },
    })
    const props = {
      className: "first-red children-red",
      style: { backgroundColor: "#0000ff" },
    }
    const { renderer, instance } = setup(props, resolve)

    hostConfig.commitUpdate(instance, "div", {}, props, null)

    expect(lastStyle(renderer)).toEqual({
      backgroundColor: "#0000ff",
      selectors: [
        { on: ":first-child", style: { padding: 8 } },
        { on: "& > *", style: { backgroundColor: "#ff0000" } },
      ],
    })
  })

  it("drops the selector rules while the element is hidden", () => {
    // An index selector that sets `visibility` would otherwise paint an
    // element React asked to hide, the same hole `hover` had.
    const { resolve } = tableResolver({
      "first-peek": {
        padding: 16,
        selectors: [{ on: ":first-child", style: { visibility: "visible" } }],
      },
    })
    const { renderer, instance } = setup({ className: "first-peek" }, resolve)

    hostConfig.hideInstance(instance)

    expect(lastStyle(renderer)).toEqual({ padding: 16, visibility: "hidden" })
  })

  it("drops the hover style of a class while the element is hidden", () => {
    // A hover style that sets `visibility` would otherwise paint an element
    // React asked to hide.
    const { resolve } = tableResolver({
      "peek-on-hover": { padding: 16, hover: { visibility: "visible" } },
    })
    const { renderer, instance } = setup({ className: "peek-on-hover" }, resolve)

    hostConfig.hideInstance(instance)

    expect(lastStyle(renderer)).toEqual({ padding: 16, visibility: "hidden" })
  })

  it("asks the resolver once per token, not once per string", () => {
    // `clsx` writes a new string every time a flag flips, and the tokens in it
    // are the same. Asking per string would resolve `p-4` four times below.
    const { resolve, asked } = tableResolver({
      "p-4": { padding: 16 },
      "bg-red": { backgroundColor: "#ff0000" },
      "text-lg": { fontSize: 18 },
    })
    const { instance } = setup({}, resolve)

    for (const className of [
      "p-4",
      "p-4 bg-red",
      "p-4 text-lg",
      "p-4 bg-red text-lg",
      "p-4 bg-red",
    ]) {
      hostConfig.commitUpdate(instance, "div", {}, { className }, null)
    }

    expect(asked).toEqual(["p-4", "bg-red", "text-lg"])
  })

  it("warns once and sends the inline style when the root has no resolver", () => {
    const warnings: unknown[] = []
    const warn = console.warn
    console.warn = (...args: unknown[]) => warnings.push(args[0])
    try {
      const props = { className: "p-4", style: { padding: 8 } }
      const { renderer, instance } = setup(props)

      hostConfig.commitUpdate(instance, "div", {}, props, null)
      hostConfig.commitUpdate(instance, "div", {}, props, null)

      expect(lastStyle(renderer)).toEqual({ padding: 8 })
      expect(warnings).toHaveLength(1)
      expect(String(warnings[0])).toContain("resolveClassName")
    } finally {
      console.warn = warn
    }
  })
})

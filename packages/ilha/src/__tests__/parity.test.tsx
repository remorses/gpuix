import { describe, expect, test } from "vitest"

import { atom } from "ilha"
import type { View } from "ilha"

import type { HostElement } from "../host.js"
import { selectedText, textSearch, windowSize } from "../primitives.js"
import { createRoot, useGpuix, type Root } from "../root.js"
import {
  RecordingRenderer,
  asRenderer,
  click,
  keyDown,
} from "./recording-renderer.js"

const setup = (
  handlers: Parameters<typeof createRoot>[1] = {}
): { root: Root; renderer: RecordingRenderer } => {
  const renderer = new RecordingRenderer()
  return { renderer, root: createRoot(asRenderer(renderer), handlers) }
}

const sleep = (ms: number): Promise<void> => {
  const { promise, resolve } = Promise.withResolvers<void>()
  setTimeout(resolve, ms)
  return promise
}

const settle = async (root: Root): Promise<void> => {
  await sleep(2)
  root.flush()
}

describe("child order", () => {
  test("a component's output stays in front of its later siblings", async () => {
    const { root, renderer } = setup()
    const write: { grow?: () => void } = {}
    const Child = () => {
      const big = atom(false)
      write.grow = () => big.set(true)
      return big() ? (
        <div>
          <text>B</text>
        </div>
      ) : (
        <text>A</text>
      )
    }
    const App = () => (
      <div testId="row">
        <Child />
        <text>Z</text>
      </div>
    )
    root.render(App)
    await settle(root)
    const row = () => renderer.findByProp("testId", "row")?.id as number
    expect(renderer.textOf(row())).toBe("AZ")

    write.grow?.()
    await settle(root)
    expect(renderer.textOf(row())).toBe("BZ")
  })

  test("an atom hole that swaps text for an element keeps its place", async () => {
    const { root, renderer } = setup()
    const write: { swap?: () => void } = {}
    const App = () => {
      const middle = atom<View>("A")
      write.swap = () =>
        middle.set(
          <div>
            <text>B</text>
          </div>
        )
      return (
        <div testId="row">
          <text>X</text>
          {middle}
          <text>Z</text>
        </div>
      )
    }
    root.render(App)
    await settle(root)
    const row = () => renderer.findByProp("testId", "row")?.id as number
    expect(renderer.textOf(row())).toBe("XAZ")

    write.swap?.()
    await settle(root)
    expect(renderer.textOf(row())).toBe("XBZ")
  })

  test("a keyed list between siblings grows, shrinks and reorders in place", async () => {
    const { root, renderer } = setup()
    const write: { set?: (items: string[]) => void } = {}
    const App = () => {
      const items = atom(["a", "b", "c"])
      write.set = (value) => items.set(value)
      return (
        <div testId="list">
          <text>[</text>
          {items().map((item) => (
            <text key={item}>{item}</text>
          ))}
          <text>]</text>
        </div>
      )
    }
    root.render(App)
    await settle(root)
    const text = () =>
      renderer.textOf(renderer.findByProp("testId", "list")?.id as number)
    expect(text()).toBe("[abc]")

    for (const [items, expected] of [
      [["c", "a"], "[ca]"],
      [["d", "c", "b", "a"], "[dcba]"],
      [[], "[]"],
    ] as const) {
      write.set?.([...items])
      await settle(root)
      expect(text()).toBe(expected)
    }
    expect(renderer.errors).toEqual([])
  })

  test("a false conditional adds no native element and appears in place", async () => {
    const { root, renderer } = setup()
    const write: { show?: (value: boolean) => void } = {}
    const App = () => {
      const show = atom(false)
      write.show = (value) => show.set(value)
      return (
        <div testId="parent">
          <text>a</text>
          {show() && <text>b</text>}
          <text>c</text>
        </div>
      )
    }
    root.render(App)
    await settle(root)
    const parent = () => renderer.findByProp("testId", "parent")
    expect(parent()?.children).toHaveLength(2)
    const [first, last] = parent()?.children ?? []

    write.show?.(true)
    await settle(root)
    expect(renderer.textOf(parent()?.id as number)).toBe("abc")
    expect(parent()?.children[0]).toBe(first)
    expect(parent()?.children[2]).toBe(last)

    write.show?.(false)
    await settle(root)
    expect(parent()?.children).toEqual([first, last])
  })
})

describe("props", () => {
  test("a prop the next render drops is cleared natively", async () => {
    const { root, renderer } = setup()
    const write: { off?: () => void } = {}
    const App = () => {
      const on = atom(true)
      write.off = () => on.set(false)
      return (
        <text testId="body" {...(on() ? { highlight: { query: "a" } } : {})}>
          a
        </text>
      )
    }
    root.render(App)
    await settle(root)
    const body = () => renderer.findByProp("testId", "body")
    expect(body()?.props.highlight).toEqual({ query: "a" })

    write.off?.()
    await settle(root)
    expect(body()?.props.highlight).toBeNull()
  })

  test("a repaint that changes nothing sends nothing", async () => {
    const { root, renderer } = setup()
    const write: { bump?: () => void } = {}
    const App = () => {
      const count = atom(0)
      write.bump = () => count.update((n: number) => n + 1)
      return (
        <div
          testId="card"
          style={{ hover: { opacity: 0.5 }, padding: 4 }}
          highlight={{ query: "x" }}
          onclick={() => count()}
        >
          <text>static</text>
          <text>{String(count() > 100)}</text>
        </div>
      )
    }
    root.render(App)
    await settle(root)
    const sent = renderer.log.length

    write.bump?.()
    await settle(root)
    expect(renderer.log.slice(sent)).toEqual([])
  })

  test("<div> and <text> receive only the universal props", async () => {
    const { root, renderer } = setup()
    const stray: object = { placeholder: "ignored" }
    root.render(() => (
      <div testId="box" {...stray}>
        x
      </div>
    ))
    await settle(root)
    expect(renderer.findByProp("testId", "box")?.props).toEqual({
      testId: "box",
    })
  })

  test("a ref can call a host method on first paint", async () => {
    const { root, renderer } = setup()
    const seen: Array<HostElement | null> = []
    root.render(() => (
      <div
        testId="target"
        ref={(node) => {
          seen.push(node)
          node?.scrollIntoView()
        }}
      >
        x
      </div>
    ))
    await settle(root)
    const target = renderer.findByProp("testId", "target")
    expect(renderer.scrolledIntoView).toEqual([target?.id])
    expect(seen[0]?.id).toBe(target?.id)
  })
})

describe("events", () => {
  test("GPUIX-only events bind through their camelCase props", async () => {
    const { root, renderer } = setup()
    const seen: string[] = []
    root.render(() => (
      <markdown
        testId="doc"
        source="[a](b)"
        onLinkClick={(event) => seen.push(`link:${event.value}`)}
        onMouseDownOutside={() => seen.push("outside")}
        onHighlight={() => seen.push("highlight")}
      />
    ))
    await settle(root)
    const doc = renderer.findByProp("testId", "doc")
    expect([...(doc?.events ?? [])].sort()).toEqual([
      "highlight",
      "linkClick",
      "mouseDownOutside",
    ])

    root.dispatch({
      elementId: doc?.id as number,
      eventType: "linkClick",
      value: "b",
    })
    expect(seen).toEqual(["link:b"])
  })

  test("event props that share a GPUIX event all fire", async () => {
    const { root, renderer } = setup()
    const seen: string[] = []
    root.render(() => (
      <input
        testId="field"
        value=""
        oninput={() => seen.push("input")}
        onchange={() => seen.push("change")}
        onauxclick={() => seen.push("aux")}
        oncontextmenu={() => seen.push("menu")}
      />
    ))
    await settle(root)
    const id = renderer.findByProp("testId", "field")?.id as number

    root.dispatch({ elementId: id, eventType: "change", value: "x" })
    expect(seen).toEqual(["input", "change"])

    // `contextmenu` is the right button only; `auxclick` is every other one.
    root.dispatch({ elementId: id, eventType: "auxClick", isRightClick: false })
    root.dispatch({ elementId: id, eventType: "auxClick", isRightClick: true })
    expect(seen.slice(2)).toEqual(["aux", "aux", "menu"])
  })

  test("preventDefault on keydown cancels the Tab default", async () => {
    const { root, renderer } = setup()
    root.render(() => (
      <div testId="trap" onkeydown={(event) => event.preventDefault()}>
        x
      </div>
    ))
    await settle(root)
    const id = renderer.findByProp("testId", "trap")?.id as number

    root.dispatch({ ...keyDown(id, "tab"), keystrokeId: 7 })
    // The window event of the same keystroke runs the default action.
    root.dispatch({
      elementId: 1,
      eventType: "windowKeyDown",
      key: "tab",
      keystrokeId: 7,
    })
    expect(renderer.focusNextCalls).toBe(0)

    root.dispatch({
      elementId: 1,
      eventType: "windowKeyDown",
      key: "tab",
      keystrokeId: 8,
    })
    expect(renderer.focusNextCalls).toBe(1)
  })

  test("a controlled input does not echo the typed value back", async () => {
    const { root, renderer } = setup()
    const App = () => {
      const value = atom("")
      return (
        <input
          testId="field"
          value={value}
          oninput={(event) => value.set(String(event.value))}
        />
      )
    }
    root.render(App)
    await settle(root)
    const id = renderer.findByProp("testId", "field")?.id as number
    const sent = renderer.log.length

    root.dispatch({ elementId: id, eventType: "change", value: "hi" })
    await settle(root)
    expect(
      renderer.log.slice(sent).filter(([op]) => op === "setCustomProp")
    ).toEqual([])
  })
})

describe("root", () => {
  test("a failing root component reaches onUncaughtError", async () => {
    const errors: Error[] = []
    const { root } = setup({ onUncaughtError: (error) => errors.push(error) })
    const original = console.error
    console.error = () => undefined
    try {
      root.render(() => {
        throw new Error("boom")
      })
      await settle(root)
    } finally {
      console.error = original
    }
    expect(errors.map((error) => error.message)).toEqual(["boom"])
  })

  test("a failing nested component reaches onUncaughtError", async () => {
    const errors: Error[] = []
    const { root, renderer } = setup({
      onUncaughtError: (error) => errors.push(error),
    })
    const Broken = (): View => {
      throw new Error("nested")
    }
    const original = console.error
    console.error = () => undefined
    try {
      root.render(() => (
        <div testId="shell">
          <text>ok</text>
          <Broken />
        </div>
      ))
      await settle(root)
    } finally {
      console.error = original
    }
    expect(errors.map((error) => error.message)).toEqual(["nested"])
    // The parent stays up and the hole paints ilha's error view.
    const shell = renderer.findByProp("testId", "shell")
    expect(renderer.textOf(shell?.id as number)).toBe("oknested")
  })

  test("useGpuix reads the host in nested and keyed components", async () => {
    const { root, renderer } = setup()
    const seen: unknown[] = []
    const write: { bump?: () => void } = {}
    const Row = () => {
      seen.push(useGpuix()?.renderer)
      return <text>row</text>
    }
    const App = () => {
      const count = atom(0)
      write.bump = () => count.update((n: number) => n + 1)
      return (
        <div>
          <text>{count()}</text>
          <div>
            <Row key="keyed" />
          </div>
          <Row />
        </div>
      )
    }
    root.render(App)
    await settle(root)
    write.bump?.()
    await settle(root)

    expect(seen.length).toBeGreaterThan(2)
    expect(seen.every((value) => value === asRenderer(renderer))).toBe(true)
  })

  test("render replaces the tree, and a new root can take the renderer", async () => {
    const { root, renderer } = setup()
    root.render(() => <text>one</text>)
    await settle(root)
    root.render(() => <text>two</text>)
    await settle(root)
    expect(renderer.textOf(renderer.rootId as number)).toBe("two")

    root.unmount()
    await settle(root)
    expect(renderer.nodes.size).toBe(0)

    const next = createRoot(asRenderer(renderer))
    next.render(() => <text testId="three">three</text>)
    await settle(next)
    expect(renderer.textOf(renderer.rootId as number)).toBe("three")
    root.dispatch(click(renderer.findByProp("testId", "three")?.id as number))
    expect(renderer.errors).toEqual([])
  })
})

describe("component lifetime", () => {
  test("a keyed component the view drops is destroyed and returns fresh", async () => {
    const { root, renderer } = setup()
    const write: { show?: (value: boolean) => void } = {}
    const Row = () => {
      const count = atom(0)
      return (
        <text testId="row" onclick={() => count.update((n: number) => n + 1)}>
          {count}
        </text>
      )
    }
    const App = () => {
      const show = atom(true)
      write.show = (value) => show.set(value)
      return <div testId="list">{show() ? <Row key="a" /> : <text>none</text>}</div>
    }
    root.render(App)
    await settle(root)
    const row = () => renderer.findByProp("testId", "row")
    const first = row()?.id as number
    root.dispatch(click(first))
    await settle(root)
    expect(renderer.textOf(first)).toBe("1")

    write.show?.(false)
    await settle(root)
    expect(renderer.destroyed).toContain(first)
    expect(row()).toBeUndefined()

    write.show?.(true)
    await settle(root)
    expect(row()?.id).not.toBe(first)
    expect(renderer.textOf(row()?.id as number)).toBe("0")
    expect(renderer.errors).toEqual([])
  })

  test("state the view never reads survives between events", async () => {
    const { root, renderer } = setup()
    const App = () => {
      const shown = atom(0)
      const clicks = atom(0)
      return (
        <text
          testId="tally"
          onclick={() => {
            clicks.update((n: number) => n + 1)
            shown.set(clicks())
          }}
        >
          {shown}
        </text>
      )
    }
    root.render(App)
    await settle(root)
    const id = renderer.findByProp("testId", "tally")?.id as number
    for (let press = 0; press < 3; press += 1) {
      root.dispatch(click(id))
      await settle(root)
    }
    expect(renderer.textOf(id)).toBe("3")
  })
})

describe("primitives", () => {
  test("windowSize, selectedText and textSearch follow the host", async () => {
    const { root, renderer } = setup()
    renderer.selection = "picked"
    const App = () => {
      const size = windowSize({ intervalMs: 16 })
      const selection = selectedText()
      const search = textSearch({ query: "ne" })
      return (
        <div testId="search" {...search.props}>
          <text testId="size">{`${size().width}x${size().height}`}</text>
          <text testId="selection">{selection() ?? "none"}</text>
          <text testId="matches">{`${search.active}/${search.total}`}</text>
          <text>needle</text>
        </div>
      )
    }
    root.render(App)
    await settle(root)
    const text = (testId: string) =>
      renderer.textOf(renderer.findByProp("testId", testId)?.id as number)
    expect(text("size")).toBe("800x600")
    expect(text("selection")).toBe("picked")
    expect(renderer.windowSelection).toBe(true)

    const search = renderer.findByProp("testId", "search")
    expect(search?.props.highlight).toMatchObject({
      activeIndex: 0,
      query: "ne",
    })
    root.dispatch({
      elementId: search?.id as number,
      eventType: "highlight",
      matchCount: 3,
    })
    await settle(root)
    expect(text("matches")).toBe("0/3")

    renderer.windowSize = { height: 700, width: 1000 }
    await sleep(40)
    root.flush()
    expect(text("size")).toBe("1000x700")

    root.unmount()
    await settle(root)
    expect(renderer.windowSelection).toBe(false)
  })
})

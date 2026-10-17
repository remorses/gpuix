import { describe, expect, test } from "vitest"

import * as Stream from "effect/Stream"
import { atom, h } from "ilha"

import { createRoot, useGpuix, type Root } from "../root.js"
import {
  RecordingRenderer,
  asRenderer,
  click,
} from "./recording-renderer.js"

const setup = (): { root: Root; renderer: RecordingRenderer } => {
  const renderer = new RecordingRenderer()
  return { renderer, root: createRoot(asRenderer(renderer)) }
}

const sleep = (ms: number): Promise<void> => {
  const { promise, resolve } = Promise.withResolvers<void>()
  setTimeout(resolve, ms)
  return promise
}

/** Let ilha's queued work and the pass-finish microtasks run, then flush. */
const settle = async (root: Root): Promise<void> => {
  await sleep(2)
  root.flush()
}

describe("mounting", () => {
  test("paints a component into a single GPUIX root", async () => {
    const { root, renderer } = setup()
    const App = () => (
      <div style={{ display: "flex", gap: 8, padding: 12 }}>
        <text style={{ color: "#ff0000" }}>hello</text>
      </div>
    )
    root.render(App)
    await settle(root)

    const container = renderer.nodes.get(renderer.rootId as number)
    expect(container?.type).toBe("div")
    expect(container?.style).toMatchObject({
      display: "flex",
      flexDirection: "column",
      height: "100%",
      width: "100%",
    })

    const [appId] = container?.children ?? []
    const app = renderer.nodes.get(appId as number)
    expect(app?.type).toBe("div")
    expect(app?.style).toMatchObject({
      display: "flex",
      gap: 8,
      padding: 12,
    })

    const [textId] = app?.children ?? []
    const text = renderer.nodes.get(textId as number)
    expect(text?.type).toBe("text")
    expect(text?.style).toMatchObject({ color: "#ff0000" })
    expect(renderer.textOf(appId as number)).toBe("hello")
  })

  test("paints every root child inside the single GPUIX root", async () => {
    const { root, renderer } = setup()
    const App = () => [<text key="a">a</text>, <text key="b">b</text>]
    root.render(App)
    await settle(root)

    const container = renderer.nodes.get(renderer.rootId as number)
    expect(container?.children).toHaveLength(2)
    expect(renderer.textOf(renderer.rootId as number)).toBe("ab")
  })

  test("keeps native element ids across a repaint", async () => {
    const { root, renderer } = setup()
    const write: { set?: (value: string) => void } = {}
    const App = () => {
      const label = atom("first")
      write.set = (value: string) => label.set(value)
      return <div style={{ padding: 4 }}>{label}</div>
    }
    root.render(App)
    await settle(root)

    const containerId = renderer.rootId as number
    const before = renderer.descendantsOf(containerId)
    expect(before).toHaveLength(2)

    write.set?.("second")
    await settle(root)

    const after = renderer.descendantsOf(containerId)
    expect(after).toEqual(before)
    expect(renderer.textOf(before[0] as number)).toBe("second")
  })

  test("keeps a focused input's element across a repaint", async () => {
    const { root, renderer } = setup()
    const write: { set?: (value: string) => void } = {}
    const App = () => {
      const value = atom("")
      write.set = (next: string) => value.set(next)
      return <input autoFocus value={value} oninput={() => undefined} />
    }
    root.render(App)
    await settle(root)

    const inputId = renderer.descendantsOf(renderer.rootId as number)[0]
    expect(renderer.nodes.get(inputId as number)?.type).toBe("input")

    write.set?.("typed")
    await settle(root)

    expect(renderer.descendantsOf(renderer.rootId as number)[0]).toBe(inputId)
    expect(renderer.nodes.get(inputId as number)?.props.value).toBe("typed")
  })
})

describe("props", () => {
  test("passes object-valued props through the raw prop bag", async () => {
    const { root, renderer } = setup()
    const App = () => (
      <text highlight={{ query: "needle" }} testId="body">
        needle
      </text>
    )
    root.render(App)
    await settle(root)

    const node = renderer.findByProp("testId", "body")
    expect(node?.props.highlight).toEqual({ query: "needle" })
  })

  test("converts a CSS string style into a StyleDesc", async () => {
    const { root, renderer } = setup()
    const App = () => (
      <div style="display:flex;background-color:#1e1e2e;padding:12px;width:100%">
        x
      </div>
    )
    root.render(App)
    await settle(root)

    const node = renderer.find(
      (candidate) => candidate.props.backgroundColor === undefined &&
        candidate.style.backgroundColor === "#1e1e2e"
    )
    expect(node?.style).toMatchObject({
      backgroundColor: "#1e1e2e",
      display: "flex",
      padding: 12,
      width: "100%",
    })
  })

  test("keeps nested hover styles from a style object", async () => {
    const { root, renderer } = setup()
    const App = () => (
      <div
        testId="card"
        style={{
          backgroundColor: "#232323",
          hover: { backgroundColor: "#2c2c2c" },
          padding: 12,
        }}
      >
        x
      </div>
    )
    root.render(App)
    await settle(root)

    const card = renderer.findByProp("testId", "card")
    expect(card?.style).toMatchObject({
      backgroundColor: "#232323",
      hover: { backgroundColor: "#2c2c2c" },
      padding: 12,
    })
  })

  test("warns once for an unsupported tag", async () => {
    const { root } = setup()
    const warnings: string[] = []
    const original = console.warn
    console.warn = (message: string) => warnings.push(message)
    try {
      root.render(() => h("marquee", null, "x"))
      await settle(root)
    } finally {
      console.warn = original
    }
    expect(warnings.filter((line) => line.includes("marquee"))).toHaveLength(1)
  })
})

describe("events", () => {
  test("routes a native event to the ilha handler", async () => {
    const { root, renderer } = setup()
    const clicks: number[] = []
    const App = () => (
      <div testId="button" onclick={() => clicks.push(1)}>
        press
      </div>
    )
    root.render(App)
    await settle(root)

    const button = renderer.findByProp("testId", "button")
    expect(button?.events.has("click")).toBe(true)

    root.dispatch(click(button?.id as number))
    expect(clicks).toEqual([1])
  })

  test("binds lowercase oninput to the native change event", async () => {
    const { root, renderer } = setup()
    const typed: string[] = []
    const App = () => (
      <input
        testId="field"
        value=""
        oninput={(event) => typed.push(String(event.value))}
      />
    )
    root.render(App)
    await settle(root)

    const field = renderer.findByProp("testId", "field")
    expect(field?.events.has("change")).toBe(true)

    root.dispatch({
      elementId: field?.id as number,
      eventType: "change",
      value: "hi",
    })
    expect(typed).toEqual(["hi"])
  })
})

describe("repaint edge cases", () => {
  test("an atom bound to an input value survives the prop bag", async () => {
    const { root, renderer } = setup()
    const App = () => {
      const value = atom("hello")
      return <input testId="field" value={value} />
    }
    root.render(App)
    await settle(root)

    const field = renderer.findByProp("testId", "field")
    expect(field?.props.value).toBe("hello")
  })

  test("drops a handler the next render removes", async () => {
    const { root, renderer } = setup()
    const write: { off?: () => void } = {}
    const clicks: number[] = []
    const App = () => {
      const on = atom(true)
      write.off = () => on.set(false)
      return (
        <div
          testId="target"
          {...(on() ? { onclick: () => clicks.push(1) } : {})}
        >
          press
        </div>
      )
    }
    root.render(App)
    await settle(root)

    const target = renderer.findByProp("testId", "target")
    expect(target?.events.has("click")).toBe(true)
    root.dispatch(click(target?.id as number))
    expect(clicks).toEqual([1])

    write.off?.()
    await settle(root)

    const after = renderer.findByProp("testId", "target")
    expect(after?.id).toBe(target?.id)
    expect(after?.events.has("click")).toBe(false)
    root.dispatch(click(after?.id as number))
    expect(clicks).toEqual([1])
  })
})

describe("components", () => {
  test("a nested component adds no wrapper element", async () => {
    const { root, renderer } = setup()
    const Child = () => <text testId="child">child</text>
    const App = () => (
      <div testId="row" style={{ display: "flex" }}>
        <Child />
        <Child />
      </div>
    )
    root.render(App)
    await settle(root)

    const row = renderer.findByProp("testId", "row")
    const children = row?.children ?? []
    expect(children).toHaveLength(2)
    for (const childId of children) {
      expect(renderer.nodes.get(childId)?.type).toBe("text")
      expect(renderer.nodes.get(childId)?.props.testId).toBe("child")
    }
  })

  test("a nested component keeps its state when the parent repaints", async () => {
    const { root, renderer } = setup()
    const write: { bump?: () => void } = {}
    const Child = () => {
      const count = atom(0)
      return (
        <text testId="child" onclick={() => count.update((n: number) => n + 1)}>
          {count}
        </text>
      )
    }
    const App = () => {
      const parent = atom(0)
      write.bump = () => parent.update((n: number) => n + 1)
      return (
        <div>
          <text testId="parent">{parent}</text>
          <Child />
        </div>
      )
    }
    root.render(App)
    await settle(root)

    const child = renderer.findByProp("testId", "child")
    root.dispatch(click(child?.id as number))
    await settle(root)
    expect(renderer.textOf(child?.id as number)).toBe("1")

    write.bump?.()
    await settle(root)

    const after = renderer.findByProp("testId", "child")
    expect(after?.id).toBe(child?.id)
    expect(renderer.textOf(after?.id as number)).toBe("1")
  })

  test("reorders keyed rows without recreating them", async () => {
    const { root, renderer } = setup()
    const write: { flip?: () => void } = {}
    const App = () => {
      const order = atom(["a", "b"])
      write.flip = () => order.set(["b", "a"])
      return (
        <div>
          {order().map((id) => (
            <text key={id} testId={`row-${id}`}>
              {id}
            </text>
          ))}
        </div>
      )
    }
    root.render(App)
    await settle(root)

    const rowA = renderer.findByProp("testId", "row-a")
    const rowB = renderer.findByProp("testId", "row-b")

    write.flip?.()
    await settle(root)

    expect(renderer.findByProp("testId", "row-a")?.id).toBe(rowA?.id)
    expect(renderer.findByProp("testId", "row-b")?.id).toBe(rowB?.id)

    const parent = renderer.findByProp("testId", "row-a")?.id
    const container = renderer.find(
      (node) => node.children.length === 2 && node.children[0] === rowB?.id
    )
    expect(container?.children).toEqual([rowB?.id, rowA?.id])
    expect(parent).toBeDefined()
  })
})

describe("ilha features", () => {
  test("an async component paints once it resolves", async () => {
    const { root, renderer } = setup()
    const Page = async () => {
      await sleep(1)
      return <text testId="page">ready</text>
    }
    root.render(Page)
    await settle(root)

    expect(renderer.findByProp("testId", "page")).toBeDefined()
    expect(renderer.textOf(renderer.rootId as number)).toBe("ready")
  })

  test("a generator component paints a stream of views", async () => {
    const { root, renderer } = setup()
    function* Ticks() {
      yield Stream.map(Stream.fromIterable(["one", "two"]), (value: string) => (
        <text testId="tick">{value}</text>
      ))
    }
    root.render(Ticks)
    await settle(root)

    expect(renderer.findByProp("testId", "tick")).toBeDefined()
    expect(renderer.textOf(renderer.rootId as number)).toBe("two")
  })

  test("useGpuix reads the host renderer from inside a component", async () => {
    const { root, renderer } = setup()
    const seen: unknown[] = []
    const App = () => {
      seen.push(useGpuix()?.renderer)
      return <text>host</text>
    }
    root.render(App)
    await settle(root)

    expect(seen[0]).toBe(asRenderer(renderer))
  })
})

describe("teardown", () => {
  test("unmount destroys every element it created", async () => {
    const { root, renderer } = setup()
    root.render(() => <div>bye</div>)
    await settle(root)

    const created = renderer.allIds()
    expect(created.length).toBeGreaterThan(0)

    root.unmount()
    await settle(root)

    expect(renderer.nodes.size).toBe(0)
    for (const id of created) {
      expect(renderer.destroyed).toContain(id)
    }
  })
})

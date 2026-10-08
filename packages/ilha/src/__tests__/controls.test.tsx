import { describe, expect, test } from "vitest"

import { atom } from "ilha"

import {
  AnimatePresence,
  Button,
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
  ComboboxValue,
  Dialog,
  DialogBackdrop,
  DialogClose,
  DialogPopup,
  DialogPortal,
  DialogTitle,
  DialogTrigger,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
  motion,
  usePresence,
} from "../index.js"
import { createRoot, type Root } from "../root.js"
import {
  RecordingRenderer,
  asRenderer,
  click,
  keyDown,
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

/** Long enough for a dismiss layer, which registers on a task, to open. */
const settle = async (root: Root): Promise<void> => {
  await sleep(5)
  root.flush()
}

const idOf = (renderer: RecordingRenderer, testId: string): number => {
  const node = renderer.findByProp("testId", testId)
  if (!node) {
    throw new Error(`no element with testId "${testId}"`)
  }
  return node.id
}

/** One keystroke on the window, the way native reports Escape and Tab. */
const windowKey = (root: Root, key: string, keystrokeId: number): void => {
  root.dispatch({ elementId: 1, eventType: "windowKeyDown", key, keystrokeId })
}

describe("Button", () => {
  test("clicks on press, Enter and Space, and is a tab stop", async () => {
    const { root, renderer } = setup()
    const clicks: string[] = []
    root.render(() => (
      <Button testId="save" onclick={() => clicks.push("click")}>
        <text>Save</text>
      </Button>
    ))
    await settle(root)
    const save = renderer.findByProp("testId", "save")
    expect(save?.props).toMatchObject({ role: "button", tabIndex: 0 })

    const id = save?.id as number
    root.dispatch(click(id))
    root.dispatch(keyDown(id, "enter"))
    // Space fires on release, like a browser button.
    root.dispatch(keyDown(id, "space"))
    expect(clicks).toHaveLength(2)
    root.dispatch({ elementId: id, eventType: "keyUp", key: "space" })
    expect(clicks).toHaveLength(3)
  })

  test("a disabled button ignores presses and leaves the tab order", async () => {
    const { root, renderer } = setup()
    const clicks: string[] = []
    root.render(() => (
      <Button
        testId="save"
        disabled
        onClick={() => clicks.push("click")}
        style={(state) => ({ opacity: state.disabled ? 0.4 : 1 })}
      >
        <text>Save</text>
      </Button>
    ))
    await settle(root)
    const save = renderer.findByProp("testId", "save")
    expect(save?.props.tabIndex).toBe(-1)
    expect(save?.style).toEqual({ opacity: 0.4 })

    root.dispatch(click(save?.id as number))
    root.dispatch(keyDown(save?.id as number, "enter"))
    expect(clicks).toEqual([])
  })

  test("asChild puts the behaviour on the child and keeps its handler", async () => {
    const { root, renderer } = setup()
    const order: string[] = []
    root.render(() => (
      <Button asChild onclick={() => order.push("button")}>
        <div testId="row" onClick={() => order.push("child")}>
          <text>Row</text>
        </div>
      </Button>
    ))
    await settle(root)
    const row = renderer.findByProp("testId", "row")
    expect(row?.props.role).toBe("button")
    expect([...(row?.events ?? [])]).toContain("click")

    root.dispatch(click(row?.id as number))
    expect(order).toEqual(["child", "button"])
  })
})

describe("Select", () => {
  const Picker = (props: { onChange: (value: string) => void }) => {
    const value = atom("a")
    return (
      <Select
        items={[
          { label: "Alpha", value: "a" },
          { label: "Beta", value: "b" },
        ]}
        value={value()}
        onValueChange={(next) => {
          value.set(next)
          props.onChange(next)
        }}
      >
        <SelectTrigger
          testId="trigger"
          style={(state) => ({ opacity: state.open ? 0.5 : 1 })}
        >
          <SelectValue testId="value" />
        </SelectTrigger>
        <SelectContent testId="menu">
          <SelectItem value="a" testId="item-a">
            {(state) => <text>{state.selected ? "[Alpha]" : "Alpha"}</text>}
          </SelectItem>
          <SelectItem
            value="b"
            testId="item-b"
            style={(state) => ({ opacity: state.highlighted ? 1 : 0.6 })}
          >
            <text>Beta</text>
          </SelectItem>
          <SelectItem value="c" testId="item-c" disabled>
            <text>Gamma</text>
          </SelectItem>
        </SelectContent>
      </Select>
    )
  }

  test("opens from the trigger and selects with a click", async () => {
    const { root, renderer } = setup()
    const changes: string[] = []
    root.render(() => <Picker onChange={(value) => changes.push(value)} />)
    await settle(root)
    expect(renderer.textOf(idOf(renderer, "value"))).toBe("Alpha")
    expect(renderer.findByProp("testId", "menu")).toBeUndefined()

    root.dispatch(click(idOf(renderer, "trigger")))
    await settle(root)
    expect(renderer.findByProp("testId", "trigger")?.style).toEqual({
      opacity: 0.5,
    })
    expect(renderer.textOf(idOf(renderer, "menu"))).toBe("[Alpha]BetaGamma")

    root.dispatch(click(idOf(renderer, "item-b")))
    await settle(root)
    expect(changes).toEqual(["b"])
    expect(renderer.findByProp("testId", "menu")).toBeUndefined()
    expect(renderer.textOf(idOf(renderer, "value"))).toBe("Beta")
    // Closing hands focus back to the trigger.
    expect(renderer.getFocusedElementId()).toBe(idOf(renderer, "trigger"))
  })

  test("moves with the arrow keys, skips disabled items and selects on Enter", async () => {
    const { root, renderer } = setup()
    const changes: string[] = []
    root.render(() => <Picker onChange={(value) => changes.push(value)} />)
    await settle(root)

    root.dispatch(keyDown(idOf(renderer, "trigger"), "down"))
    await settle(root)
    const menu = idOf(renderer, "menu")
    root.dispatch(keyDown(menu, "down"))
    await settle(root)
    expect(renderer.findByProp("testId", "item-b")?.style).toEqual({
      opacity: 1,
    })

    // The disabled third item is not a stop: down wraps to the first.
    root.dispatch(keyDown(menu, "down"))
    root.dispatch(keyDown(menu, "down"))
    root.dispatch(keyDown(menu, "enter"))
    await settle(root)
    expect(changes).toEqual(["b"])

    root.dispatch(click(idOf(renderer, "trigger")))
    await settle(root)
    root.dispatch(click(idOf(renderer, "item-c")))
    await settle(root)
    expect(changes).toEqual(["b"])
  })

  test("Escape and a press outside close the menu", async () => {
    const { root, renderer } = setup()
    root.render(() => <Picker onChange={() => undefined} />)
    await settle(root)

    root.dispatch(click(idOf(renderer, "trigger")))
    await settle(root)
    windowKey(root, "escape", 1)
    await settle(root)
    expect(renderer.findByProp("testId", "menu")).toBeUndefined()

    root.dispatch(click(idOf(renderer, "trigger")))
    await settle(root)
    root.dispatch({
      elementId: idOf(renderer, "menu"),
      eventType: "mouseDownOutside",
    })
    await settle(root)
    expect(renderer.findByProp("testId", "menu")).toBeUndefined()
  })
})

describe("Dialog", () => {
  const Sheet = () => (
    <Dialog>
      <DialogTrigger testId="open">
        <text>Open</text>
      </DialogTrigger>
      <DialogPortal testId="layer">
        <DialogBackdrop testId="backdrop" />
        <DialogPopup testId="popup">
          <DialogTitle>
            <text>Settings</text>
          </DialogTitle>
          <DialogClose testId="close">
            <text>Close</text>
          </DialogClose>
        </DialogPopup>
      </DialogPortal>
    </Dialog>
  )

  test("opens over the window, takes focus and returns it on close", async () => {
    const { root, renderer } = setup()
    root.render(Sheet)
    await settle(root)
    expect(renderer.findByProp("testId", "layer")).toBeUndefined()

    const trigger = idOf(renderer, "open")
    root.dispatch(click(trigger))
    await settle(root)
    const layer = renderer.findByProp("testId", "layer")
    expect(layer?.type).toBe("anchored")
    expect(layer?.props).toMatchObject({ fill: "window", occlude: true })
    const popup = renderer.findByProp("testId", "popup")
    expect(popup?.props).toMatchObject({ role: "dialog", tabIndex: -1 })
    expect(renderer.getFocusedElementId()).toBe(popup?.id)

    root.dispatch(click(idOf(renderer, "close")))
    await settle(root)
    expect(renderer.findByProp("testId", "layer")).toBeUndefined()
    expect(renderer.getFocusedElementId()).toBe(trigger)
  })

  test("Escape and the backdrop close it, and Tab stays inside", async () => {
    const { root, renderer } = setup()
    root.render(Sheet)
    await settle(root)

    root.dispatch(click(idOf(renderer, "open")))
    await settle(root)
    const popup = idOf(renderer, "popup")
    root.dispatch({ ...keyDown(popup, "tab"), keystrokeId: 1 })
    windowKey(root, "tab", 1)
    expect(renderer.focusedWithin).toEqual([["next", popup]])
    expect(renderer.focusNextCalls).toBe(0)

    windowKey(root, "escape", 2)
    await settle(root)
    expect(renderer.findByProp("testId", "layer")).toBeUndefined()

    root.dispatch(click(idOf(renderer, "open")))
    await settle(root)
    root.dispatch({
      elementId: idOf(renderer, "backdrop"),
      eventType: "mouseDown",
    })
    await settle(root)
    expect(renderer.findByProp("testId", "layer")).toBeUndefined()
  })

  test("a controlled dialog follows its `open` prop", async () => {
    const { root, renderer } = setup()
    const write: { open?: (value: boolean) => void } = {}
    const requests: boolean[] = []
    const App = () => {
      const open = atom(false)
      write.open = (value) => open.set(value)
      return (
        <Dialog open={open()} onOpenChange={(next) => requests.push(next)}>
          <DialogPortal>
            <DialogPopup testId="popup">
              <DialogClose testId="close" />
            </DialogPopup>
          </DialogPortal>
        </Dialog>
      )
    }
    root.render(App)
    await settle(root)
    write.open?.(true)
    await settle(root)
    expect(renderer.findByProp("testId", "popup")).toBeDefined()

    // The owner decides: a close request alone does not close it.
    root.dispatch(click(idOf(renderer, "close")))
    await settle(root)
    expect(requests).toEqual([false])
    expect(renderer.findByProp("testId", "popup")).toBeDefined()

    write.open?.(false)
    await settle(root)
    expect(renderer.findByProp("testId", "popup")).toBeUndefined()
  })
})

describe("motion.div", () => {
  test("a new target starts a new generation and stale completions are dropped", async () => {
    const { root, renderer } = setup()
    const write: { collapse?: () => void } = {}
    const done: number[] = []
    const App = () => {
      const collapsed = atom(false)
      write.collapse = () => collapsed.set(true)
      return (
        <motion.div
          testId="panel"
          initial={false}
          animate={{ width: collapsed() ? 0 : 200 }}
          transition={{ duration: 0.2 }}
          onMotionComplete={(event) => done.push(event.motionGeneration ?? 0)}
        />
      )
    }
    root.render(App)
    await settle(root)
    const panel = () => renderer.findByProp("testId", "panel")
    expect(panel()?.props.motion).toMatchObject({
      animate: { width: 200 },
      generation: 1,
      initial: false,
    })

    write.collapse?.()
    await settle(root)
    expect(panel()?.props.motion).toMatchObject({
      animate: { width: 0 },
      generation: 2,
    })

    const id = panel()?.id as number
    root.dispatch({ elementId: id, eventType: "motionComplete", motionGeneration: 1 })
    root.dispatch({ elementId: id, eventType: "motionComplete", motionGeneration: 2 })
    expect(done).toEqual([2])
  })
})

describe("Tooltip", () => {
  test("opens on hover and on focus, closes on leave, press and Escape", async () => {
    const { root, renderer } = setup()
    root.render(() => (
      <Tooltip disableHoverableContent>
        <TooltipTrigger testId="trigger">
          <text>Save</text>
        </TooltipTrigger>
        <TooltipContent testId="tip">
          <text>Saves the file</text>
        </TooltipContent>
      </Tooltip>
    ))
    await settle(root)
    const trigger = idOf(renderer, "trigger")
    const shown = () => renderer.findByProp("testId", "tip") !== undefined
    const fire = async (eventType: string) => {
      root.dispatch({ elementId: trigger, eventType })
      await settle(root)
    }
    expect(shown()).toBe(false)

    await fire("mouseEnter")
    expect(shown()).toBe(true)
    // Anchored above the trigger, centred, like Base UI.
    const tip = renderer.findByProp("testId", "tip")
    const layer = renderer.find((node) => node.children.includes(tip?.id as number))
    expect(layer?.props).toMatchObject({ align: "center", side: "top" })
    await fire("mouseLeave")
    expect(shown()).toBe(false)

    await fire("focus")
    expect(shown()).toBe(true)
    await fire("mouseDown")
    expect(shown()).toBe(false)

    await fire("mouseEnter")
    windowKey(root, "escape", 1)
    await settle(root)
    expect(shown()).toBe(false)
  })

  test("waits for the provider's delay, and skips it right after another closed", async () => {
    const { root, renderer } = setup()
    root.render(() => (
      <TooltipProvider delayDuration={40} skipDelayDuration={500} disableHoverableContent>
        <Tooltip>
          <TooltipTrigger testId="first" />
          <TooltipContent testId="first-tip" />
        </Tooltip>
        <Tooltip>
          <TooltipTrigger testId="second" />
          <TooltipContent testId="second-tip" />
        </Tooltip>
      </TooltipProvider>
    ))
    await settle(root)
    const shown = (testId: string) =>
      renderer.findByProp("testId", testId) !== undefined

    root.dispatch({ elementId: idOf(renderer, "first"), eventType: "mouseEnter" })
    await settle(root)
    expect(shown("first-tip")).toBe(false)
    await sleep(50)
    root.flush()
    expect(shown("first-tip")).toBe(true)

    root.dispatch({ elementId: idOf(renderer, "first"), eventType: "mouseLeave" })
    root.dispatch({ elementId: idOf(renderer, "second"), eventType: "mouseEnter" })
    await settle(root)
    expect(shown("first-tip")).toBe(false)
    expect(shown("second-tip")).toBe(true)
  })

  test("a hoverable tooltip stays open while the pointer is on it", async () => {
    const { root, renderer } = setup()
    root.render(() => (
      <Tooltip>
        <TooltipTrigger testId="trigger" />
        <TooltipContent testId="tip" />
      </Tooltip>
    ))
    await settle(root)
    root.dispatch({ elementId: idOf(renderer, "trigger"), eventType: "mouseEnter" })
    await settle(root)
    root.dispatch({ elementId: idOf(renderer, "trigger"), eventType: "mouseLeave" })
    root.dispatch({ elementId: idOf(renderer, "tip"), eventType: "mouseEnter" })
    await sleep(120)
    root.flush()
    expect(renderer.findByProp("testId", "tip")).toBeDefined()

    root.dispatch({ elementId: idOf(renderer, "tip"), eventType: "mouseLeave" })
    await sleep(120)
    root.flush()
    expect(renderer.findByProp("testId", "tip")).toBeUndefined()
  })
})

describe("Combobox", () => {
  const FRUITS = ["apple", "apricot", "banana", "cherry"]

  const Search = (props: {
    multiple?: boolean
    onChange: (value: string | string[] | null) => void
  }) => (
    <Combobox
      items={FRUITS}
      multiple={props.multiple}
      autoHighlight
      onValueChange={props.onChange}
    >
      <ComboboxInput testId="input" placeholder="Fruit" />
      <ComboboxValue testId="value" placeholder="none" />
      <ComboboxContent testId="menu">
        <ComboboxList testId="list">
          {(item) => (
            <ComboboxItem
              value={item}
              testId={`item-${item}`}
              disabled={item === "apricot"}
              style={(state) => ({ opacity: state.highlighted ? 1 : 0.6 })}
            >
              {(state) => <text>{state.selected ? `[${item}]` : item}</text>}
            </ComboboxItem>
          )}
        </ComboboxList>
        <ComboboxEmpty testId="empty">
          <text>No match</text>
        </ComboboxEmpty>
      </ComboboxContent>
    </Combobox>
  )

  const type = async (root: Root, renderer: RecordingRenderer, value: string) => {
    root.dispatch({
      elementId: idOf(renderer, "input"),
      eventType: "change",
      value,
    })
    await settle(root)
  }

  test("filters as the user types and selects the highlighted item on Enter", async () => {
    const { root, renderer } = setup()
    const changes: unknown[] = []
    root.render(() => <Search onChange={(value) => changes.push(value)} />)
    await settle(root)
    expect(renderer.findByProp("testId", "menu")).toBeUndefined()

    await type(root, renderer, "ap")
    expect(renderer.textOf(idOf(renderer, "list"))).toBe("appleapricot")
    expect(renderer.findByProp("testId", "item-apple")?.style).toEqual({
      opacity: 1,
    })

    root.dispatch({ elementId: idOf(renderer, "input"), eventType: "submit" })
    await settle(root)
    expect(changes).toEqual(["apple"])
    expect(renderer.findByProp("testId", "menu")).toBeUndefined()
    expect(renderer.findByProp("testId", "input")?.props.value).toBe("apple")
    expect(renderer.textOf(idOf(renderer, "value"))).toBe("apple")
  })

  test("arrow keys skip disabled items, and an empty result shows the fallback", async () => {
    const { root, renderer } = setup()
    const changes: unknown[] = []
    root.render(() => <Search onChange={(value) => changes.push(value)} />)
    await settle(root)

    await type(root, renderer, "a")
    const input = idOf(renderer, "input")
    // apple is highlighted; apricot is disabled, so down lands on banana.
    root.dispatch(keyDown(input, "down"))
    await settle(root)
    expect(renderer.findByProp("testId", "item-banana")?.style).toEqual({
      opacity: 1,
    })
    root.dispatch(click(idOf(renderer, "item-apricot")))
    await settle(root)
    expect(changes).toEqual([])

    await type(root, renderer, "zzz")
    expect(renderer.textOf(idOf(renderer, "menu"))).toBe("No match")
  })

  test("multiple keeps the menu open and toggles each pick", async () => {
    const { root, renderer } = setup()
    const changes: unknown[] = []
    root.render(() => (
      <Search multiple onChange={(value) => changes.push(value)} />
    ))
    await settle(root)

    await type(root, renderer, "a")
    root.dispatch(click(idOf(renderer, "item-banana")))
    await settle(root)
    root.dispatch(click(idOf(renderer, "item-cherry")))
    await settle(root)
    expect(changes).toEqual([["banana"], ["banana", "cherry"]])
    expect(renderer.textOf(idOf(renderer, "value"))).toBe("banana, cherry")
    expect(renderer.textOf(idOf(renderer, "item-banana"))).toBe("[banana]")

    root.dispatch(click(idOf(renderer, "item-banana")))
    await settle(root)
    expect(changes.at(-1)).toEqual(["cherry"])
    expect(renderer.findByProp("testId", "menu")).toBeDefined()
  })
})

describe("AnimatePresence", () => {
  const completeMotion = (root: Root, renderer: RecordingRenderer, testId: string) => {
    const node = renderer.findByProp("testId", testId)
    const motionProp = node?.props.motion as { generation: number }
    root.dispatch({
      elementId: node?.id as number,
      eventType: "motionComplete",
      motionGeneration: motionProp.generation,
    })
  }

  test("keeps a motion.div until its exit animation completes", async () => {
    const { root, renderer } = setup()
    const write: { show?: (value: boolean) => void } = {}
    let exits = 0
    const App = () => {
      const visible = atom(true)
      write.show = (value) => visible.set(value)
      return (
        <div testId="host">
          <AnimatePresence onExitComplete={() => (exits += 1)}>
            {visible() && (
              <motion.div
                key="toast"
                testId="toast"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
              />
            )}
          </AnimatePresence>
        </div>
      )
    }
    root.render(App)
    await settle(root)
    const toast = () => renderer.findByProp("testId", "toast")
    const id = toast()?.id
    expect(toast()?.props.motion).toMatchObject({
      animate: { opacity: 1 },
      initial: { opacity: 0 },
      isExit: false,
    })

    write.show?.(false)
    await settle(root)
    // Still mounted, same native element, now animating to `exit`.
    expect(toast()?.id).toBe(id)
    expect(toast()?.props.motion).toMatchObject({
      animate: { opacity: 0 },
      isExit: true,
    })
    expect(exits).toBe(0)

    completeMotion(root, renderer, "toast")
    await settle(root)
    expect(toast()).toBeUndefined()
    expect(exits).toBe(1)
    expect(renderer.errors).toEqual([])
  })

  test("a child that returns before its exit ends stays, and animates back in", async () => {
    const { root, renderer } = setup()
    const write: { show?: (value: boolean) => void } = {}
    const App = () => {
      const visible = atom(true)
      write.show = (value) => visible.set(value)
      return (
        <AnimatePresence>
          {visible() && (
            <motion.div
              key="panel"
              testId="panel"
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
            />
          )}
        </AnimatePresence>
      )
    }
    root.render(App)
    await settle(root)
    const panel = () => renderer.findByProp("testId", "panel")
    const id = panel()?.id

    write.show?.(false)
    await settle(root)
    write.show?.(true)
    await settle(root)
    expect(panel()?.id).toBe(id)
    expect(panel()?.props.motion).toMatchObject({
      animate: { opacity: 1 },
      isExit: false,
    })

    // The exit it interrupted reports late and must not remove it.
    root.dispatch({
      elementId: id as number,
      eventType: "motionComplete",
      motionGeneration: 2,
    })
    await settle(root)
    expect(panel()?.id).toBe(id)
  })

  test("initial={false} skips the enter animation, and plain children leave at once", async () => {
    const { root, renderer } = setup()
    const write: { items?: (value: string[]) => void } = {}
    const App = () => {
      const items = atom(["a", "b"])
      write.items = (value) => items.set(value)
      return (
        <div testId="host">
          <AnimatePresence initial={false}>
            {items().map((item) =>
              item === "a" ? (
                <motion.div
                  key={item}
                  testId="animated"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                />
              ) : (
                <text key={item} testId="plain">
                  {item}
                </text>
              )
            )}
          </AnimatePresence>
        </div>
      )
    }
    root.render(App)
    await settle(root)
    expect(
      (renderer.findByProp("testId", "animated")?.props.motion as { initial: unknown })
        .initial
    ).toBe(false)

    write.items?.([])
    await settle(root)
    // Nothing animates the <text> out, so it is gone with the render.
    expect(renderer.findByProp("testId", "plain")).toBeUndefined()
    expect(renderer.findByProp("testId", "animated")).toBeDefined()

    completeMotion(root, renderer, "animated")
    await settle(root)
    expect(renderer.findByProp("testId", "host")?.children).toEqual([])
  })

  test("usePresence lets a component run its own exit", async () => {
    const { root, renderer } = setup()
    const write: { show?: (value: boolean) => void; done?: () => void } = {}
    const Fading = () => {
      const [present, safeToRemove] = usePresence()
      write.done = safeToRemove
      return <text testId="fading">{present ? "in" : "out"}</text>
    }
    const App = () => {
      const visible = atom(true)
      write.show = (value) => visible.set(value)
      return <AnimatePresence>{visible() && <Fading key="f" />}</AnimatePresence>
    }
    root.render(App)
    await settle(root)
    expect(renderer.textOf(idOf(renderer, "fading"))).toBe("in")

    write.show?.(false)
    await settle(root)
    expect(renderer.textOf(idOf(renderer, "fading"))).toBe("out")

    write.done?.()
    await settle(root)
    expect(renderer.findByProp("testId", "fading")).toBeUndefined()
  })
})

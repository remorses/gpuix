/// GPUI accessibility tree from React `role` and `aria-*` props.
///
/// A node is in the tree only with both an id (always set) and a role.
/// These tests dump GPUI's debug tree after paint, not screenshots.

import fs from "fs"
import path from "path"
import React, { useState } from "react"
import { beforeEach, describe, expect, it } from "vitest"
import {
  createTestRoot,
  hasNativeTestRenderer,
  type A11yTreeDump,
  type TestRoot,
} from "../testing.js"
import { SHOTS_DIR } from "./test-utils.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

type A11yNode = NonNullable<NonNullable<A11yTreeDump["nodes"]>[string]["aria"]>

function ariaOf(tree: A11yTreeDump): A11yNode[] {
  return Object.values(tree.nodes ?? {})
    .map((node) => node.aria)
    .filter((aria): aria is A11yNode => aria != null)
}

function withRole(tree: A11yTreeDump, role: string): A11yNode[] {
  return ariaOf(tree).filter((aria) => aria.role === role)
}

describeNative("accessibility", () => {
  let testRoot: TestRoot

  beforeEach(() => {
    testRoot = createTestRoot()
  })

  it("exposes role, aria-label, and Click for an onClick button", () => {
    testRoot.render(
      <div
        role="button"
        aria-label="Delete note"
        aria-id="notes.delete"
        onClick={() => {}}
        style={{ width: 120, height: 40 }}
      >
        Delete
      </div>,
    )

    const tree = testRoot.renderer.getA11yTree()
    const buttons = withRole(tree, "Button")
    expect(buttons).toEqual([
      expect.objectContaining({
        role: "Button",
        label: "Delete note",
        author_id: "notes.delete",
      }),
    ])
    expect(buttons[0]?.on_action).toEqual(expect.arrayContaining(["Click"]))
  })

  it("reports a <text> node as Label with its content", () => {
    testRoot.render(<text style={{ width: 200, height: 24 }}>Hello</text>)

    const labels = withRole(testRoot.renderer.getA11yTree(), "Label")
    expect(labels.some((aria) => aria.value === "Hello")).toBe(true)
  })

  it("omits a div with no role from the tree", () => {
    testRoot.render(
      <div style={{ width: 120, height: 40 }} aria-label="silent">
        Hello
      </div>,
    )

    const tree = testRoot.renderer.getA11yTree()
    expect(withRole(tree, "GenericContainer")).toEqual([])
    expect(ariaOf(tree).some((aria) => aria.label === "silent")).toBe(false)
  })

  it("drops role none and presentation", () => {
    testRoot.render(
      <div style={{ width: 200, height: 80 }}>
        <div role="none" aria-label="hidden none" style={{ width: 80, height: 24 }} />
        <div
          role="presentation"
          aria-label="hidden presentation"
          style={{ width: 80, height: 24 }}
        />
      </div>,
    )

    const labels = ariaOf(testRoot.renderer.getA11yTree()).map(
      (aria) => aria.label,
    )
    expect(labels).not.toContain("hidden none")
    expect(labels).not.toContain("hidden presentation")
  })

  it("passes aria-expanded, aria-selected, and aria-level", () => {
    testRoot.render(
      <div style={{ width: 240, height: 80 }}>
        <div
          role="button"
          aria-label="Folder"
          aria-expanded={true}
          style={{ width: 120, height: 24 }}
        />
        <div
          role="option"
          aria-label="One"
          aria-selected={true}
          style={{ width: 120, height: 24 }}
        />
        <div
          role="heading"
          aria-label="Title"
          aria-level={2}
          style={{ width: 120, height: 24 }}
        />
      </div>,
    )

    const tree = testRoot.renderer.getA11yTree()
    expect(withRole(tree, "Button")[0]).toEqual(
      expect.objectContaining({ label: "Folder", expanded: true }),
    )
    expect(withRole(tree, "ListBoxOption")[0]).toEqual(
      expect.objectContaining({ label: "One", selected: true }),
    )
    expect(withRole(tree, "Heading")[0]).toEqual(
      expect.objectContaining({ label: "Title", level: 2 }),
    )
  })

  it("passes aria-keyshortcuts, aria-placeholder, aria-checked, aria-pressed, and aria-orientation", () => {
    testRoot.render(
      <div style={{ width: 320, height: 200 }}>
        <div
          role="button"
          aria-label="Pressed"
          aria-keyshortcuts="Meta+K"
          aria-pressed={true}
          style={{ width: 120, height: 24 }}
        />
        <div
          role="button"
          aria-label="Unpressed"
          aria-pressed={false}
          style={{ width: 120, height: 24 }}
        />
        <div
          role="button"
          aria-label="Partial"
          aria-pressed="mixed"
          style={{ width: 120, height: 24 }}
        />
        <div
          role="checkbox"
          aria-label="Select all"
          aria-checked="mixed"
          style={{ width: 120, height: 24 }}
        />
        <div
          role="textbox"
          aria-label="Search"
          aria-placeholder="Search notes"
          style={{ width: 120, height: 24 }}
        />
        <div
          role="tablist"
          aria-label="Panels"
          aria-orientation="vertical"
          style={{ width: 120, height: 24 }}
        />
      </div>,
    )

    const tree = testRoot.renderer.getA11yTree()
    const buttons = withRole(tree, "Button")
    const byLabel = (label: string) =>
      buttons.find((aria) => aria.label === label)
    expect(byLabel("Pressed")).toEqual(
      expect.objectContaining({ keyboard_shortcut: "Meta+K", toggled: "True" }),
    )
    expect(byLabel("Unpressed")?.toggled).toBe("False")
    expect(byLabel("Partial")?.toggled).toBe("Mixed")
    expect(withRole(tree, "CheckBox")[0]?.toggled).toBe("Mixed")
    expect(withRole(tree, "TextInput")[0]?.placeholder).toBe("Search notes")
    expect(withRole(tree, "TabList")[0]?.orientation).toBe("Vertical")
  })

  it("passes aria-valuenow, aria-valuemin, and aria-valuemax as numeric values", () => {
    testRoot.render(
      <div style={{ width: 280, height: 120 }}>
        <div
          role="slider"
          aria-label="Volume"
          aria-valuenow={40}
          aria-valuemin={0}
          aria-valuemax={100}
          style={{ width: 120, height: 24 }}
        />
        <div
          role="spinbutton"
          aria-label="Quantity"
          aria-valuenow={2.5}
          aria-valuemin={-10}
          style={{ width: 120, height: 24 }}
        />
      </div>,
    )

    const tree = testRoot.renderer.getA11yTree()
    expect(withRole(tree, "Slider")[0]).toEqual(
      expect.objectContaining({
        label: "Volume",
        numeric_value: 40,
        min_numeric_value: 0,
        max_numeric_value: 100,
      }),
    )
    const spinbutton = withRole(tree, "SpinButton")[0]
    expect(spinbutton?.numeric_value).toBe(2.5)
    expect(spinbutton?.min_numeric_value).toBe(-10)
    expect(spinbutton?.max_numeric_value).toBeUndefined()
  })

  it("passes aria-posinset, aria-setsize, and the table indexes", () => {
    testRoot.render(
      <div style={{ width: 280, height: 160 }}>
        <div
          role="listitem"
          aria-label="Row"
          aria-posinset={2}
          aria-setsize={9}
          style={{ width: 120, height: 24 }}
        />
        <div
          role="gridcell"
          aria-label="Cell"
          aria-rowindex={3}
          aria-colindex={4}
          style={{ width: 120, height: 24 }}
        />
        <div
          role="grid"
          aria-label="Sheet"
          aria-rowcount={10}
          aria-colcount={6}
          style={{ width: 120, height: 24 }}
        />
      </div>,
    )

    const tree = testRoot.renderer.getA11yTree()
    expect(withRole(tree, "ListItem")[0]).toEqual(
      expect.objectContaining({ position_in_set: 2, size_of_set: 9 }),
    )
    expect(withRole(tree, "GridCell")[0]).toEqual(
      expect.objectContaining({ row_index: 3, column_index: 4 }),
    )
    expect(withRole(tree, "Grid")[0]).toEqual(
      expect.objectContaining({ row_count: 10, column_count: 6 }),
    )
  })

  it("drops invalid aria values instead of panicking", () => {
    testRoot.render(
      <div style={{ width: 280, height: 120 }}>
        <div
          role="slider"
          aria-label="Bad value"
          aria-valuenow="not-a-number"
          aria-valuemin={Number.POSITIVE_INFINITY}
          aria-posinset={0}
          aria-setsize={-3}
          aria-rowindex={2.5}
          style={{ width: 120, height: 24 }}
        />
        <div
          role="button"
          aria-label="Bad tokens"
          aria-orientation="diagonal"
          aria-pressed="maybe"
          style={{ width: 120, height: 24 }}
        />
      </div>,
    )

    const tree = testRoot.renderer.getA11yTree()
    const slider = withRole(tree, "Slider")[0]
    expect(slider?.label).toBe("Bad value")
    expect(slider?.numeric_value).toBeUndefined()
    expect(slider?.min_numeric_value).toBeUndefined()
    expect(slider?.position_in_set).toBeUndefined()
    expect(slider?.size_of_set).toBeUndefined()
    expect(slider?.row_index).toBeUndefined()

    const button = withRole(tree, "Button")[0]
    expect(button?.orientation).toBeUndefined()
    expect(button?.toggled).toBeUndefined()
  })

  it("reports <input> as TextInput and <textarea> as MultilineTextInput", () => {
    testRoot.render(
      <div style={{ width: 280, height: 120 }}>
        <input value="name" placeholder="Your name" style={{ width: 200, height: 32 }} />
        <textarea value="body" style={{ width: 200, height: 48 }} />
      </div>,
    )

    const tree = testRoot.renderer.getA11yTree()
    expect(withRole(tree, "TextInput").some((aria) => aria.value === "name")).toBe(
      true,
    )
    expect(
      withRole(tree, "MultilineTextInput").some((aria) => aria.value === "body"),
    ).toBe(true)
  })

  it("uses img alt as the accessible name", () => {
    const src = path.join(SHOTS_DIR, "gpuix-a11y-img.svg")
    fs.writeFileSync(
      src,
      '<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#000"/></svg>',
    )
    testRoot.render(
      <img alt="Cat photo" src={src} style={{ width: 40, height: 40 }} />,
    )

    const images = withRole(testRoot.renderer.getA11yTree(), "Image")
    expect(images.some((aria) => aria.label === "Cat photo")).toBe(true)
  })

  it("keeps img alt when src is empty", () => {
    testRoot.render(<img alt="Empty source" src="" style={{ width: 40, height: 40 }} />)

    const images = withRole(testRoot.renderer.getA11yTree(), "Image")
    expect(images.some((aria) => aria.label === "Empty source")).toBe(true)
  })

  it("exposes role and aria-label on <anchored>", () => {
    testRoot.render(
      <anchored
        role="menu"
        aria-label="File menu"
        position={{ x: 8, y: 8 }}
        style={{ width: 80, height: 40 }}
      />,
    )

    expect(withRole(testRoot.renderer.getA11yTree(), "Menu")[0]).toEqual(
      expect.objectContaining({ role: "Menu", label: "File menu" }),
    )
  })

  it("exposes role and aria-label on <virtual-list>", () => {
    testRoot.render(
      <virtual-list
        role="list"
        aria-label="Messages"
        style={{ width: 200, height: 80 }}
      >
        <text>one</text>
      </virtual-list>,
    )

    expect(withRole(testRoot.renderer.getA11yTree(), "List")[0]).toEqual(
      expect.objectContaining({ role: "List", label: "Messages" }),
    )
  })

  it("joins interpolated <text> children into one Label", () => {
    const name = "Ada"
    testRoot.render(<text style={{ width: 200, height: 24 }}>Hello {name}!</text>)

    const labels = withRole(testRoot.renderer.getA11yTree(), "Label")
    expect(labels.filter((aria) => aria.value === "Hello Ada!")).toHaveLength(1)
  })

  it("lets an explicit role override the <text> default", () => {
    testRoot.render(
      <text role="heading" aria-level={1} style={{ width: 200, height: 24 }}>
        Title
      </text>,
    )

    const tree = testRoot.renderer.getA11yTree()
    expect(withRole(tree, "Heading")[0]).toEqual(
      expect.objectContaining({ role: "Heading", value: "Title", level: 1 }),
    )
  })

  it("updates aria-label when React state changes", () => {
    function Toggle() {
      const [on, setOn] = useState(false)
      return (
        <div
          role="button"
          aria-label={on ? "On" : "Off"}
          onClick={() => setOn(true)}
          style={{ width: 80, height: 32 }}
        />
      )
    }

    testRoot.render(<Toggle />)
    expect(withRole(testRoot.renderer.getA11yTree(), "Button")[0]?.label).toBe("Off")

    testRoot.renderer.nativeSimulateClick(10, 10)
    expect(withRole(testRoot.renderer.getA11yTree(), "Button")[0]?.label).toBe("On")
  })
})

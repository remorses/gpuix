import type { EventPayload } from "@gpuix/native"
import type {
  MutationRenderer,
  NativeRenderer,
  StyleDesc,
} from "@gpuix/native/host"

export interface ModelNode {
  id: number
  type: string
  text: string
  style: StyleDesc
  props: Record<string, unknown>
  events: Set<string>
  children: number[]
}

/**
 * A `NativeRenderer` that only records the mutation protocol and rebuilds the
 * native tree from it. It lets the adapter be driven end to end without the
 * GPU addon: every assertion reads the tree GPUIX would have built.
 */
export class RecordingRenderer {
  readonly nodes = new Map<number, ModelNode>()
  readonly destroyed: number[] = []
  rootId: number | null = null
  batches = 0
  windowKeyEvents = false
  windowSelection = false
  selection: string | null = null

  /** Every mutation received, in order, for asserting on what a repaint sends. */
  readonly log: Array<[string, ...unknown[]]> = []

  applyBatch(json: string): number[] {
    this.batches += 1
    const before = this.destroyed.length
    for (const tuple of JSON.parse(json) as Array<[string, ...unknown[]]>) {
      const [op, ...args] = tuple
      this.log.push(tuple)
      this.apply(op, args)
    }
    return this.destroyed.slice(before)
  }

  getWindowSize(): { width: number; height: number } {
    return this.windowSize
  }

  windowSize = { height: 600, width: 800 }
  scrolledIntoView: number[] = []

  scrollIntoView(id: number): void {
    this.scrolledIntoView.push(id)
  }

  /** Every `focusElement` call, in order; the last one is the focused id. */
  focused: number[] = []
  focusedWithin: Array<[direction: "next" | "previous", id: number]> = []

  focusElement(id: number): void {
    this.focused.push(id)
  }

  getFocusedElementId(): number | null {
    return this.focused.at(-1) ?? null
  }

  focusNextWithin(id: number): void {
    this.focusedWithin.push(["next", id])
  }

  focusPreviousWithin(id: number): void {
    this.focusedWithin.push(["previous", id])
  }

  focusNextCalls = 0

  focusNext(): void {
    this.focusNextCalls += 1
  }

  private detach(id: number): void {
    for (const node of this.nodes.values()) {
      node.children = node.children.filter((child) => child !== id)
    }
  }

  /** Native destroy takes the whole subtree, like `RetainedTree`. */
  private destroy(id: number): void {
    const node = this.nodes.get(id)
    if (!node) {
      return
    }
    this.nodes.delete(id)
    this.destroyed.push(id)
    for (const child of node.children) {
      this.destroy(child)
    }
  }

  setWindowKeyEvents(enabled: boolean): void {
    this.windowKeyEvents = enabled
  }

  setWindowSelectionChange(enabled: boolean): void {
    this.windowSelection = enabled
  }

  getSelectedText(): string | null {
    return this.selection
  }

  /** Mutations that named an element the native tree does not hold. */
  readonly errors: string[] = []

  private apply(op: string, args: unknown[]): void {
    const id = args[0] as number
    const ids =
      op === "appendChild" || op === "insertBefore"
        ? (args as number[])
        : op === "createElement" || op === "setKeyboardFocusDim"
          ? []
          : [id]
    for (const target of ids) {
      if (!this.nodes.has(target)) {
        this.errors.push(`${op}(${args.join(", ")}): no element ${target}`)
      }
    }
    switch (op) {
      case "createElement": {
        this.nodes.set(id, {
          children: [],
          events: new Set(),
          id,
          props: {},
          style: {},
          text: "",
          type: args[1] as string,
        })
        return
      }
      case "destroyElement": {
        this.destroy(id)
        for (const node of this.nodes.values()) {
          node.children = node.children.filter((child) => child !== id)
        }
        if (this.rootId === id) {
          this.rootId = null
        }
        return
      }
      case "appendChild": {
        const parentId = args[0] as number
        const childId = args[1] as number
        const parent = this.nodes.get(parentId)
        if (!parent) {
          return
        }
        this.detach(childId)
        parent.children.push(childId)
        return
      }
      case "insertBefore": {
        const parentId = args[0] as number
        const childId = args[1] as number
        const beforeId = args[2] as number
        const parent = this.nodes.get(parentId)
        if (!parent) {
          return
        }
        this.detach(childId)
        const at = parent.children.indexOf(beforeId)
        parent.children.splice(at < 0 ? parent.children.length : at, 0, childId)
        return
      }
      case "setText": {
        const node = this.nodes.get(id)
        if (node) {
          node.text = args[1] as string
        }
        return
      }
      case "setStyle": {
        const node = this.nodes.get(id)
        if (node) {
          node.style = (args[1] ?? {}) as StyleDesc
        }
        return
      }
      case "setCustomProp": {
        const node = this.nodes.get(id)
        if (node) {
          node.props[args[1] as string] = args[2]
        }
        return
      }
      case "setEventListener": {
        const node = this.nodes.get(id)
        if (!node) {
          return
        }
        if (args[2]) {
          node.events.add(args[1] as string)
        } else {
          node.events.delete(args[1] as string)
        }
        return
      }
      case "setRoot": {
        this.rootId = id
        return
      }
      default:
        return
    }
  }

  /** Text of a node and its descendants, in paint order. */
  textOf(id: number): string {
    const node = this.nodes.get(id)
    if (!node) {
      return ""
    }
    return (
      node.text + node.children.map((child) => this.textOf(child)).join("")
    )
  }

  /** Depth-first ids of a node's descendants, excluding the node itself. */
  descendantsOf(id: number): number[] {
    const node = this.nodes.get(id)
    if (!node) {
      return []
    }
    return node.children.flatMap((child) => [child, ...this.descendantsOf(child)])
  }

  /** Every node id in the tree, root first. */
  allIds(): number[] {
    return this.rootId === null
      ? []
      : [this.rootId, ...this.descendantsOf(this.rootId)]
  }

  find(predicate: (node: ModelNode) => boolean): ModelNode | undefined {
    return this.allIds()
      .map((id) => this.nodes.get(id))
      .find((node): node is ModelNode => !!node && predicate(node))
  }

  findByProp(name: string, value: unknown): ModelNode | undefined {
    return this.find((node) => node.props[name] === value)
  }
}

export const asRenderer = (renderer: RecordingRenderer): NativeRenderer =>
  renderer as unknown as NativeRenderer

export const click = (elementId: number): EventPayload => ({
  elementId,
  eventType: "click",
})

export const keyDown = (elementId: number, key: string): EventPayload => ({
  elementId,
  eventType: "keyDown",
  key,
})

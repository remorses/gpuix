import { expect, test } from "vitest"

import { atom } from "ilha"
import type { View } from "ilha"

import { createRoot } from "../root.js"
import { RecordingRenderer, asRenderer } from "./recording-renderer.js"

/** One node of a random tree: a leaf, a box, a component, or nothing. */
type Spec =
  | { kind: "leaf"; id: string }
  | { kind: "gone" }
  | { kind: "box"; id: string; keyed: boolean; children: Spec[] }
  | { kind: "part"; id: string; keyed: boolean; children: Spec[] }

const sleep = (ms: number): Promise<void> => {
  const { promise, resolve } = Promise.withResolvers<void>()
  setTimeout(resolve, ms)
  return promise
}

/** mulberry32: the run is reproducible from the seed. */
const random = (seed: number) => () => {
  seed = (seed + 0x6d2b79f5) | 0
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed)
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296
}

function randomSpec(next: () => number, depth: number, ids: string[]): Spec {
  const roll = next()
  const id = ids[Math.floor(next() * ids.length)] as string
  if (roll < 0.15) {
    return { kind: "gone" }
  }
  if (depth === 0 || roll < 0.5) {
    return { id, kind: "leaf" }
  }
  const children = Array.from({ length: Math.floor(next() * 4) }, () =>
    randomSpec(next, depth - 1, ids)
  )
  return {
    children,
    id,
    keyed: next() < 0.5,
    kind: roll < 0.75 ? "box" : "part",
  }
}

const textOf = (spec: Spec): string => {
  if (spec.kind === "gone") {
    return ""
  }
  if (spec.kind === "leaf") {
    return spec.id
  }
  return `[${spec.id}${spec.children.map(textOf).join("")}]`
}

/**
 * ilha scopes component keys to the fiber, not to the sibling list, so a key
 * carries the node's path to stay unique across the whole view.
 */
const view = (spec: Spec, path: string): View => {
  if (spec.kind === "gone") {
    return false
  }
  if (spec.kind === "leaf") {
    return <text>{spec.id}</text>
  }
  const key = spec.keyed ? `${spec.id}@${path}` : undefined
  const body = [
    <text>{`[${spec.id}`}</text>,
    ...spec.children.map((child, index) => view(child, `${path}.${index}`)),
    <text>]</text>,
  ]
  if (spec.kind === "box") {
    return <div key={key}>{body}</div>
  }
  return <Part key={key} body={body} />
}

const Part = (props: { body: View[] }) => props.body

const SEEDS = [0xc0ffee, 7, 1234, 99991]

test.each(SEEDS)("the native tree matches the view after any repaint (seed %i)", async (seed) => {
  const next = random(seed)
  const ids = ["a", "b", "c", "d", "e"]
  const renderer = new RecordingRenderer()
  const root = createRoot(asRenderer(renderer))
  const write: { set?: (spec: Spec[]) => void } = {}
  const first: Spec[] = [randomSpec(next, 3, ids)]
  const App = () => {
    const specs = atom(first)
    write.set = (value) => specs.set(value)
    return (
      <div testId="top">
        {specs().map((spec, index) => view(spec, String(index)))}
      </div>
    )
  }
  root.render(App)
  await sleep(1)
  root.flush()
  const top = () => renderer.findByProp("testId", "top")
  expect(renderer.textOf(top()?.id as number)).toBe(first.map(textOf).join(""))

  for (let step = 0; step < 150; step += 1) {
    const specs = Array.from({ length: 1 + Math.floor(next() * 4) }, () =>
      randomSpec(next, 3, ids)
    )
    write.set?.(specs)
    await sleep(1)
    root.flush()
    expect(renderer.textOf(top()?.id as number), `step ${step}`).toBe(
      specs.map(textOf).join("")
    )
  }

  expect(renderer.errors).toEqual([])
  root.unmount()
  expect(renderer.nodes.size).toBe(0)
})

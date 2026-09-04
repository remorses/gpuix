/// Inheritance.
///
/// A text property declared on an ancestor reaches every `<text>` below it,
/// the way CSS inherits it. GPUI does this itself: a `div` pushes its text
/// style onto a window stack, and a `<text>` with no style of its own paints
/// with the whole stack composed.
///
/// Two more things inherit and are not text. `userSelect: "none"` turns off
/// selection for a subtree, and `selectionColor` sets the wash for one.

import React from "react"
import type { StyleDesc } from "@gpuix/react"
import { Grid, Panel, Sample } from "./ui.js"

const SENTENCE = "The quick brown fox jumps over the lazy dog"

const CASES: Array<[string, StyleDesc]> = [
  ["color", { color: "#ff5c8a" }],
  ["fontSize", { fontSize: 22 }],
  ["fontWeight", { fontWeight: "bold" }],
  ["fontFamily", { fontFamily: "Courier New" }],
  ["lineHeight", { lineHeight: "2.2" }],
  ["textAlign", { textAlign: "right" }],
]

const BOX = {
  width: 200,
  height: 96,
  padding: 8,
  borderRadius: 8,
  backgroundColor: "#ffffff",
  color: "#101018",
} as const

export function Inheritance() {
  return (
    <div className="col gap-4">
      <Panel
        title="Declared on an ancestor"
        note="Each pair paints the same text twice. On the left the property sits on the box, on the right it sits on the text. The two are identical."
      >
        {CASES.map(([name, declaration]) => (
          <div key={name} className="col gap-2">
            <text className="mono text-xs text-muted">{JSON.stringify(declaration)}</text>
            <Grid>
              <Sample label="on the ancestor">
                <div style={{ ...BOX, ...declaration }}>
                  <text>{SENTENCE}</text>
                </div>
              </Sample>
              <Sample label="on the text">
                <div style={BOX}>
                  <text style={declaration}>{SENTENCE}</text>
                </div>
              </Sample>
            </Grid>
          </div>
        ))}
      </Panel>

      <Panel title="Nearest ancestor wins" note="Two boxes disagree, and the inner one decides.">
        <Grid>
          <Sample label="outer red, inner blue">
            <div style={{ ...BOX, color: "#e11d48", fontSize: 18 }}>
              <div style={{ color: "#2563eb" }}>
                <text>{SENTENCE}</text>
              </div>
            </div>
          </Sample>
          <Sample label="the size still comes from the outer box" hint="Only `color` was re-declared.">
            <div style={{ ...BOX, color: "#e11d48", fontSize: 18 }}>
              <div style={{ color: "#2563eb" }}>
                <text>18px, inherited past the inner box</text>
              </div>
            </div>
          </Sample>
        </Grid>
      </Panel>

      <Panel
        title="currentColor follows the inherited colour"
        note="`currentColor` reads the computed `color`, which means the inherited one when the element declares none."
      >
        <Grid>
          <Sample label='color on the parent, borderColor: "currentColor"'>
            <div style={{ color: "#f9c74f" }}>
              <div style={{ height: 56, borderRadius: 8, borderWidth: 6, borderColor: "currentColor" }} className="w-full" />
            </div>
          </Sample>
          <Sample label="a nearer colour wins for the border too">
            <div style={{ color: "#f9c74f" }}>
              <div style={{ color: "#22c55e" }}>
                <div style={{ height: 56, borderRadius: 8, borderWidth: 6, borderColor: "currentColor" }} className="w-full" />
              </div>
            </div>
          </Sample>
        </Grid>
      </Panel>

      <Panel
        title="Selection"
        note="Drag across the paragraphs. The first selects, the second does not, and the third selects with its own wash. Press Cmd+C to copy what you selected."
      >
        <Grid>
          <Sample label="nothing declared" hint="Selectable, with the theme accent.">
            <div style={{ ...BOX, height: 120 }}>
              <text>{SENTENCE}</text>
            </div>
          </Sample>
          <Sample label='userSelect: "none"' hint="Inherited, so the whole box opts out.">
            <div style={{ ...BOX, height: 120, userSelect: "none" }}>
              <text>{SENTENCE}</text>
            </div>
          </Sample>
          <Sample label='selectionColor: "#22c55e66"' hint="Inherited by the subtree.">
            <div style={{ ...BOX, height: 120, selectionColor: "#22c55e66" }}>
              <text>{SENTENCE}</text>
            </div>
          </Sample>
        </Grid>
      </Panel>
    </div>
  )
}

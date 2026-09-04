/// Custom properties and `var()`.
///
/// A custom property holds text, not a typed value. Substitution is textual,
/// and the property parser then reads the result as if the author had written
/// it in place. So a variable can hold a colour, a length, a whole `calc()` or
/// the name of another variable.
///
/// A `var()` with no declaration and no fallback is invalid at computed-value
/// time. CSS says the property takes its inherited or initial value, which here
/// means the declaration drops and the element keeps what it had.

import React, { useState } from "react"
import { Button, Grid, Panel, Row, Sample, Swatch } from "./ui.js"

const BRANDS = ["#7c6cff", "#22c55e", "#f97316", "#e11d48"]

function Live() {
  const [brand, setBrand] = useState(BRANDS[0])
  return (
    <Panel
      title="One declaration, a whole subtree"
      note="The picker changes `--brand` on the box below. Every element under it reads the new value on the next frame, and nothing outside it re-resolves."
    >
      <Row>
        {BRANDS.map((value) => (
          <Button key={value} label={value} active={value === brand} onClick={() => setBrand(value)} />
        ))}
      </Row>
      <div
        style={{
          "--brand": brand,
          "--brand-soft": "color-mix(in oklch, var(--brand) 30%, #16161f)",
          padding: 16,
          borderRadius: 12,
          borderWidth: 1,
          borderColor: "var(--brand-soft)",
          backgroundColor: "var(--brand-soft)",
        }}
        className="col gap-3"
      >
        <Grid>
          <Sample label='backgroundColor: "var(--brand)"'>
            <Swatch color="var(--brand)" />
          </Sample>
          <Sample label='"var(--brand-soft)"' hint="a variable that names another one">
            <Swatch color="var(--brand-soft)" />
          </Sample>
          <Sample label='"oklch(from var(--brand) calc(l + 0.2) c h)"' hint="lighter, same hue">
            <Swatch color="oklch(from var(--brand) calc(l + 0.2) c h)" />
          </Sample>
          <Sample label='borderColor: "var(--brand)"' hint="the same value on a border">
            <div style={{ height: 56, borderRadius: 8, borderWidth: 6, borderColor: "var(--brand)" }} className="w-full" />
          </Sample>
        </Grid>
        <text className="text-sm" style={{ color: "var(--brand)" }}>
          Text coloured through the same variable.
        </text>
      </div>
    </Panel>
  )
}

export function Variables() {
  return (
    <div className="col gap-4">
      <Live />

      <Panel title="Where the value comes from" note="Nearest declaration wins, the same as in CSS.">
        <Grid>
          <Sample label="declared on the element itself">
            <div style={{ "--c": "#ff5c8a" }}>
              <Swatch color="var(--c)" />
            </div>
          </Sample>
          <Sample label="declared on the parent" hint="Inherited down the tree.">
            <div style={{ "--c": "#5cc8ff" }}>
              <div>
                <Swatch color="var(--c)" />
              </div>
            </div>
          </Sample>
          <Sample label="two ancestors disagree" hint="The nearer one, so green.">
            <div style={{ "--c": "#ff5c8a" }}>
              <div style={{ "--c": "#22c55e" }}>
                <Swatch color="var(--c)" />
              </div>
            </div>
          </Sample>
          <Sample label='var(--missing, "#f9c74f")' hint="The fallback.">
            <Swatch color="var(--missing, #f9c74f)" />
          </Sample>
          <Sample label="var(--missing)" hint="No fallback, so the declaration drops.">
            <div style={{ height: 56, borderRadius: 8, borderWidth: 1, borderColor: "var(--color-line)", backgroundColor: "var(--missing)" }} className="w-full" />
          </Sample>
          <Sample label="--a: var(--b), --b: var(--a)" hint="A cycle, so both are invalid.">
            <div style={{ "--a": "var(--b)", "--b": "var(--a)" }}>
              <div style={{ height: 56, borderRadius: 8, borderWidth: 1, borderColor: "var(--color-line)", backgroundColor: "var(--a)" }} className="w-full" />
            </div>
          </Sample>
        </Grid>
      </Panel>

      <Panel
        title="Lengths through a variable"
        note="A number declares its own text, so `{ '--pad': 8 }` declares `8`, and a property that wants pixels reads it as 8 pixels."
      >
        <Grid>
          <Sample label='"--pad": "20px", padding: "var(--pad)"'>
            <div style={{ "--pad": "20px", padding: "var(--pad)", backgroundColor: "var(--color-brand)", borderRadius: 8 }}>
              <div style={{ height: 40, backgroundColor: "var(--color-panel)", borderRadius: 4 }} />
            </div>
          </Sample>
          <Sample label='"--pad": 20, padding: "var(--pad)"' hint="The same box.">
            <div style={{ "--pad": 20, padding: "var(--pad)", backgroundColor: "var(--color-brand)", borderRadius: 8 }}>
              <div style={{ height: 40, backgroundColor: "var(--color-panel)", borderRadius: 4 }} />
            </div>
          </Sample>
          <Sample label='padding: "calc(var(--spacing) * 5)"' hint="Folded to 20px.">
            <div style={{ padding: "calc(var(--spacing) * 5)", backgroundColor: "var(--color-brand)", borderRadius: 8 }}>
              <div style={{ height: 40, backgroundColor: "var(--color-panel)", borderRadius: 4 }} />
            </div>
          </Sample>
        </Grid>
      </Panel>

      <Panel
        title="A variable inside a state"
        note="A state resolves against the element's own scope, so a declaration on the element is in scope for the `var()` in its hover style. Point at either box."
      >
        <Grid>
          <Sample label='hover: { backgroundColor: "var(--brand)" }'>
            <div
              style={{
                "--brand": "#7c6cff",
                height: 56,
                borderRadius: 8,
                backgroundColor: "var(--color-raised)",
                hover: { backgroundColor: "var(--brand)" },
              }}
              className="w-full"
            />
          </Sample>
          <Sample label="active: the same, while the button is down">
            <div
              style={{
                "--brand": "#22c55e",
                height: 56,
                borderRadius: 8,
                backgroundColor: "var(--color-raised)",
                cursor: "pointer",
                active: { backgroundColor: "var(--brand)" },
              }}
              className="w-full"
            />
          </Sample>
        </Grid>
      </Panel>
    </div>
  )
}

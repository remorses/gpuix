/// Colour values.
///
/// lightningcss reads the whole of CSS Color 4 and CSS Color 5. Every swatch
/// below paints the string under it, with no conversion in JavaScript. Three
/// values need something only the engine knows, so they get their own
/// treatment: `currentColor` reads the computed `color`, `light-dark()` reads
/// the appearance of the window, and a system colour reads the platform.

import React from "react"
import { Grid, Panel, Sample, Swatch } from "./ui.js"

const HEX: Array<[string, string]> = [
  ["#f00", "three digits"],
  ["#f00f", "four digits, with alpha"],
  ["#ff8800", "six digits"],
  ["#ff880080", "eight digits, half alpha"],
  ["rebeccapurple", "a named colour"],
  ["transparent", "the keyword"],
]

const FUNCTIONS: Array<[string, string]> = [
  ["rgb(255 0 0)", "space separated"],
  ["rgba(255, 0, 0, 0.5)", "the legacy comma form"],
  ["rgb(0 0 0 / 50%)", "alpha after a slash"],
  ["hsl(200 100% 50%)", ""],
  ["hsla(200, 100%, 50%, 0.5)", ""],
  ["hwb(90 10% 10%)", ""],
  ["lab(60% 40 30)", ""],
  ["lch(60% 50 30)", ""],
  ["oklab(0.7 0.1 0.1)", ""],
  ["oklch(0.7 0.15 200)", ""],
]

const MIXES: Array<[string, string]> = [
  ["color-mix(in srgb, #ff0000 60%, #0000ff)", "mixed in sRGB"],
  ["color-mix(in oklch, #ff0000 60%, #0000ff)", "the same two in OKLCH"],
  ["color-mix(in oklch, var(--color-brand) 40%, white)", "a mix over a variable"],
  ["light-dark(#eeeeee, #1e1e2e)", "the window is dark, so the second one"],
]

const RELATIVE: Array<[string, string]> = [
  ["rgb(from #bad455 b r g)", "channels swapped"],
  ["oklch(from #bad455 calc(l - 0.15) c h)", "the same hue, darker"],
  ["oklch(from var(--color-brand) l calc(c * 0.4) h)", "the brand, washed out"],
  ["hsl(from #bad455 calc(h + 180) s l)", "the opposite hue"],
]

function List({ title, note, entries }: {
  title: string
  note: string
  entries: Array<[string, string]>
}) {
  return (
    <Panel title={title} note={note}>
      <Grid>
        {entries.map(([value, hint]) => (
          <Sample key={value} label={value} hint={hint || undefined}>
            <Swatch color={value} />
          </Sample>
        ))}
      </Grid>
    </Panel>
  )
}

export function Colors() {
  return (
    <div className="col gap-4">
      <List
        title="Hex and keywords"
        note="Three, four, six and eight digits, plus the named colours."
        entries={HEX}
      />
      <List
        title="Colour functions"
        note="CSS Color 4. Every one of these reaches lightningcss as written."
        entries={FUNCTIONS}
      />
      <List
        title="color-mix and light-dark"
        note="CSS Color 5. The root window is dark today, so light-dark() always takes its second colour."
        entries={MIXES}
      />
      <List
        title="Relative colour syntax"
        note="A colour built from another colour. The channel names are in scope for the arithmetic."
        entries={RELATIVE}
      />

      <Panel
        title="currentColor"
        note="The computed `color` of the element, or of the nearest ancestor that declares one."
      >
        <Grid>
          <Sample label='color: "#ff5c8a", borderColor: "currentColor"'>
            <div
              style={{ height: 56, borderRadius: 8, color: "#ff5c8a", borderWidth: 6, borderColor: "currentColor" }}
              className="w-full"
            />
          </Sample>
          <Sample label="the same border, colour on the parent" hint="Inherited, so the border follows it.">
            <div style={{ color: "#5cc8ff" }}>
              <div
                style={{ height: 56, borderRadius: 8, borderWidth: 6, borderColor: "currentColor" }}
                className="w-full"
              />
            </div>
          </Sample>
          <Sample label='backgroundColor: "color-mix(in oklch, currentColor 30%, black)"'>
            <div style={{ color: "#f9c74f" }}>
              <Swatch color="color-mix(in oklch, currentColor 30%, black)" />
            </div>
          </Sample>
        </Grid>
      </Panel>

      <Panel
        title="A value the parser rejects"
        note="Painting something arbitrary would be worse than painting nothing, so the declaration drops and the element keeps what it had."
      >
        <Grid>
          <Sample label='backgroundColor: "banana"' hint="Nothing painted.">
            <div style={{ height: 56, borderRadius: 8, borderWidth: 1, borderColor: "var(--color-line)", backgroundColor: "banana" }} className="w-full" />
          </Sample>
          <Sample label='backgroundColor: "hsv(0 100% 100%)"' hint="No CSS specification defines hsv().">
            <div style={{ height: 56, borderRadius: 8, borderWidth: 1, borderColor: "var(--color-line)", backgroundColor: "hsv(0 100% 100%)" }} className="w-full" />
          </Sample>
        </Grid>
      </Panel>
    </div>
  )
}

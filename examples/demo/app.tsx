/// The shell around the panels.
///
/// The whole palette is custom properties on one element. Every class token
/// points at one of them, so switching the palette changes one declaration at
/// the root and the whole tree follows it on the next frame. No token is
/// resolved again, because the class channel never held a colour.

import React, { useState } from "react"
import { useGpuixRequired } from "@gpuix/react"
import type { StyleDesc } from "@gpuix/react"
import { ClassNames } from "./class-names.js"
import { Colors } from "./colors.js"
import { Gradients } from "./gradients.js"
import { Corners } from "./corners.js"
import { Inheritance } from "./inheritance.js"
import { Lengths } from "./lengths.js"
import { Motion } from "./motion-panel.js"
import { frameOverlay, Perf } from "./perf.js"
import { Selectors } from "./selectors.js"
import { Variables } from "./variables.js"

/// The palette every panel reads. Exported so a test can mount one panel
/// on its own and still get the colours.
export const PALETTES: Record<string, StyleDesc> = {
  midnight: {
    "--color-bg": "#0b0b12",
    "--color-panel": "#14141d",
    "--color-raised": "#1c1c28",
    "--color-track": "#23232f",
    "--color-line": "#2b2b3a",
    "--color-fg": "#e8e8f2",
    "--color-muted": "#9a9ab4",
    "--color-faint": "#6b6b85",
    "--color-brand": "#7c6cff",
  },
  forest: {
    "--color-bg": "#07120d",
    "--color-panel": "#0e1c15",
    "--color-raised": "#16281f",
    "--color-track": "#1d3227",
    "--color-line": "#24402f",
    "--color-fg": "#e4f2e9",
    "--color-muted": "#8fb5a0",
    "--color-faint": "#5f8570",
    "--color-brand": "#22c55e",
  },
  paper: {
    "--color-bg": "#f4f4f7",
    "--color-panel": "#ffffff",
    "--color-raised": "#ececf2",
    "--color-track": "#e2e2ea",
    "--color-line": "#d5d5e0",
    "--color-fg": "#15151f",
    "--color-muted": "#54546a",
    "--color-faint": "#8a8aa0",
    "--color-brand": "#5b4bd6",
  },
}

type PaletteName = keyof typeof PALETTES

/// Declarations every palette shares.
export const BASE: StyleDesc = {
  "--spacing": "4px",
  "--font-mono": "Menlo",
  "--color-brand-soft": "color-mix(in oklch, var(--color-brand) 22%, var(--color-panel))",
}

const SECTIONS = [
  { id: "colors", title: "Colours", render: () => <Colors /> },
  { id: "gradients", title: "Gradients", render: () => <Gradients /> },
  { id: "corners", title: "Corner shape", render: () => <Corners /> },
  { id: "lengths", title: "Lengths", render: () => <Lengths /> },
  { id: "variables", title: "Variables", render: () => <Variables /> },
  { id: "inheritance", title: "Inheritance", render: () => <Inheritance /> },
  { id: "classes", title: "className", render: () => <ClassNames /> },
  { id: "selectors", title: "Selectors", render: () => <Selectors /> },
  { id: "motion", title: "Motion", render: () => <Motion /> },
] as const

type SectionId = (typeof SECTIONS)[number]["id"] | "perf"

function SidebarItem({ title, active, onClick }: {
  title: string
  active: boolean
  onClick: () => void
}) {
  return (
    <div
      className={[
        "row items-center px-3 py-2 rounded pointer select-none",
        active ? "bg-brand-soft" : "hover:bg-raised",
      ].join(" ")}
      onClick={onClick}
    >
      <text className={active ? "text-sm font-semibold text-fg" : "text-sm text-muted"}>{title}</text>
    </div>
  )
}

export function App() {
  const [section, setSection] = useState<SectionId>("colors")
  const [palette, setPalette] = useState<PaletteName>("midnight")
  const current = SECTIONS.find((entry) => entry.id === section)
  // The performance panel reads the frame overlay, which a renderer may not
  // have. It is in the sidebar only when this one does.
  const overlay = frameOverlay(useGpuixRequired())

  return (
    <div
      style={{ ...BASE, ...PALETTES[palette], backgroundColor: "var(--color-bg)" }}
      className="row full"
    >
      <div
        className="col gap-1 p-3 border"
        style={{ width: 190, height: "100%", backgroundColor: "var(--color-panel)" }}
      >
        <div className="col gap-1 px-3 py-3">
          <text className="text-lg font-bold text-fg">GPUIX</text>
          <text className="text-xs text-faint">CSS values, classes, motion</text>
        </div>
        {SECTIONS.map((entry) => (
          <SidebarItem
            key={entry.id}
            title={entry.title}
            active={entry.id === section}
            onClick={() => setSection(entry.id)}
          />
        ))}
        {overlay ? (
          <SidebarItem title="Performance" active={section === "perf"} onClick={() => setSection("perf")} />
        ) : null}

        <div className="grow" />
        <text className="text-xs text-faint px-3 pt-3">Palette</text>
        {(Object.keys(PALETTES) as PaletteName[]).map((name) => (
          <SidebarItem
            key={name}
            title={name}
            active={name === palette}
            onClick={() => setPalette(name)}
          />
        ))}
      </div>

      <div className="col grow scroll-y p-5 gap-4 min-h-0" style={{ height: "100%", minWidth: 0 }}>
        {section === "perf" && overlay ? <Perf renderer={overlay} /> : current?.render()}
      </div>
    </div>
  )
}

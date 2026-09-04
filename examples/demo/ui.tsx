/// Small pieces the panels share.
///
/// Everything here is written with `className`, so the chrome of the demo runs
/// through the same class channel the class panel demonstrates.

import React from "react"
import type { ReactNode } from "react"
import type { StyleDesc } from "@gpuix/react"

export function Panel({ title, note, children }: {
  title: string
  note?: string
  children: ReactNode
}) {
  return (
    <div className="col gap-3 p-5 rounded bg-panel border w-full" style={{ minWidth: 0 }}>
      <div className="col gap-1">
        <text className="text-lg font-semibold text-fg">{title}</text>
        {note ? <text className="text-sm text-muted leading-1.5">{note}</text> : null}
      </div>
      {children}
    </div>
  )
}

/// A labelled sample. The label is the source text the sample was written
/// with, so the picture and the code sit next to each other.
export function Sample({ label, hint, children }: {
  label: string
  hint?: string
  children: ReactNode
}) {
  return (
    <div className="col gap-2" style={{ width: 200, flexShrink: 0 }}>
      {children}
      <text className="mono text-xs text-muted">{label}</text>
      {hint ? <text className="text-xs text-faint">{hint}</text> : null}
    </div>
  )
}

export function Grid({ children }: { children: ReactNode }) {
  return (
    <div className="row wrap gap-4 w-full" style={{ minWidth: 0 }}>
      {children}
    </div>
  )
}

export function Swatch({ color, style }: { color?: string; style?: StyleDesc }) {
  return (
    <div
      style={{
        height: 56,
        borderRadius: 8,
        backgroundColor: color,
        ...style,
      }}
      className="w-full"
    />
  )
}

/// A bar whose painted width is the length under test.
export function Bar({ length, tint }: { length: string | number; tint?: string }) {
  return (
    <div className="row w-full rounded bg-track" style={{ height: 18 }}>
      <div
        style={{
          // The length is a padding rather than a width, because `width` also
          // takes `auto` and a percentage, and these panels show what the
          // length parser alone reads. The box has no children, so the padding
          // is the whole of it. The track is a flex row, so without
          // `flexShrink` a bar longer than the track would be squeezed back
          // into it.
          flexShrink: 0,
          paddingLeft: length,
          height: 18,
          borderRadius: 6,
          backgroundColor: tint ?? "var(--color-brand)",
        }}
      />
    </div>
  )
}

export function Button({ label, active, onClick }: {
  label: string
  active?: boolean
  onClick: () => void
}) {
  return (
    <div
      className={[
        "row items-center justify-center px-3 py-2 rounded pointer select-none border",
        active ? "bg-brand" : "bg-raised",
        "hover:bg-brand-soft",
      ].join(" ")}
      onClick={onClick}
    >
      <text className="text-sm font-medium text-fg">{label}</text>
    </div>
  )
}

export function Row({ children }: { children: ReactNode }) {
  return <div className="row items-center gap-2 wrap">{children}</div>
}

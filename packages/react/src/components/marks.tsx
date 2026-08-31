import { createElement, forwardRef } from "react"
import type { PublicInstance, Props } from "../types/host.js"
import { motion, MotionDivProps } from "./index.js"

export interface CanvasPathCommand {
  type: "move" | "line" | "curve" | "close"
  x?: number
  y?: number
  x1?: number
  y1?: number
  x2?: number
  y2?: number
}

export interface CanvasShape {
  type?: "path" | "body" | "eyes" | "stroke"
  path?: CanvasPathCommand[]
  fill?: string
  stroke?: string
  strokeWidth?: number
  
  // Body props
  squash?: number
  breatheLoop?: number // speed multiplier

  // Eyes props
  glanceX?: number
  glanceY?: number
  blink?: number
  blinkLoop?: number
  
  // Stroke props
  wiggle?: number
  wiggleLoop?: number
}

export interface CanvasProps extends Props {
  shapes?: CanvasShape[]
}

export const Canvas = forwardRef<PublicInstance, CanvasProps>(function Canvas(
  props,
  ref
) {
  return createElement("canvas", { ...props, ref })
})


export function VectorPath({ path, fill, stroke, strokeWidth }: { path: CanvasPathCommand[], fill?: string, stroke?: string, strokeWidth?: number }) {
  return <Canvas shapes={[{ type: "path", path, fill, stroke, strokeWidth }]} style={{ width: '100%', height: '100%' }} />
}

export function OrganicBody({ path, fill, squash = 0, breatheLoop, stroke, strokeWidth }: { path: CanvasPathCommand[], fill?: string, stroke?: string, strokeWidth?: number, squash?: number, breatheLoop?: number }) {
  return <Canvas shapes={[{ type: "body", path, fill, stroke, strokeWidth, squash, breatheLoop }]} style={{ width: '100%', height: '100%' }} />
}

export function VectorEyes({ path, stroke, strokeWidth, glanceX = 0, glanceY = 0, blink = 0, blinkLoop }: { path: CanvasPathCommand[], stroke: string, strokeWidth?: number, glanceX?: number, glanceY?: number, blink?: number, blinkLoop?: number }) {
  return <Canvas shapes={[{ type: "eyes", path, stroke, strokeWidth, glanceX, glanceY, blink, blinkLoop }]} style={{ width: '100%', height: '100%' }} />
}

export function DecorativeStroke({ path, stroke, strokeWidth, wiggle = 0, wiggleLoop }: { path: CanvasPathCommand[], stroke: string, strokeWidth?: number, wiggle?: number, wiggleLoop?: number }) {
  return <Canvas shapes={[{ type: "stroke", path, stroke, strokeWidth, wiggle, wiggleLoop }]} style={{ width: '100%', height: '100%' }} />
}

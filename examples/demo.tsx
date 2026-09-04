/**
 * Every feature on the css-values-and-classname branch, in one window.
 *
 * Colour values, lengths and arithmetic, custom properties, inheritance, the
 * `className` channel and the `height: auto` animation each get a panel. Pick
 * one in the sidebar.
 *
 * Run with:  cd examples && bun run demo
 */

import React from "react"
import { render } from "@gpuix/react"
import { App } from "./demo/app.js"
import { countedResolveClassName } from "./demo/classes.js"

render(<App />, {
  title: "GPUIX",
  width: 1180,
  height: 820,
  minWidth: 720,
  minHeight: 520,
  resolveClassName: countedResolveClassName,
})

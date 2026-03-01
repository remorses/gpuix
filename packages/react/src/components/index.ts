// GPUIX component type definitions
// These map to GPUI element types

export const gpuixComponents = {
  div: "div",
  text: "text",
  img: "img",
  svg: "svg",
  canvas: "canvas",
  input: "input",
} as const

export type GpuixComponentType = keyof typeof gpuixComponents

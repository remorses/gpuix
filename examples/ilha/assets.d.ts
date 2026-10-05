// Bun inlines `import icon from "./icon.svg" with { type: "text" }` as a string.
declare module "*.svg" {
  const source: string
  export default source
}

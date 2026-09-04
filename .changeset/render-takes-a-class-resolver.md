---
"@gpuix/react": minor
---

Pass a class resolver to `render()`.

`createRoot` took `resolveClassName`, but `render()` did not, so an application
that wanted `className` had to open the window and build the root by hand.
`render()` now takes the same root options:

```ts
render(<App />, { resolveClassName, title: "Demo" })
```

A `Root` also carries the `renderer` it draws on, so an application that lets
`render()` open the window can still reach the handle afterwards. Inside the
tree, `useGpuixRequired()` gives the same renderer.

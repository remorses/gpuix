'@gpuix/ilha': minor
---

Add **`@gpuix/ilha`**, a GPUIX renderer for [ilha](https://ilha.build). ilha components, atoms, streams and generators run unchanged; only the host changes.

```bash
bun add --exact @gpuix/ilha @gpuix/native ilha effect
```

```json
{
  "compilerOptions": {
    "jsx": "react-jsx",
    "jsxImportSource": "@gpuix/ilha"
  }
}
```

```tsx
import { atom } from 'ilha'
import { render } from '@gpuix/ilha'

const Counter = () => {
  const count = atom(0)
  return (
    <div style={{ display: 'flex', gap: 8, padding: 12 }}>
      <text>Count: {count}</text>
      <div onclick={() => count.update((n: number) => n + 1)}>+</div>
    </div>
  )
}

render(Counter)
```

The adapter drives ilha's own painter through the new `ilha/renderer` entry point, so every ilha feature works: `atom`, `watch`, `when`, generators, `resource`, contexts, error boundaries and keyed lists.

- Element identity survives a repaint: a focused `<input>` keeps its native element, caret and scroll offset while the component rerenders
- A nested component adds no wrapper box — its output is spliced into the parent, like ilha's DOM `display: contents` slot host
- Props are passed to GPUIX as they are, so object-valued props such as `highlight` and `theme` work; `style` accepts a `StyleDesc` object or a CSS declaration string
- A repaint sends only the props and styles that changed, and clears a prop the next render drops
- A skipped view (`{cond && …}`) adds no native element, so it takes no `gap` in a flex parent
- Lowercase ilha event props (`onclick`, `oninput`, `onkeydown`, …) map to the GPUIX events of the same element. Every GPUIX event also works under its own name (`onLinkClick`, `onMouseDownOutside`, `onHighlight`, `onVisibleRange`, …)
- The handler receives the GPUIX `EventPayload` plus `type`, `target` and `currentTarget`. On `onkeydown`, `preventDefault()` cancels Tab focus movement and `stopPropagation()` skips ancestor handlers
- `ref` receives the `HostElement`: `id`, `scrollIntoView()`, `setImage()`, `setImagePixels()`
- `windowSize()`, `windowInsets()`, `selectedText()` and `textSearch()` are the ilha counterparts of the React hooks and Solid primitives, each returning an atom
- `useGpuix()` / `useGpuixRequired()` read the native renderer from any component
- `onUncaughtError` receives a failing root component, a failing nested component no `ErrorBoundary` caught, and a failing commit
- `createTestRoot()` from `@gpuix/ilha/testing` mounts against the GPU-backed test renderer

- `Button`, `Select`, `Combobox`, `Dialog`, `Tooltip`, `AnimatePresence` and `motion.div` are the same headless parts as in React and Solid: `style` as a function of the part's state, `asChild`, a render function as the child of `SelectItem`, `ComboboxItem`, `ComboboxList` and `ComboboxValue`. Also importable from `@gpuix/ilha/button`, `/select`, `/combobox`, `/dialog`, `/tooltip` and `/floating`
- `usePresence()` and `useIsPresent()` return plain values, because an ilha component reruns when its presence changes

`examples/ilha/chat.tsx` is the chat example in ilha, with the same automation check and mount script as `examples/solid`.

`@gpuix/ilha` requires `ilha@^0.15.1`: 0.15 for the `ilha/renderer` host entry point, 0.15.1 for the keyed component and repaint fixes the controls rely on.

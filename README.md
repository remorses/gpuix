# GPUIX

React bindings for [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) - Zed's GPU-accelerated UI framework.

## Architecture

GPUIX bridges React to GPUI using a **description-based renderer** that matches GPUI's immediate-mode architecture:

```
┌─────────────────────────────────────────────────────────┐
│  React (JavaScript)                                     │
│                                                         │
│  function App() {                                       │
│    const [count, setCount] = useState(0)                │
│    return (                                             │
│      <div style={{ display: 'flex', gap: 8 }}>          │
│        <text>Count: {count}</text>                      │
│        <div onClick={() => setCount(c => c + 1)}>       │
│          Click me                                       │
│        </div>                                           │
│      </div>                                             │
│    )                                                    │
│  }                                                      │
└─────────────────────────────────────────────────────────┘
                         ↓ JSON element tree
┌─────────────────────────────────────────────────────────┐
│  Rust (napi-rs)                                         │
│                                                         │
│  // Rebuild GPUI elements from description each frame   │
│  fn build_element(desc: &ElementDesc) -> AnyElement     │
└─────────────────────────────────────────────────────────┘
                         ↓
┌─────────────────────────────────────────────────────────┐
│  GPUI                                                   │
│                                                         │
│  GPU-accelerated rendering via Metal/Vulkan             │
└─────────────────────────────────────────────────────────┘
```

## Why This Works

GPUI is an **immediate-mode** UI framework - it rebuilds the entire element tree every frame. This actually aligns perfectly with React's declarative model:

1. React reconciler builds a virtual element tree
2. Tree is serialized to JSON and sent to Rust via napi-rs
3. Rust rebuilds GPUI elements from the description
4. GPUI renders to GPU

## Packages

- **`@gpuix/native`** - Rust/napi-rs bindings to GPUI
- **`@gpuix/react`** - React reconciler and components

## Building

### Prerequisites

1. Rust toolchain
2. Node.js 18+
3. Xcode with Metal Toolchain (macOS)

```bash
# Install Metal Toolchain if needed
xcodebuild -downloadComponent MetalToolchain

# Install dependencies
bun install

# Build native package
cd packages/native
bun run build

# Build React package
cd ../react
bun run build

# Run example (use tmux for long-running sessions)
cd ../../examples
npx tsx counter.tsx
```

## Usage

```tsx
import { createRoot, flushSync, GpuixRenderer } from '@gpuix/react'

// Create the native renderer
const renderer = new GpuixRenderer((event) => {
  // Handle events from GPUI
  console.log('Event:', event)
})

// Create React root
const root = createRoot(renderer)

// Render your app
flushSync(() => {
  root.render(<App />)
})

// Start the GPUI event loop
renderer.run()
```

## Supported Elements

| Element | Description |
|---------|-------------|
| `div` | Container with flexbox layout |
| `text` | Text content |
| `img` | Images |
| `svg` | Vector graphics |
| `canvas` | Custom drawing |

## Supported Events

- `onClick`, `onMouseDown`, `onMouseUp`
- `onMouseEnter`, `onMouseLeave`, `onMouseMove`
- `onKeyDown`, `onKeyUp`
- `onFocus`, `onBlur`
- `onScroll`

## Supported Styles

Tailwind-like styling via the `style` prop:

```tsx
<div style={{
  display: 'flex',
  flexDirection: 'column',
  gap: 8,
  padding: 16,
  backgroundColor: '#3b82f6',
  borderRadius: 8,
}}>
  <text style={{ color: '#ffffff', fontSize: 18 }}>
    Hello GPUI!
  </text>
</div>
```

## Status

⚠️ **Work in Progress**

- [x] React reconciler (based on opentui)
- [x] Element tree serialization
- [x] napi-rs bindings structure  
- [x] Style mapping (CSS → GPUI)
- [x] Event callback system
- [x] GPUI element building (build_element, apply_styles)
- [x] Event wiring (click, mouseDown, mouseUp, mouseMove)
- [x] **Standalone build** - Pinned GPUI and macOS deps for compatibility
- [ ] Focus management
- [ ] Keyboard events
- [ ] Text input

### Current Blocker

None currently. If native builds regress, check the GPUI pin and macOS `core-text`/`core-graphics` versions in `packages/native/Cargo.toml`.

## Documentation

See [AGENTS.md](./AGENTS.md) for detailed architecture, communication flow, and contributing guide.

## License

Apache-2.0

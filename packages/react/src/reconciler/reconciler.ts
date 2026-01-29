import ReactReconciler from "react-reconciler"
import { hostConfig } from "./host-config"

// Cast to any because @types/react-reconciler is out of date with react-reconciler 0.31.0
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const reconciler = ReactReconciler(hostConfig as any)

// Inject into DevTools if available
try {
  // @ts-expect-error the types for `react-reconciler` are not up to date with the library
  reconciler.injectIntoDevTools()
} catch {
  // DevTools not available
}

import ReactReconciler from "react-reconciler"
import { hostConfig } from "./host-config"

export const reconciler = ReactReconciler(hostConfig)

// Inject into DevTools if available
try {
  // @ts-expect-error the types for `react-reconciler` are not up to date with the library
  reconciler.injectIntoDevTools()
} catch {
  // DevTools not available
}

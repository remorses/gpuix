import { activeRenderer } from "./reconciler/renderer.js"

/**
 * Copy `text` to the OS clipboard.
 *
 * A command, not state: it reads the mounted renderer once and returns nothing,
 * so there is no hook to subscribe to. An empty string is a real clipboard
 * entry, which is how an app clears the clipboard.
 *
 * Throws when no window is mounted or when the host has no clipboard binding,
 * so a copy button that cannot copy never looks like it worked.
 */
export function copyText(text: string): void {
  const renderer = activeRenderer()
  if (!renderer?.writeToClipboard) {
    throw new Error(
      "copyText needs a mounted GPUIX window with clipboard support. Call render() first.",
    )
  }
  renderer.writeToClipboard(text)
}

/**
 * GPUIX bindings for [ilha](https://ilha.build).
 *
 * ilha's components, atoms, generators and streams run unchanged; only the host
 * changes. `render(Component)` mounts the same component tree into a GPUIX
 * window instead of the DOM. State primitives stay in `ilha`; this package
 * exports the renderer, the host, and the window primitives (`windowSize`,
 * `windowInsets`, `selectedText`, `textSearch`).
 *
 * ```tsx
 * import { atom } from "ilha"
 * import { render } from "@gpuix/ilha"
 *
 * const Counter = () => {
 *   const count = atom(0)
 *   return (
 *     <div style={{ display: "flex", gap: 8 }}>
 *       <text>Count: {count}</text>
 *       <div onclick={() => count.update((n: number) => n + 1)}>+</div>
 *     </div>
 *   )
 * }
 *
 * render(Counter)
 * ```
 */
export type { HostRenderer, Painter, PaintOps } from "ilha/renderer"

export {
  HostElement,
  HostRoot,
  HostText,
  createHostElement,
  createHostText,
  gpuixOps,
} from "./host.js"
export type {
  HostGpuixContext,
  HostListener,
  HostNode,
  HostParent,
} from "./host.js"
export { hostEventFor, gpuixEventTypeFor } from "./events.js"
export type { HostEvent } from "./events.js"
export { elementTypeForTag, parseStyleText } from "./props.js"
export { createRoot, useGpuix, useGpuixRequired } from "./root.js"
export type { GpuixContextValue, Root } from "./root.js"
export {
  selectedText,
  textSearch,
  windowInsets,
  windowSize,
} from "./primitives.js"
export type { TextSearch } from "./primitives.js"
export { createRenderer, render, resetRender } from "./renderer.js"
export type { RenderOptions, Renderer } from "./renderer.js"
export { motion } from "./components/motion.js"
export type { MotionDivProps } from "./components/motion.js"
export {
  AnimatePresence,
  PresenceContext,
  useIsPresent,
  usePresence,
} from "./components/animate-presence.js"
export type {
  AnimatePresenceProps,
  PresenceContextValue,
} from "./components/animate-presence.js"
export {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxGroup,
  ComboboxInput,
  ComboboxItem,
  ComboboxLabel,
  ComboboxList,
  ComboboxSeparator,
  ComboboxTrigger,
  ComboboxValue,
} from "./components/combobox.js"
export type {
  ComboboxInputProps,
  ComboboxItemProps,
  ComboboxItemState,
  ComboboxListProps,
  ComboboxProps,
  ComboboxTriggerProps,
  ComboboxValueProps,
} from "./components/combobox.js"
export {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "./components/tooltip.js"
export type {
  TooltipContentProps,
  TooltipProps,
  TooltipProviderProps,
  TooltipTriggerProps,
} from "./components/tooltip.js"
export { Button, buttonProps } from "./components/button.js"
export type {
  ButtonBehavior,
  ButtonProps,
  ButtonState,
} from "./components/button.js"
export {
  Dialog,
  DialogBackdrop,
  DialogClose,
  DialogDescription,
  DialogPopup,
  DialogPortal,
  DialogTitle,
  DialogTrigger,
} from "./components/dialog.js"
export type {
  DialogBackdropProps,
  DialogCloseProps,
  DialogPopupProps,
  DialogPortalProps,
  DialogProps,
  DialogTriggerProps,
} from "./components/dialog.js"
export {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectScrollDownButton,
  SelectScrollUpButton,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "./components/select.js"
export type {
  SelectContentProps,
  SelectItemData,
  SelectItemProps,
  SelectItemState,
  SelectProps,
  SelectTriggerProps,
  SelectTriggerState,
  SelectValueProps,
} from "./components/select.js"
export {
  DismissableLayer,
  FloatingLayer,
  floatingRootStyle,
  mergeStyles,
  renderSlot,
  resolveStyle,
} from "./components/floating.js"
export type {
  DismissableLayerProps,
  FloatingContentProps,
  StateStyle,
} from "./components/floating.js"
export type {
  Bindable,
  DivProps,
  ElementProps,
  FormProps,
  Handler,
  IlhaEvents,
} from "./types.js"
export * from "@gpuix/native/host"
export { GpuixRenderer } from "@gpuix/native"
export type { EventModifiers, EventPayload, WindowOptions } from "@gpuix/native"

// GPUIX React - React bindings for GPUI
export { createRoot, flushSync } from "./reconciler/index.js"
export {
  createRenderer,
  enableAutomation,
  render,
  resetRender,
  startFrameLoop,
} from "./reconciler/renderer.js"
export { GpuixContext, useGpuix, useGpuixRequired } from "./hooks/use-gpuix.js"
export { useWindowInsets, useWindowSize } from "./hooks/use-window-size.js"
export { findRanges, useTextSearch } from "./hooks/use-text-search.js"
export {
  createTextSearchController,
  observeSelectedText,
  observeWindowInsets,
  observeWindowSize,
  pushDismissLayer,
  readWindowInsets,
  readWindowSize,
} from "@gpuix/native/host"
export type { DismissLayer } from "@gpuix/native/host"
export type {
  FindRangesOptions,
  TextSearch,
  TextSearchOptions,
} from "./hooks/use-text-search.js"
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
export { Button, buttonProps } from "./components/button.js"
export type { ButtonBehavior, ButtonProps, ButtonState } from "./components/button.js"
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
export { DismissableLayer } from "./components/floating.js"
export type { DismissableLayerProps } from "./components/floating.js"
export type {
  DialogBackdropProps,
  DialogCloseProps,
  DialogPopupProps,
  DialogPortalProps,
  DialogProps,
  DialogTriggerProps,
} from "./components/dialog.js"
export { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "./components/tooltip.js"
export type {
  TooltipContentProps,
  TooltipProps,
  TooltipProviderProps,
  TooltipTriggerProps,
} from "./components/tooltip.js"
export {
  AnimatePresence,
  motion,
  useIsPresent,
  usePresence,
} from "./components/index.js"
export type { AnimatePresenceProps } from "./components/index.js"
export { onFrame, stepSpring, GELATIN } from "./motion-spring.js"
export type { Root, FrameLoop, RenderOptions } from "./reconciler/renderer.js"
export type {
  WindowInsets,
  WindowInsetsOptions,
  WindowSize,
  WindowSizeOptions,
} from "./hooks/use-window-size.js"

// Re-export types
export type { MotionDivProps } from "./components/index.js"
export type {
  AnchoredProps,
  CodeProps,
  CursorValue,
  DebugFrameOverlayMode,
  DebugFrameOverlayStats,
  DiffProps,
  DimensionValue,
  EdgeInsets,
  ElementBounds,
  GpuixMetrics,
  GpuixTheme,
  HighlightMatch,
  HighlightSpec,
  ImgInstance,
  ImgProps,
  FocusTarget,
  InputProps,
  KeyEvent,
  LinearGradientBackground,
  LinearGradientStop,
  MarkdownProps,
  MotionEase,
  MotionProps,
  MotionStyle,
  MotionTransition,
  MotionSpringTransition,
  MotionTweenTransition,
  MutationHost,
  NativeRenderer,
  NativeWindowInsets,
  PathPromptOptions,
  Props,
  PublicInstance,
  StyleDesc,
  SvgProps,
  SelectionHost,
  SyntaxTheme,
  TextareaProps,
  VirtualListProps,
  WindowInsetsHost,
  WindowKeyEventHandler,
  WindowKeyEventHandlers,
  WindowSizeHost,
} from "./types/host.js"
export { handleGpuixEvent } from "./reconciler/event-registry.js"
export {
  applyMacCpuThrottleFromEnv,
  MAC_CPU_THROTTLES,
  readMacCpuThrottle,
} from "./cpu-throttle.js"
export type { MacCpuThrottle } from "./cpu-throttle.js"
export type {
  EventPayload,
  EventModifiers,
  WindowOptions,
  WindowSize as NativeWindowSize,
} from "@gpuix/native"

export { GpuixRenderer } from "@gpuix/native"

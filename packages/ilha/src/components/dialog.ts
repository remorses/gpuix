/** Headless Base UI-shaped Dialog over a full-window GPUIX anchored layer. */

import { resolveFocusTarget } from "@gpuix/native/host"
import type { FocusTarget } from "@gpuix/native/host"
import { atom, context, createContext } from "ilha"
import type { View } from "ilha"

import type { HostElement } from "../host.js"
import { useGpuixRequired } from "../root.js"
import type { DivProps } from "../types.js"
import { buttonProps, splitButtonProps } from "./button.js"
import {
  DismissableLayer,
  el,
  renderSlot,
  splitHandlers,
  styleObject,
} from "./floating.js"

interface DialogState {
  open(): boolean
  modal(): boolean
  setOpen(open: boolean): void
  pointerDismissal(): boolean
  trigger: { current: HostElement | null }
}

const DialogContext = createContext<DialogState | undefined>(undefined)

function useDialog(name: string): DialogState {
  const value = context(DialogContext)()
  if (!value) {
    throw new Error(`${name} must be used inside Dialog`)
  }
  return value
}

export interface DialogProps {
  children?: View
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
  /** Default true: blocks the page behind, keeps Tab inside the popup. */
  modal?: boolean
  /** Keep the dialog open when the backdrop is pressed. */
  disablePointerDismissal?: boolean
}

export function Dialog(props: DialogProps): View {
  const internalOpen = atom(props.defaultOpen ?? false)
  // The parts read the latest props through this box; a controlled `open`
  // reaches them because a parent repaint reruns its nested components.
  const box = atom.lazy(() => ({ props }))()
  box.props = props
  const state = atom.lazy<DialogState>(() => {
    const open = () =>
      box.props.open === undefined ? internalOpen() : box.props.open
    return {
      modal: () => box.props.modal ?? true,
      open,
      pointerDismissal: () => !box.props.disablePointerDismissal,
      setOpen(next) {
        if (open() === next) {
          return
        }
        if (box.props.open === undefined) {
          internalOpen.set(next)
        }
        box.props.onOpenChange?.(next)
      },
      trigger: { current: null },
    }
  })()
  return el(DialogContext.Provider, { value: state }, props.children)
}

export interface DialogTriggerProps extends DivProps {
  asChild?: boolean
}

export function DialogTrigger(props: DialogTriggerProps): View {
  const state = useDialog("DialogTrigger")
  const { behavior, rest } = splitButtonProps(props)
  const { asChild, children, ref, ...host } = rest
  return renderSlot({
    asChild: asChild as boolean | undefined,
    children: children as View,
    defaultTabIndex: 0,
    props: {
      ...host,
      ...buttonProps({
        ...behavior,
        onClick(event) {
          behavior.onClick?.(event)
          state.setOpen(true)
        },
      }),
      "aria-expanded": state.open(),
      ref(element: HostElement | null) {
        state.trigger.current = element
        ;(ref as DivProps["ref"])?.(element)
      },
    },
  })
}

export interface DialogPortalProps extends DivProps {}

/**
 * A full-window layer painted after the page, so it covers `<virtual-list>`
 * and everything else. Mounted only while the dialog is open.
 */
export function DialogPortal(props: DialogPortalProps): View {
  const state = useDialog("DialogPortal")
  if (!state.open()) {
    return null
  }
  const { children, style, ...rest } = props
  const own = styleObject(style)
  return el(
    "anchored",
    {
      ...rest,
      deferred: true,
      // Native sizes the layer to the viewport, so a resize never leaves an
      // uncovered strip that clicks could reach.
      fill: "window",
      occlude: state.modal(),
      // Below menus and tooltips (priority 1), so a Select inside the popup
      // opens on top of it.
      priority: 0,
      style: {
        alignItems: "center",
        // Explicit transparent: the Backdrop paints the dim, not the layer.
        backgroundColor: "transparent",
        display: "flex",
        justifyContent: "center",
        ...own,
        pointerEvents: state.modal() ? own?.pointerEvents : "none",
      },
    },
    children
  )
}

export interface DialogBackdropProps extends DivProps {}

/** Covers the window behind the popup. A press on it closes the dialog. */
export function DialogBackdrop(props: DialogBackdropProps): View {
  const state = useDialog("DialogBackdrop")
  const { handlers, rest } = splitHandlers(props, ["mousedown"])
  const { children, style, ...host } = rest
  return el(
    "div",
    {
      ...host,
      onMouseDown(event: Parameters<NonNullable<typeof handlers.mousedown>>[0]) {
        handlers.mousedown?.(event)
        if (state.pointerDismissal()) {
          state.setOpen(false)
        }
      },
      style: {
        bottom: 0,
        left: 0,
        position: "absolute",
        right: 0,
        top: 0,
        ...styleObject(style),
      },
    },
    children as View
  )
}

export interface DialogPopupProps extends DivProps {
  /** Focus on open. Default: the popup itself, so the first Tab enters it. */
  initialFocus?: FocusTarget<HostElement>
  /** Focus on close. Default: the trigger, else the element focused before open. */
  finalFocus?: FocusTarget<HostElement>
}

/** The dialog surface. Moves focus in on open and back out on close. */
export function DialogPopup(props: DialogPopupProps): View {
  const state = useDialog("DialogPopup")
  const renderer = useGpuixRequired()
  const popup = atom.lazy(() => ({ current: null as HostElement | null }))()
  if (!state.open()) {
    return null
  }
  const { handlers, rest } = splitHandlers(props, ["keydown"])
  const { children, finalFocus, initialFocus, ref, role, style, tabIndex, ...host } =
    rest
  return el(
    DismissableLayer,
    {
      finalFocus: (previous: number | null) =>
        resolveFocusTarget(
          finalFocus as DialogPopupProps["finalFocus"],
          () => state.trigger.current?.id || previous
        ),
      initialFocus: () =>
        resolveFocusTarget(
          initialFocus as DialogPopupProps["initialFocus"],
          () => popup.current?.id || null
        ),
      onEscapeKeyDown: () => state.setOpen(false),
    },
    el(
      "div",
      {
        ...host,
        onKeyDown(event: Parameters<NonNullable<typeof handlers.keydown>>[0]) {
          handlers.keydown?.(event)
          const id = popup.current?.id
          if (
            !state.modal() ||
            event.key !== "tab" ||
            event.defaultPrevented ||
            !id
          ) {
            return
          }
          event.preventDefault()
          if (event.modifiers?.shift) {
            renderer.focusPreviousWithin?.(id)
          } else {
            renderer.focusNextWithin?.(id)
          }
        },
        ref(element: HostElement | null) {
          popup.current = element
          ;(ref as DivProps["ref"])?.(element)
        },
        role: role ?? "dialog",
        // A press inside the popup must never reach the backdrop behind it.
        // The popup is focused so the first Tab enters it; it is a surface,
        // not a control, so it gets no focus look of its own.
        style: {
          keyboardFocusDim: false,
          pointerEvents: "auto",
          ...styleObject(style),
        },
        tabIndex: tabIndex ?? -1,
      },
      children as View
    )
  )
}

export function DialogTitle(props: DivProps): View {
  const { children, ...rest } = props
  return el("div", { "aria-level": 2, role: "heading", ...rest }, children)
}

export function DialogDescription(props: DivProps): View {
  const { children, ...rest } = props
  return el("div", rest, children)
}

export interface DialogCloseProps extends DivProps {
  asChild?: boolean
}

export function DialogClose(props: DialogCloseProps): View {
  const state = useDialog("DialogClose")
  const { behavior, rest } = splitButtonProps(props)
  const { asChild, children, ...host } = rest
  return renderSlot({
    asChild: asChild as boolean | undefined,
    children: children as View,
    defaultTabIndex: 0,
    props: {
      ...host,
      ...buttonProps({
        ...behavior,
        onClick(event) {
          behavior.onClick?.(event)
          state.setOpen(false)
        },
      }),
    },
  })
}

export {
  Dialog as Root,
  DialogBackdrop as Backdrop,
  DialogClose as Close,
  DialogDescription as Description,
  DialogPopup as Popup,
  DialogPortal as Portal,
  DialogTitle as Title,
  DialogTrigger as Trigger,
}

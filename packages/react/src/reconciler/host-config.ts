import { createContext } from "react"
import type { ReactContext } from "react-reconciler"
import { DefaultEventPriority } from "react-reconciler/constants"

// NoEventPriority = 0 in react-reconciler, but types don't export it
const NoEventPriority = 0
import type {
  Container,
  ElementType,
  HostContext,
  Instance,
  Props,
  PublicInstance,
  TextInstance,
} from "../types/host"

let elementIdCounter = 0
let currentUpdatePriority = NoEventPriority

function generateId(type: string): string {
  return `${type}_${++elementIdCounter}`
}

function createInstance(type: ElementType, props: Props): Instance {
  return {
    id: props.id ?? generateId(type),
    type,
    props,
    children: [],
    parent: null,
  }
}

function createTextInstance(text: string): TextInstance {
  return {
    id: generateId("text"),
    text,
    parent: null,
  }
}

// https://github.com/facebook/react/tree/main/packages/react-reconciler#practical-examples
// Type annotation removed - @types/react-reconciler is out of date with react-reconciler 0.31.0
export const hostConfig = {
  supportsMutation: true,
  supportsPersistence: false,
  supportsHydration: false,

  // Create instances
  createInstance(
    type: ElementType,
    props: Props,
    _rootContainerInstance: Container,
    _hostContext: HostContext
  ): Instance {
    const instance = createInstance(type, props)
    console.log("[GPUIX] createInstance:", type, "id:", instance.id)
    return instance
  },

  // Append a child to a parent
  appendChild(parent: Instance, child: Instance | TextInstance): void {
    if ("type" in child) {
      child.parent = parent
      parent.children.push(child)
    } else {
      parent.textContent = (parent.textContent || "") + child.text
      console.log("[GPUIX] appendChild text node -> parent content:", parent.type, parent.textContent)
    }
  },

  // Remove a child from a parent
  removeChild(parent: Instance, child: Instance | TextInstance): void {
    if ("type" in child) {
      const index = parent.children.indexOf(child)
      if (index !== -1) {
        parent.children.splice(index, 1)
        child.parent = null
      }
    }
  },

  // Insert a child before another child
  insertBefore(
    parent: Instance,
    child: Instance | TextInstance,
    beforeChild: Instance | TextInstance
  ): void {
    if ("type" in child && "type" in beforeChild) {
      const index = parent.children.indexOf(beforeChild)
      if (index !== -1) {
        child.parent = parent
        parent.children.splice(index, 0, child)
      }
    }
  },

  // Insert a child before another in container
  insertInContainerBefore(
    _parent: Container,
    _child: Instance,
    _beforeChild: Instance
  ): void {
    // Container doesn't support multiple children in our model
  },

  // Remove a child from container
  removeChildFromContainer(_parent: Container, _child: Instance): void {
    // Will trigger re-render
  },

  // Prepare for commit
  prepareForCommit(_containerInfo: Container): Record<string, unknown> | null {
    return null
  },

  // Reset after commit - trigger GPUI render
  resetAfterCommit(containerInfo: Container): void {
    console.log("[GPUIX] resetAfterCommit called")
    containerInfo.requestRender()
  },

  // Get root context
  getRootHostContext(_rootContainerInstance: Container): HostContext {
    return { isInsideText: false }
  },

  // Get child context
  getChildHostContext(
    parentHostContext: HostContext,
    type: ElementType,
    _rootContainerInstance: Container
  ): HostContext {
    const isInsideText = type === "text"
    return { ...parentHostContext, isInsideText }
  },

  // Should set text content
  shouldSetTextContent(_type: ElementType, _props: Props): boolean {
    return false
  },

  // Create text instance
  createTextInstance(
    text: string,
    _rootContainerInstance: Container,
    hostContext: HostContext
  ): TextInstance {
    if (!hostContext.isInsideText) {
      // Text outside of text elements gets wrapped
    }
    return createTextInstance(text)
  },

  // Schedule timeout
  scheduleTimeout: setTimeout,

  // Cancel timeout
  cancelTimeout: clearTimeout,

  // No timeout
  noTimeout: -1,

  // Should attempt synchronous flush
  shouldAttemptEagerTransition(): boolean {
    return false
  },

  // Finalize initial children
  finalizeInitialChildren(
    _instance: Instance,
    _type: ElementType,
    _props: Props,
    _rootContainerInstance: Container,
    _hostContext: HostContext
  ): boolean {
    return false
  },

  // Commit mount
  commitMount(
    _instance: Instance,
    _type: ElementType,
    _props: Props,
    _internalInstanceHandle: unknown
  ): void {
    // Focus handling could happen here
  },

  // Commit update
  commitUpdate(
    instance: Instance,
    _updatePayload: unknown,
    _type: ElementType,
    _oldProps: Props,
    newProps: Props,
    _internalInstanceHandle: unknown
  ): void {
    instance.props = newProps
    if (newProps.id) {
      instance.id = newProps.id
    }
  },

  // Commit text update
  commitTextUpdate(
    textInstance: TextInstance,
    _oldText: string,
    newText: string
  ): void {
    textInstance.text = newText
  },

  // Append child to container
  appendChildToContainer(container: Container, child: Instance): void {
    console.log("[GPUIX] appendChildToContainer called, child type:", child.type, "id:", child.id)
    // The container will serialize this for GPUI
    const tree = instanceToElementDesc(child)
    console.log("[GPUIX] instanceToElementDesc result:", JSON.stringify(tree, null, 2))
    container.render(tree)
  },

  appendInitialChild(parent: Instance, child: Instance | TextInstance): void {
    console.log("[GPUIX] appendInitialChild:", "type" in child ? child.type : "text", "to parent:", parent.type)
    if ("type" in child) {
      child.parent = parent
      parent.children.push(child)
    } else {
      // Text instance - store as text content
      parent.textContent = (parent.textContent || "") + child.text
      console.log("[GPUIX] appendInitialChild text node -> parent content:", parent.type, parent.textContent)
    }
  },

  // Hide instance
  hideInstance(instance: Instance): void {
    instance.props = { ...instance.props, style: { ...instance.props.style, visibility: "hidden" } }
  },

  // Unhide instance
  unhideInstance(instance: Instance, _props: Props): void {
    const style = { ...(instance.props.style || {}) }
    delete (style as Record<string, unknown>).visibility
    instance.props = { ...instance.props, style }
  },

  // Hide text instance
  hideTextInstance(_textInstance: TextInstance): void {
    // Text visibility handled by parent
  },

  // Unhide text instance
  unhideTextInstance(_textInstance: TextInstance, _text: string): void {
    // Text visibility handled by parent
  },

  // Clear container
  clearContainer(_container: Container): void {
    // Nothing to clear in our model
  },

  // Priority handling
  setCurrentUpdatePriority(newPriority: number): void {
    currentUpdatePriority = newPriority
  },

  getCurrentUpdatePriority: (): number => currentUpdatePriority,

  resolveUpdatePriority(): number {
    if (currentUpdatePriority !== NoEventPriority) {
      return currentUpdatePriority
    }
    return DefaultEventPriority
  },

  maySuspendCommit(): boolean {
    return false
  },

  NotPendingTransition: null,

  HostTransitionContext: createContext(null) as unknown as ReactContext<null>,

  resetFormInstance(): void {},

  requestPostPaintCallback(): void {},

  trackSchedulerEvent(): void {},

  resolveEventType(): null {
    return null
  },

  resolveEventTimeStamp(): number {
    return -1.1
  },

  preloadInstance(): boolean {
    return true
  },

  startSuspendingCommit(): void {},

  suspendInstance(): void {},

  waitForCommitToBeReady(): null {
    return null
  },

  detachDeletedInstance(instance: Instance): void {
    instance.parent = null
    instance.children = []
  },

  getPublicInstance(instance: Instance): PublicInstance {
    return instance
  },

  preparePortalMount(_containerInfo: Container): void {},

  isPrimaryRenderer: true,

  getInstanceFromNode(): null {
    return null
  },

  beforeActiveInstanceBlur(): void {},

  afterActiveInstanceBlur(): void {},

  prepareScopeUpdate(): void {},

  getInstanceFromScope(): null {
    return null
  },
}

// Convert Instance tree to ElementDesc for GPUI
import type { ElementDesc, StyleDesc } from "../types/host"

function instanceToElementDesc(instance: Instance): ElementDesc {
  const events: string[] = []

  // Collect registered events
  if (instance.props.onClick) events.push("click")
  if (instance.props.onMouseDown) events.push("mouseDown")
  if (instance.props.onMouseUp) events.push("mouseUp")
  if (instance.props.onMouseEnter) events.push("mouseEnter")
  if (instance.props.onMouseLeave) events.push("mouseLeave")
  if (instance.props.onMouseMove) events.push("mouseMove")
  if (instance.props.onKeyDown) events.push("keyDown")
  if (instance.props.onKeyUp) events.push("keyUp")
  if (instance.props.onFocus) events.push("focus")
  if (instance.props.onBlur) events.push("blur")
  if (instance.props.onScroll) events.push("scroll")

  const desc: ElementDesc = {
    elementType: instance.type,
    id: instance.id,
    style: instance.props.style as StyleDesc | undefined,
    content: instance.textContent,
    events: events.length > 0 ? events : undefined,
    tabIndex: instance.props.tabIndex,
    tabStop: instance.props.tabStop,
    autoFocus: instance.props.autoFocus,
    children:
      instance.children.length > 0
        ? instance.children.map(instanceToElementDesc)
        : undefined,
  }
  console.log("[GPUIX] instanceToElementDesc:", desc.elementType, "id:", desc.id, "children:", desc.children?.length ?? 0)
  return desc
}

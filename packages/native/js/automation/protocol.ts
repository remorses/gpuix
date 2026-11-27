/// Typed GPUIX automation protocol and SSE codec.
///
/// Wire format is SSE `data:` lines so `eventsource-parser` can extract
/// messages from noisy stdout. JSON carries every field. Do not emit SSE
/// `id:` or `event:` fields; process logs that start with those prefixes
/// would otherwise leak into the next message.

import { createParser, type EventSourceMessage } from "eventsource-parser"
import { z } from "zod"

export const PROTOCOL_VERSION = 1 as const

export const automationErrorCodes = [
  "Timeout",
  "NotFound",
  "Ambiguous",
  "Protocol",
  "Closed",
  "Unsupported",
  "Security",
  "Cancelled",
] as const

export type AutomationErrorCode = (typeof automationErrorCodes)[number]

export class AutomationError extends Error {
  readonly code: AutomationErrorCode
  readonly data?: unknown

  constructor(code: AutomationErrorCode, message: string, data?: unknown) {
    super(message)
    this.name = "AutomationError"
    this.code = code
    this.data = data
  }
}

const errorCodeSchema = z.enum(automationErrorCodes)

const pointSchema = z.object({
  x: z.number(),
  y: z.number(),
})

const buttonSchema = z.number().int().min(0).max(2).optional()

/** Held modifiers, in the same hyphenated syntax as `press("cmd-a")`. */
const modifiersSchema = z.string().optional()

export const boundsSchema = z.object({
  x: z.number(),
  y: z.number(),
  width: z.number(),
  height: z.number(),
})

export type ElementBounds = z.infer<typeof boundsSchema>

export const treeNodeSchema: z.ZodType<TreeNode> = z.lazy(() =>
  z.object({
    id: z.number(),
    type: z.string(),
    text: z.string().optional(),
    testId: z.string().optional(),
    style: z.record(z.string(), z.unknown()).optional(),
    events: z.array(z.string()).optional(),
    customProps: z.record(z.string(), z.unknown()).optional(),
    bounds: boundsSchema.optional(),
    children: z.array(treeNodeSchema).optional(),
  })
)

export interface TreeNode {
  id: number
  type: string
  text?: string
  testId?: string
  style?: Record<string, unknown>
  events?: string[]
  customProps?: Record<string, unknown>
  bounds?: ElementBounds
  children?: TreeNode[]
}

const okSchema = z.object({ ok: z.literal(true) })

const capabilitiesSchema = z.array(
  z.enum(["input", "screenshot", "clock", "tree"])
)

/** Single source of truth for method names, params, and results. */
export const methods = {
  initialize: {
    params: z.object({
      protocolVersion: z.literal(PROTOCOL_VERSION),
      client: z.string(),
    }),
    result: z.object({
      protocolVersion: z.literal(PROTOCOL_VERSION),
      pid: z.number().int(),
      capabilities: capabilitiesSchema,
      window: z.object({
        width: z.number(),
        height: z.number(),
      }),
    }),
  },
  cancel: {
    params: z.object({ id: z.number().int() }),
    result: okSchema,
  },
  click: {
    params: z.object({
      x: z.number(),
      y: z.number(),
      button: buttonSchema,
      modifiers: modifiersSchema,
    }),
    result: okSchema,
  },
  mouseDown: {
    params: pointSchema.extend({
      button: buttonSchema,
      modifiers: modifiersSchema,
    }),
    result: okSchema,
  },
  mouseUp: {
    params: pointSchema.extend({
      button: buttonSchema,
      modifiers: modifiersSchema,
    }),
    result: okSchema,
  },
  mouseMove: {
    params: pointSchema.extend({
      pressedButton: buttonSchema,
      modifiers: modifiersSchema,
    }),
    result: okSchema,
  },
  scrollWheel: {
    params: pointSchema.extend({
      deltaX: z.number(),
      deltaY: z.number(),
      modifiers: modifiersSchema,
    }),
    result: okSchema,
  },
  keystrokes: {
    params: z.object({
      keys: z.string(),
      elementId: z.number().optional(),
    }),
    result: okSchema,
  },
  keyDown: {
    params: z.object({
      key: z.string(),
      isHeld: z.boolean().optional(),
      elementId: z.number().optional(),
    }),
    result: okSchema,
  },
  keyUp: {
    params: z.object({
      key: z.string(),
      elementId: z.number().optional(),
    }),
    result: okSchema,
  },
  focus: {
    params: z.object({ elementId: z.number() }),
    result: okSchema,
  },
  blur: {
    params: z.object({}),
    result: okSchema,
  },
  scrollTo: {
    params: z.object({
      elementId: z.number(),
      x: z.number(),
      y: z.number(),
    }),
    result: okSchema,
  },
  getScrollOffset: {
    params: z.object({ elementId: z.number() }),
    result: z.object({
      offset: z.tuple([z.number(), z.number()]).nullable(),
    }),
  },
  getTree: {
    params: z.object({}),
    result: z.object({ tree: treeNodeSchema.nullable() }),
  },
  getFullTree: {
    params: z.object({}),
    result: z.object({ tree: treeNodeSchema.nullable() }),
  },
  getPaintedText: {
    params: z.object({}),
    result: z.object({ text: z.array(z.string()) }),
  },
  getAllText: {
    params: z.object({}),
    result: z.object({ text: z.array(z.string()) }),
  },
  getBounds: {
    params: z.object({ elementId: z.number() }),
    result: z.object({ bounds: boundsSchema.nullable() }),
  },
  getSelectedText: {
    params: z.object({}),
    result: z.object({ text: z.string().nullable() }),
  },
  clearSelection: {
    params: z.object({}),
    result: okSchema,
  },
  screenshot: {
    params: z.object({ path: z.string() }),
    result: z.object({ path: z.string() }),
  },
  clockPause: {
    params: z.object({}),
    result: z.object({ nowMs: z.number() }),
  },
  clockSet: {
    params: z.object({ nowMs: z.number().nonnegative() }),
    result: z.object({ nowMs: z.number() }),
  },
  clockFastForward: {
    params: z.object({ deltaMs: z.number().nonnegative() }),
    result: z.object({ nowMs: z.number() }),
  },
  clockResume: {
    params: z.object({}),
    result: z.object({ nowMs: z.number() }),
  },
} as const

export type MethodName = keyof typeof methods
export type ParamsOf<M extends MethodName> = z.infer<(typeof methods)[M]["params"]>
export type ResultOf<M extends MethodName> = z.infer<(typeof methods)[M]["result"]>

export type AutomationRequest<M extends MethodName = MethodName> = {
  [K in M]: { id: number; method: K; params: ParamsOf<K> }
}[M]

export type AutomationSuccess<M extends MethodName = MethodName> = {
  [K in M]: { id: number; result: ResultOf<K> }
}[M]

export interface AutomationFailure {
  id: number
  error: {
    code: AutomationErrorCode
    message: string
    data?: unknown
  }
}

export type AutomationResponse<M extends MethodName = MethodName> =
  | AutomationSuccess<M>
  | AutomationFailure

export const serverEventNames = ["console", "frame", "closed"] as const
export type ServerEventName = (typeof serverEventNames)[number]

export type AutomationServerEvent =
  | { event: "console"; params: { text: string } }
  | { event: "frame"; params: { n: number; path?: string } }
  | { event: "closed"; params: { reason: string } }

export type WireMessage =
  | AutomationRequest
  | AutomationResponse
  | AutomationServerEvent

const methodNames = Object.keys(methods) as MethodName[]

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

function parseMethodName(value: unknown): MethodName {
  if (typeof value !== "string" || !methodNames.includes(value as MethodName)) {
    throw new AutomationError(
      "Protocol",
      `Unknown automation method: ${String(value)}`
    )
  }
  return value as MethodName
}

export function parseRequest(value: unknown): AutomationRequest {
  if (!isObject(value)) {
    throw new AutomationError("Protocol", "Request must be an object")
  }
  if (typeof value.id !== "number" || !Number.isInteger(value.id)) {
    throw new AutomationError("Protocol", "Request id must be an integer")
  }
  const method = parseMethodName(value.method)
  const parsed = methods[method].params.safeParse(value.params)
  if (!parsed.success) {
    throw new AutomationError(
      "Protocol",
      `Invalid params for ${method}: ${parsed.error.message}`,
      parsed.error.issues
    )
  }
  return { id: value.id, method, params: parsed.data } as AutomationRequest
}

export function parseResponse(value: unknown): AutomationResponse {
  if (!isObject(value)) {
    throw new AutomationError("Protocol", "Response must be an object")
  }
  if (typeof value.id !== "number" || !Number.isInteger(value.id)) {
    throw new AutomationError("Protocol", "Response id must be an integer")
  }
  if ("error" in value) {
    if (!isObject(value.error) || typeof value.error.message !== "string") {
      throw new AutomationError("Protocol", "Invalid error payload")
    }
    const code = errorCodeSchema.safeParse(value.error.code)
    if (!code.success) {
      throw new AutomationError("Protocol", "Invalid error code")
    }
    return {
      id: value.id,
      error: {
        code: code.data,
        message: value.error.message,
        ...(value.error.data === undefined ? {} : { data: value.error.data }),
      },
    }
  }
  if (!("result" in value)) {
    throw new AutomationError("Protocol", "Response needs result or error")
  }
  return { id: value.id, result: value.result } as AutomationSuccess
}

export function parseServerEvent(value: unknown): AutomationServerEvent {
  if (!isObject(value) || typeof value.event !== "string") {
    throw new AutomationError("Protocol", "Event must have an event name")
  }
  if (value.event === "console") {
    const params = z.object({ text: z.string() }).parse(value.params)
    return { event: "console", params }
  }
  if (value.event === "frame") {
    const params = z
      .object({ n: z.number(), path: z.string().optional() })
      .parse(value.params)
    return { event: "frame", params }
  }
  if (value.event === "closed") {
    const params = z.object({ reason: z.string() }).parse(value.params)
    return { event: "closed", params }
  }
  throw new AutomationError(
    "Protocol",
    `Unknown server event: ${value.event}`
  )
}

export function parseWireMessage(value: unknown): WireMessage {
  if (!isObject(value)) {
    throw new AutomationError("Protocol", "Wire message must be an object")
  }
  if ("method" in value) return parseRequest(value)
  if ("event" in value) return parseServerEvent(value)
  if ("result" in value || "error" in value) return parseResponse(value)
  throw new AutomationError("Protocol", "Unrecognized wire message")
}

export function encodeSse(message: WireMessage): string {
  return `data: ${JSON.stringify(message)}\n\n`
}

export interface SseDecoder {
  feed(chunk: string): void
}

export function createSseDecoder(
  onMessage: (message: WireMessage) => void,
  onInvalid?: (raw: string, error: unknown) => void
): SseDecoder {
  const parser = createParser({
    onEvent(event: EventSourceMessage) {
      try {
        onMessage(parseWireMessage(JSON.parse(event.data)))
      } catch (error) {
        onInvalid?.(event.data, error)
      }
    },
  })
  return {
    feed(chunk: string): void {
      parser.feed(chunk)
    },
  }
}

export function decodeSseChunk(chunk: string): WireMessage[] {
  const messages: WireMessage[] = []
  const decoder = createSseDecoder((message) => {
    messages.push(message)
  })
  decoder.feed(chunk)
  return messages
}

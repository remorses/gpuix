/// GPUIX Playwright-like automation API.

export {
  App,
  browserRendererAsTest,
  browserKeystrokeInit,
  connectStdio,
  connectTest,
  handleAutomationRequest,
  InProcessBackend,
  launch,
  liveRendererAsTest,
  Locator,
  IME_MIRROR_SELECTOR,
  serveAutomationStdio,
  SseBackend,
  VideoRecording,
} from "./client.js"
export type {
  AutomationBackend,
  DragOptions,
  LiveAutomationRenderer,
  MouseOptions,
  PointTarget,
  TestAutomationRenderer,
} from "./client.js"
export {
  AutomationError,
  createSseDecoder,
  decodeSseChunk,
  encodeSse,
  methods,
  parseRequest,
  parseResponse,
  parseWireMessage,
  PROTOCOL_VERSION,
} from "./protocol.js"
export type {
  AutomationErrorCode,
  AutomationRequest,
  AutomationResponse,
  AutomationServerEvent,
  ElementBounds,
  MethodName,
  ParamsOf,
  ResultOf,
  TreeNode,
  WireMessage,
} from "./protocol.js"

/// Platform module for gpui_node — custom GPUI platform that runs inside Node.js.
///
/// Instead of blocking the main thread with [NSApp run] (macOS) or similar,
/// NodePlatform::run() returns immediately and lets JS drive the frame loop
/// by calling tick() on each iteration of the Node.js event loop.

mod node_dispatcher;
mod node_display;
mod node_platform;
mod node_window;

pub use node_platform::NodePlatform;

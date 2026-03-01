#![deny(clippy::all)]

mod custom_elements;
mod element_tree;
mod platform;
mod renderer;
mod retained_tree;
mod style;

#[cfg(all(feature = "test-support", target_os = "macos"))]
mod test_renderer;

pub use element_tree::*;
pub use renderer::*;
pub use style::*;

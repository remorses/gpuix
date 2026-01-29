use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use std::sync::{Arc, Mutex};

use crate::element_tree::{ElementDesc, EventPayload};

/// The main GPUI renderer exposed to Node.js
/// 
/// This struct manages the GPUI application lifecycle and provides
/// methods to render element trees from JavaScript.
#[napi]
pub struct GpuixRenderer {
    /// Callback to send events back to JS
    event_callback: Option<ThreadsafeFunction<EventPayload>>,
    /// Current element tree
    current_tree: Arc<Mutex<Option<ElementDesc>>>,
    /// Whether the renderer is running
    running: Arc<Mutex<bool>>,
}

#[napi]
impl GpuixRenderer {
    /// Create a new GPUI renderer
    /// 
    /// The event_callback will be called whenever a GPUI event fires
    /// that was registered by a React element.
    #[napi(constructor)]
    pub fn new(event_callback: Option<ThreadsafeFunction<EventPayload>>) -> Self {
        Self {
            event_callback,
            current_tree: Arc::new(Mutex::new(None)),
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Render an element tree
    /// 
    /// This method receives a JSON-serialized element tree from React
    /// and triggers a GPUI re-render.
    #[napi]
    pub fn render(&self, tree_json: String) -> Result<()> {
        let tree: ElementDesc = serde_json::from_str(&tree_json)
            .map_err(|e| Error::from_reason(format!("Failed to parse element tree: {}", e)))?;

        let mut current = self.current_tree.lock().unwrap();
        *current = Some(tree);

        // TODO: Trigger GPUI re-render
        // This will be implemented when we integrate with the actual GPUI event loop

        Ok(())
    }

    /// Emit an event back to JavaScript
    pub fn emit_event(&self, payload: EventPayload) {
        if let Some(ref callback) = self.event_callback {
            callback.call(payload, ThreadsafeFunctionCallMode::NonBlocking);
        }
    }

    /// Start the GPUI application
    /// 
    /// This blocks the current thread and runs the GPUI event loop.
    /// The renderer will process render requests from the JS side.
    #[napi]
    pub fn run(&self) -> Result<()> {
        {
            let mut running = self.running.lock().unwrap();
            if *running {
                return Err(Error::from_reason("Renderer is already running"));
            }
            *running = true;
        }

        // TODO: Initialize and run GPUI application
        // This is a placeholder - actual implementation will:
        // 1. Create GPUI Application
        // 2. Open a window
        // 3. Set up the render loop that reads from self.current_tree
        // 4. Wire up event handlers that call self.emit_event()

        // For now, we'll just print a message
        println!("GPUIX Renderer: run() called - GPUI integration pending");

        Ok(())
    }

    /// Stop the GPUI application
    #[napi]
    pub fn stop(&self) -> Result<()> {
        let mut running = self.running.lock().unwrap();
        if !*running {
            return Err(Error::from_reason("Renderer is not running"));
        }
        *running = false;

        // TODO: Signal GPUI to stop

        Ok(())
    }

    /// Check if the renderer is running
    #[napi]
    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
    }

    /// Get window dimensions
    #[napi]
    pub fn get_window_size(&self) -> Result<WindowSize> {
        // TODO: Get actual window size from GPUI
        Ok(WindowSize {
            width: 800.0,
            height: 600.0,
        })
    }

    /// Set window title
    #[napi]
    pub fn set_window_title(&self, title: String) -> Result<()> {
        // TODO: Set actual window title
        println!("GPUIX: set_window_title({})", title);
        Ok(())
    }

    /// Focus an element by ID
    #[napi]
    pub fn focus_element(&self, element_id: String) -> Result<()> {
        // TODO: Focus the element in GPUI
        println!("GPUIX: focus_element({})", element_id);
        Ok(())
    }

    /// Blur the currently focused element
    #[napi]
    pub fn blur(&self) -> Result<()> {
        // TODO: Blur in GPUI
        println!("GPUIX: blur()");
        Ok(())
    }
}

#[derive(Debug, Clone)]
#[napi(object)]
pub struct WindowSize {
    pub width: f64,
    pub height: f64,
}

/// Configuration for window creation
#[derive(Debug, Clone)]
#[napi(object)]
pub struct WindowOptions {
    pub title: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub min_width: Option<f64>,
    pub min_height: Option<f64>,
    pub resizable: Option<bool>,
    pub fullscreen: Option<bool>,
    pub transparent: Option<bool>,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: Some("GPUIX".to_string()),
            width: Some(800.0),
            height: Some(600.0),
            min_width: None,
            min_height: None,
            resizable: Some(true),
            fullscreen: Some(false),
            transparent: Some(false),
        }
    }
}

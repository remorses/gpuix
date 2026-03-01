/// NodeDispatcher — implements gpui::PlatformDispatcher for the Node.js environment.
///
/// Uses simple channels for background dispatch and a Vec queue for main-thread
/// runnables. No browser APIs needed — just native Rust primitives.
///
/// On macOS, gpui doesn't export PriorityQueueReceiver/PriorityQueueSender
/// (those are gated to windows/linux/wasm), so we use crossbeam or std channels.
///
/// Reference: gpui_web/src/dispatcher.rs (333 lines)
use gpui::{PlatformDispatcher, Priority, RunnableVariant, ThreadTaskTimings};
use parking_lot::Mutex;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const BACKGROUND_THREAD_COUNT: usize = 4;

/// A delayed runnable that fires after a deadline.
struct DelayedRunnable {
    deadline: Instant,
    runnable: RunnableVariant,
}

impl PartialEq for DelayedRunnable {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline
    }
}

impl Eq for DelayedRunnable {}

// BinaryHeap is a max-heap, so we reverse the ordering to get a min-heap (earliest deadline first)
impl PartialOrd for DelayedRunnable {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DelayedRunnable {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse: earliest deadline = highest priority
        other.deadline.cmp(&self.deadline)
    }
}

pub struct NodeDispatcher {
    main_thread_id: std::thread::ThreadId,
    background_sender: std::sync::mpsc::Sender<RunnableVariant>,
    main_thread_queue: Arc<Mutex<Vec<RunnableVariant>>>,
    delayed_queue: Arc<Mutex<BinaryHeap<DelayedRunnable>>>,
    _background_threads: Vec<std::thread::JoinHandle<()>>,
}

impl NodeDispatcher {
    pub fn new() -> Self {
        let (background_sender, background_receiver) =
            std::sync::mpsc::channel::<RunnableVariant>();
        let background_receiver = Arc::new(Mutex::new(background_receiver));

        let background_threads: Vec<_> = (0..BACKGROUND_THREAD_COUNT)
            .map(|i| {
                let receiver = background_receiver.clone();
                std::thread::Builder::new()
                    .name(format!("gpuix-bg-worker-{i}"))
                    .spawn(move || {
                        loop {
                            // Lock, recv, unlock — simple but effective
                            let runnable = {
                                let rx = receiver.lock();
                                rx.recv()
                            };
                            match runnable {
                                Ok(runnable) => {
                                    if !runnable.metadata().is_closed() {
                                        runnable.run();
                                    }
                                }
                                Err(_) => {
                                    log::info!(
                                        "gpuix-bg-worker-{i}: channel disconnected, exiting"
                                    );
                                    break;
                                }
                            }
                        }
                    })
                    .expect("failed to spawn background worker thread")
            })
            .collect();

        Self {
            main_thread_id: std::thread::current().id(),
            background_sender,
            main_thread_queue: Arc::new(Mutex::new(Vec::new())),
            delayed_queue: Arc::new(Mutex::new(BinaryHeap::new())),
            _background_threads: background_threads,
        }
    }

    /// Drain the main-thread queue. Called from tick() on the Node.js main thread.
    /// Runs all pending immediate runnables + any delayed runnables whose deadline has passed.
    pub fn drain_main_thread_queue(&self) {
        // 1. Drain immediate runnables
        let runnables: Vec<RunnableVariant> = {
            let mut queue = self.main_thread_queue.lock();
            queue.drain(..).collect()
        };
        for runnable in runnables {
            if !runnable.metadata().is_closed() {
                runnable.run();
            }
        }

        // 2. Drain delayed runnables whose time has passed
        let now = Instant::now();
        loop {
            let ready = {
                let mut delayed = self.delayed_queue.lock();
                match delayed.peek() {
                    Some(entry) if entry.deadline <= now => delayed.pop(),
                    _ => None,
                }
            };
            match ready {
                Some(entry) => {
                    if !entry.runnable.metadata().is_closed() {
                        entry.runnable.run();
                    }
                }
                None => break,
            }
        }
    }
}

impl PlatformDispatcher for NodeDispatcher {
    fn get_all_timings(&self) -> Vec<ThreadTaskTimings> {
        Vec::new()
    }

    fn get_current_thread_timings(&self) -> ThreadTaskTimings {
        ThreadTaskTimings {
            thread_name: None,
            thread_id: std::thread::current().id(),
            timings: Vec::new(),
            total_pushed: 0,
        }
    }

    fn is_main_thread(&self) -> bool {
        std::thread::current().id() == self.main_thread_id
    }

    fn dispatch(&self, runnable: RunnableVariant, _priority: Priority) {
        if let Err(e) = self.background_sender.send(runnable) {
            log::error!("NodeDispatcher::dispatch: failed to send to background queue: {e:?}");
        }
    }

    fn dispatch_on_main_thread(&self, runnable: RunnableVariant, _priority: Priority) {
        self.main_thread_queue.lock().push(runnable);
    }

    fn dispatch_after(&self, duration: Duration, runnable: RunnableVariant) {
        let deadline = Instant::now() + duration;
        self.delayed_queue
            .lock()
            .push(DelayedRunnable { deadline, runnable });
    }

    fn spawn_realtime(&self, function: Box<dyn FnOnce() + Send>) {
        // Execute immediately — realtime audio callbacks are rare in our use case
        function();
    }

    fn now(&self) -> Instant {
        Instant::now()
    }
}

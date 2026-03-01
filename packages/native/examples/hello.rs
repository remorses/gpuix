//! Simple GPUIX example - renders a basic UI using GPUI directly
//!
//! This example demonstrates what the React renderer produces internally.
//! Run with: cargo run -p gpuix --example hello

use gpui::{
    div, prelude::*, px, rgb, rgba, size, App, Application, Bounds, Context, Hsla, Window,
    WindowBounds, WindowOptions,
};

struct HelloWorld {
    click_count: u32,
    button_color: Hsla,
    boxes: [Hsla; 3],
}

impl HelloWorld {
    fn new() -> Self {
        Self {
            click_count: 0,
            button_color: rgb(0x89b4fa).into(),
            boxes: [
                gpui::red().into(),
                gpui::green().into(),
                gpui::blue().into(),
            ],
        }
    }

    fn cycle_button_color(&mut self) {
        self.click_count += 1;
        // Cycle through colors
        let colors = [
            rgb(0x89b4fa), // blue
            rgb(0xa6e3a1), // green
            rgb(0xf38ba8), // pink
            rgb(0xfab387), // peach
            rgb(0xcba6f7), // mauve
        ];
        self.button_color = colors[self.click_count as usize % colors.len()].into();
    }

    fn cycle_box_color(&mut self, index: usize) {
        let colors = [
            gpui::red(),
            gpui::green(),
            gpui::blue(),
            gpui::yellow(),
            rgb(0xcba6f7), // purple
            rgb(0xf38ba8), // pink
        ];
        let current = &mut self.boxes[index];
        // Find current color index and cycle to next
        for (i, c) in colors.iter().enumerate() {
            let c_hsla: Hsla = (*c).into();
            if (current.h - c_hsla.h).abs() < 0.01 {
                *current = colors[(i + 1) % colors.len()].into();
                return;
            }
        }
        *current = colors[0].into();
    }
}

impl Render for HelloWorld {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("root")
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .gap_4()
            .child(
                div()
                    .id("title")
                    .text_color(rgb(0xcdd6f4))
                    .text_3xl()
                    .child("Hello from GPUIX!"),
            )
            .child(
                div()
                    .id("subtitle")
                    .text_color(rgb(0xa6adc8))
                    .child("React → GPUI via napi-rs"),
            )
            .child(
                div()
                    .id("counter")
                    .text_color(rgb(0xf9e2af))
                    .text_xl()
                    .child(format!("Clicks: {}", self.click_count)),
            )
            .child(
                div()
                    .id("button")
                    .flex()
                    .p_4()
                    .bg(self.button_color)
                    .rounded_lg()
                    .cursor_pointer()
                    .child(
                        div()
                            .text_color(rgb(0x1e1e2e))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Click me!"),
                    )
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.cycle_button_color();
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("boxes")
                    .flex()
                    .gap_2()
                    .mt_4()
                    .child(
                        div()
                            .id("box-0")
                            .size_12()
                            .bg(self.boxes[0])
                            .rounded_md()
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.cycle_box_color(0);
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("box-1")
                            .size_12()
                            .bg(self.boxes[1])
                            .rounded_md()
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.cycle_box_color(1);
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("box-2")
                            .size_12()
                            .bg(self.boxes[2])
                            .rounded_md()
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.cycle_box_color(2);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .mt_4()
                    .text_color(rgb(0x6c7086))
                    .text_sm()
                    .child("Click the button or boxes to change colors!"),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(600.), px(450.)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| cx.new(|_| HelloWorld::new()),
        )
        .unwrap();

        cx.activate(true);
    });
}

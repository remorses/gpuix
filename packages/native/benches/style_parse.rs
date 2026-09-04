//! How long `setStyle` spends turning JSON into a `StyleDesc`.
//!
//! Every mutation from React lands here, so this is on the path of every
//! update. Run with `cargo bench --bench style_parse`.

use std::time::Instant;

use gpuix_native::style::StyleDesc;

const TYPICAL: &str = r##"{"display":"flex","flexDirection":"row","alignItems":"center","gap":8,"padding":12,"backgroundColor":"#1f2230","borderRadius":6,"borderWidth":1,"borderColor":"#5d6481","color":"#a4accd","fontSize":14}"##;

const WITH_VARIABLES: &str = r##"{"--brand":"#ff0000","--pad":"8px","display":"flex","padding":"var(--pad)","backgroundColor":"var(--brand)","borderRadius":6,"color":"#a4accd","fontSize":14}"##;

const SMALL: &str = r##"{"width":40,"height":40}"##;

fn bench(name: &str, json: &str) {
    let rounds = 200_000;
    // Warm up, so the first parse does not pay for lazy setup.
    for _ in 0..1_000 {
        std::hint::black_box(serde_json::from_str::<StyleDesc>(json).unwrap());
    }
    let start = Instant::now();
    for _ in 0..rounds {
        std::hint::black_box(serde_json::from_str::<StyleDesc>(json).unwrap());
    }
    let each = start.elapsed().as_secs_f64() / rounds as f64;
    println!("{name:16} {:>8.0} ns/parse", each * 1e9);
}

/// The same shape with no `flatten` and no `untagged`, as a floor to aim at.
///
/// It drops custom properties and takes numbers only, so it is not a working
/// `StyleDesc`. It exists to show what those two attributes cost.
mod floor {
    use serde::Deserialize;

    #[derive(Debug, Default, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Plain {
        pub display: Option<String>,
        pub flex_direction: Option<String>,
        pub align_items: Option<String>,
        pub gap: Option<f64>,
        pub padding: Option<f64>,
        pub background_color: Option<String>,
        pub border_radius: Option<f64>,
        pub border_width: Option<f64>,
        pub border_color: Option<String>,
        pub color: Option<String>,
        pub font_size: Option<f64>,
        pub width: Option<f64>,
        pub height: Option<f64>,
    }
}

fn bench_floor(name: &str, json: &str) {
    let rounds = 200_000;
    for _ in 0..1_000 {
        std::hint::black_box(serde_json::from_str::<floor::Plain>(json).unwrap());
    }
    let start = Instant::now();
    for _ in 0..rounds {
        std::hint::black_box(serde_json::from_str::<floor::Plain>(json).unwrap());
    }
    let each = start.elapsed().as_secs_f64() / rounds as f64;
    println!("{name:16} {:>8.0} ns/parse", each * 1e9);
}

/// How long an empty `StyleDesc` takes to build, with nothing parsed.
///
/// The reader starts from a default and fills in what it reads, so every parse
/// pays this. `StyleDesc` holds 78 fields, so the write is not free, and the
/// floor struct below has 13 and does not show it.
fn bench_empty() {
    let rounds = 200_000;
    for _ in 0..1_000 {
        std::hint::black_box(StyleDesc::default());
    }
    let start = Instant::now();
    for _ in 0..rounds {
        std::hint::black_box(StyleDesc::default());
    }
    let each = start.elapsed().as_secs_f64() / rounds as f64;
    println!("{:16} {:>8.0} ns/parse", "empty", each * 1e9);
}

/// The same read, into a box.
///
/// The tree keeps a pointer to a style, not the struct, so this is the read the
/// renderer actually calls.
fn bench_boxed(name: &str, json: &str) {
    let rounds = 200_000;
    for _ in 0..1_000 {
        std::hint::black_box(StyleDesc::from_json_boxed(json).unwrap());
    }
    let start = Instant::now();
    for _ in 0..rounds {
        std::hint::black_box(StyleDesc::from_json_boxed(json).unwrap());
    }
    let each = start.elapsed().as_secs_f64() / rounds as f64;
    println!("{name:16} {:>8.0} ns/parse", each * 1e9);
}

fn main() {
    bench("small", SMALL);
    bench("typical", TYPICAL);
    bench("with variables", WITH_VARIABLES);
    println!("--- into a box ---");
    bench_boxed("small", SMALL);
    bench_boxed("typical", TYPICAL);
    bench_boxed("with variables", WITH_VARIABLES);
    println!("--- no flatten, no untagged ---");
    bench_empty();
    bench_floor("small", SMALL);
    bench_floor("typical", TYPICAL);
}

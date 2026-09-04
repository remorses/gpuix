//! Native motion tracks resolved during GPUI rendering, outside React.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use serde::Deserialize;
use web_time::Instant;

use crate::style::StyleDesc;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MotionStyle {
    pub width: Option<f64>,
    pub height: Option<MotionHeight>,
    pub opacity: Option<f64>,
    /// A `filter: blur()` sigma in pixels, on the element and its children.
    pub blur: Option<f64>,
    pub top: Option<f64>,
    pub right: Option<f64>,
    pub bottom: Option<f64>,
    pub left: Option<f64>,
    pub border_radius: Option<f64>,
    pub corner_shape: Option<MotionShape>,
}

/// A `cornerShape` on the move: the curvature `K` of `superellipse(K)`.
///
/// Reads a number or any `<corner-shape-value>` text. Interpolates the way
/// CSS Borders 4 says, in the "half corner" space where `bevel` sits at 0.5,
/// so a `round` to `square` transition sweeps the visible shape at an even
/// pace instead of jumping at the infinite end.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(try_from = "MotionShapeWire")]
pub(crate) struct MotionShape(pub f64);

#[derive(Deserialize)]
#[serde(untagged)]
enum MotionShapeWire {
    Number(f64),
    Text(String),
}

impl TryFrom<MotionShapeWire> for MotionShape {
    type Error = String;

    fn try_from(wire: MotionShapeWire) -> Result<Self, String> {
        match wire {
            MotionShapeWire::Number(k) if !k.is_nan() => Ok(Self(k)),
            MotionShapeWire::Number(_) => Err("motion cornerShape must be a number".into()),
            MotionShapeWire::Text(text) => crate::style::corners::shape(&text)
                .map(|k| Self(k as f64))
                .ok_or_else(|| format!("motion cornerShape {text:?} is not a corner shape")),
        }
    }
}

impl MotionShape {
    /// Where the curve crosses the corner's diagonal, 0 at `notch`, 0.5 at
    /// `bevel`, 1 at `square`.
    fn half_corner(self) -> f64 {
        let k = self.0;
        if k.is_infinite() {
            return if k > 0.0 { 1.0 } else { 0.0 };
        }
        let convex = 0.5f64.powf(1.0 / 2f64.powf(k.abs()));
        if k >= 0.0 {
            convex
        } else {
            1.0 - convex
        }
    }

    fn from_half_corner(h: f64) -> Self {
        if h >= 1.0 {
            return Self(f64::INFINITY);
        }
        if h <= 0.0 {
            return Self(f64::NEG_INFINITY);
        }
        let (convex, sign) = if h >= 0.5 { (h, 1.0) } else { (1.0 - h, -1.0) };
        Self(sign * (0.5f64.ln() / convex.ln()).log2())
    }

    fn mix(self, to: Self, progress: f64) -> Self {
        Self::from_half_corner(mix(self.half_corner(), to.half_corner(), progress))
    }

    /// The value as `StyleDesc` text.
    fn css(self) -> String {
        match self.0 {
            k if k == f64::INFINITY => "square".to_string(),
            k if k == f64::NEG_INFINITY => "notch".to_string(),
            k => format!("superellipse({k})"),
        }
    }
}

/// A `height`, as a number of pixels plus a share of the height the content
/// takes.
///
/// CSS Values 5 calls an interpolation with a keyword at one end an
/// `interpolate-size`. `auto` has no number until layout runs, so it stays a
/// share here and the element that owns the height multiplies it out.
///
/// Both parts are needed because a frame part way between `auto` and a length
/// is part of each. Half way from `0` to `auto` is half the content, and
/// retargeting there has to start from that, which one number cannot hold.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(from = "HeightWire")]
pub(crate) struct MotionHeight {
    pixels: f64,
    content: f64,
}

/// What a `height` looks like on the wire: a number of pixels or `"auto"`.
///
/// A motion description parses once per change, so the buffering an untagged
/// enum does costs nothing here, unlike the 36 fields that read `Numeric`.
#[derive(Deserialize)]
#[serde(untagged)]
enum HeightWire {
    Pixels(f64),
    Keyword(HeightKeyword),
}

#[derive(Deserialize)]
enum HeightKeyword {
    #[serde(rename = "auto")]
    Auto,
}

impl From<HeightWire> for MotionHeight {
    fn from(wire: HeightWire) -> Self {
        match wire {
            HeightWire::Pixels(value) => Self::pixels(value),
            HeightWire::Keyword(HeightKeyword::Auto) => Self::content(),
        }
    }
}

impl MotionHeight {
    fn pixels(value: f64) -> Self {
        Self {
            pixels: value,
            content: 0.0,
        }
    }

    /// `auto`, the whole height the content takes.
    fn content() -> Self {
        Self {
            pixels: 0.0,
            content: 1.0,
        }
    }

    /// Whether this needs the height of the content before it is a number.
    pub(crate) fn needs_content(self) -> bool {
        self.content != 0.0
    }

    /// This as a number, or `None` while it still needs the content.
    pub(crate) fn length(self) -> Option<f64> {
        (!self.needs_content()).then_some(self.pixels)
    }

    fn mix(self, to: Self, progress: f64) -> Self {
        Self {
            pixels: mix(self.pixels, to.pixels, progress),
            content: mix(self.content, to.content, progress),
        }
    }

    /// The height this means, given the height the content takes.
    pub(crate) fn resolve(self, content: f64) -> f64 {
        // An easing that overshoots can carry a collapse below zero, and CSS
        // has no negative `height`.
        (self.pixels + self.content * content).max(0.0)
    }
}

/// One step of a linear interpolation.
pub(crate) fn mix(from: f64, to: f64, progress: f64) -> f64 {
    from + (to - from) * progress
}

impl MotionStyle {
    fn interpolate(self, target: Self, progress: f64) -> Self {
        fn value(from: Option<f64>, to: Option<f64>, progress: f64) -> Option<f64> {
            to.map(|to| mix(from.unwrap_or(to), to, progress))
        }

        Self {
            width: value(self.width, target.width, progress),
            // `auto` interpolates the same way as a length, because both ends
            // are pixels plus a share of the content.
            height: target
                .height
                .map(|to| self.height.unwrap_or(to).mix(to, progress)),
            opacity: value(self.opacity, target.opacity, progress),
            blur: value(self.blur, target.blur, progress),
            top: value(self.top, target.top, progress),
            right: value(self.right, target.right, progress),
            bottom: value(self.bottom, target.bottom, progress),
            left: value(self.left, target.left, progress),
            border_radius: value(self.border_radius, target.border_radius, progress),
            corner_shape: target
                .corner_shape
                .map(|to| self.corner_shape.unwrap_or(to).mix(to, progress)),
        }
    }

    pub(crate) fn apply_to(self, style: &mut StyleDesc) {
        if let Some(value) = self.width {
            style.width = Some(value.into());
        }
        // A height that still needs the content belongs to `AutoHeight`, which
        // reads it from the frame rather than from the style.
        if let Some(height) = self.height.and_then(MotionHeight::length) {
            style.height = Some(height.into());
        }
        if let Some(value) = self.opacity {
            style.opacity = Some(value.into());
        }
        if let Some(value) = self.blur {
            style.filter = Some(format!("blur({value}px)"));
        }
        if let Some(value) = self.top {
            style.top = Some(value.into());
        }
        if let Some(value) = self.right {
            style.right = Some(value.into());
        }
        if let Some(value) = self.bottom {
            style.bottom = Some(value.into());
        }
        if let Some(value) = self.left {
            style.left = Some(value.into());
        }
        if let Some(value) = self.border_radius {
            style.border_radius = Some(value.into());
        }
        if let Some(shape) = self.corner_shape {
            style.corner_shape = Some(shape.css());
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum MotionInitial {
    Disabled(bool),
    Style(MotionStyle),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub(crate) enum MotionEase {
    Name(String),
    CubicBezier([f64; 4]),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct MotionTransition {
    #[serde(default = "default_duration")]
    duration: f64,
    #[serde(default)]
    delay: f64,
    #[serde(default = "default_ease")]
    ease: MotionEase,
}

impl Default for MotionTransition {
    fn default() -> Self {
        Self {
            duration: default_duration(),
            delay: 0.0,
            ease: default_ease(),
        }
    }
}

fn default_duration() -> f64 {
    0.3
}

fn default_ease() -> MotionEase {
    MotionEase::Name("easeOut".to_string())
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct MotionDescription {
    #[serde(default)]
    initial: Option<MotionInitial>,
    animate: MotionStyle,
    #[serde(default)]
    transition: MotionTransition,
}

/// The height the content took, as the element that measures it reports it.
///
/// `AutoHeight` writes here during layout and the state reads it at the start
/// of the next frame. It is shared because the measure closure outlives the
/// frame that built it.
#[derive(Clone, Debug, Default)]
pub(crate) struct ContentHeight(Rc<Cell<Option<f64>>>);

impl ContentHeight {
    pub(crate) fn report(&self, height: f64) {
        self.0.set(Some(height));
    }

    fn get(&self) -> Option<f64> {
        self.0.get()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MotionFrame {
    pub style: MotionStyle,
    pub active: bool,
    /// The content height this frame's `height` resolves against while the
    /// animation runs. `None` before anything was measured.
    pub content: Option<f64>,
    /// Where the element that measures the content reports what it found.
    pub measured: ContentHeight,
}

impl MotionFrame {
    /// The `height` for this frame when it still needs the height the content
    /// takes. `AutoHeight` measures that, so the style sink leaves it alone.
    pub(crate) fn measured_height(&self) -> Option<MotionHeight> {
        self.style.height.filter(|height| height.needs_content())
    }

    /// A frame a view transition composes for the arriving element of a pair.
    /// It carries the opacity and the blur of this animation frame. The
    /// transition element applies the movement at paint.
    pub(crate) fn view_transition_frame(opacity: Option<f64>, blur: Option<f64>) -> Self {
        Self {
            style: MotionStyle {
                opacity,
                blur,
                ..MotionStyle::default()
            },
            active: true,
            content: None,
            measured: ContentHeight::default(),
        }
    }

    /// Fold a view transition into this frame. The transition owns the
    /// element while it runs, so its values replace the motion ones.
    pub(crate) fn with_view_transition(
        mut self,
        opacity: Option<f64>,
        blur: Option<f64>,
    ) -> Self {
        if opacity.is_some() {
            self.style.opacity = opacity;
        }
        if blur.is_some() {
            self.style.blur = blur;
        }
        self.active = true;
        self
    }
}

pub(crate) struct MotionState {
    source: serde_json::Value,
    from: MotionStyle,
    target: MotionStyle,
    transition: MotionTransition,
    started: Instant,
    valid: bool,
    /// The content height the last frame resolved against.
    content: Option<f64>,
    measured: ContentHeight,
}

impl MotionState {
    pub(crate) fn new(source: &serde_json::Value, now: Instant) -> Result<Self, String> {
        let description = parse_description(source)?;
        let from = match description.initial {
            Some(MotionInitial::Style(style)) => style,
            Some(MotionInitial::Disabled(false)) | None => description.animate,
            Some(MotionInitial::Disabled(true)) => unreachable!("validated above"),
        };

        Ok(Self {
            source: source.clone(),
            from,
            target: description.animate,
            transition: description.transition,
            started: now,
            valid: true,
            content: None,
            measured: ContentHeight::default(),
        })
    }

    pub(crate) fn invalid(source: &serde_json::Value, now: Instant) -> Self {
        Self {
            source: source.clone(),
            from: MotionStyle::default(),
            target: MotionStyle::default(),
            transition: MotionTransition::default(),
            started: now,
            valid: false,
            content: None,
            measured: ContentHeight::default(),
        }
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.valid
    }

    /// Bring the state up to date with `source` and with what the content
    /// measured, before this frame is read.
    pub(crate) fn sync(&mut self, source: &serde_json::Value, now: Instant) -> Result<(), String> {
        self.follow_content(now);
        if self.source == *source {
            return Ok(());
        }

        let description = match parse_description(source) {
            Ok(description) => description,
            Err(error) => {
                self.source = source.clone();
                self.valid = false;
                return Err(error);
            }
        };
        self.from = if self.valid {
            self.frame(now).style
        } else {
            match description.initial {
                Some(MotionInitial::Style(style)) => style,
                Some(MotionInitial::Disabled(false)) | None => description.animate,
                Some(MotionInitial::Disabled(true)) => unreachable!("validated above"),
            }
        };
        self.target = description.animate;
        self.transition = description.transition;
        self.started = now;
        self.source = source.clone();
        self.valid = true;
        Ok(())
    }

    /// Take in the height the content measured last frame.
    ///
    /// A `height` with `auto` at an end resolves against the content every
    /// frame, so content that grows while the animation runs moves the height
    /// with it, and the box jumps. When the measurement changes part way, the
    /// start is rewritten so the frame at this progress still lands on the
    /// height that was on screen, and the rest of the curve bends toward the
    /// new end. The clock keeps running, so the animation ends when it would
    /// have.
    fn follow_content(&mut self, now: Instant) {
        let measured = self.measured.get();
        if measured == self.content {
            return;
        }
        if let (Some(old), Some(new)) = (self.content, measured) {
            let (raw, progress) = self.progress(now);
            let ends = match (self.from.height, self.target.height) {
                (Some(from), Some(target)) if raw < 1.0 => Some((from, target)),
                _ => None,
            };
            if let Some((from, target)) =
                ends.filter(|(from, target)| from.needs_content() || target.needs_content())
            {
                let visible = from.mix(target, progress).resolve(old);
                let end = target.resolve(new);
                // The pixels a start needs so that mixing it toward `end` at
                // `progress` gives `visible`. It can go below zero, which only
                // means the curve was already past it.
                let start = (visible - progress * end) / (1.0 - progress);
                self.from.height = Some(MotionHeight::pixels(start));
            }
        }
        self.content = measured;
    }

    /// Where the transition is at `now`: the share of the duration that has
    /// passed, and the same share after easing.
    fn progress(&self, now: Instant) -> (f64, f64) {
        let delay = seconds(self.transition.delay);
        let duration = seconds(self.transition.duration);
        let elapsed = now.saturating_duration_since(self.started);
        let raw = if elapsed <= delay {
            0.0
        } else if duration.is_zero() {
            1.0
        } else {
            elapsed.saturating_sub(delay).as_secs_f64() / duration.as_secs_f64()
        };
        (raw, ease(raw.clamp(0.0, 1.0), &self.transition.ease))
    }

    pub(crate) fn frame(&self, now: Instant) -> MotionFrame {
        let (raw, progress) = self.progress(now);
        let active = self.from != self.target && raw < 1.0;

        MotionFrame {
            style: self.from.interpolate(self.target, progress),
            active,
            content: self.content,
            measured: self.measured.clone(),
        }
    }

    /// The frame at `progress`, for an animation a scroll timeline drives.
    /// The share comes from the scroll offset instead of the clock, and the
    /// transition's ease still bends it. Duration and delay play no part,
    /// and the frame asks for no animation frames: a scroll repaints anyway.
    pub(crate) fn frame_at(&self, progress: f64) -> MotionFrame {
        let eased = ease(progress.clamp(0.0, 1.0), &self.transition.ease);
        MotionFrame {
            style: self.from.interpolate(self.target, eased),
            active: false,
            content: self.content,
            measured: self.measured.clone(),
        }
    }
}

fn parse_description(source: &serde_json::Value) -> Result<MotionDescription, String> {
    let description: MotionDescription =
        serde_json::from_value(source.clone()).map_err(|error| error.to_string())?;

    if matches!(description.initial, Some(MotionInitial::Disabled(true))) {
        return Err("motion initial only accepts false or a style object".to_string());
    }
    validate_style(&description.animate)?;
    if let Some(MotionInitial::Style(initial)) = &description.initial {
        validate_style(initial)?;
    }
    validate_seconds(description.transition.duration, "duration")?;
    validate_seconds(description.transition.delay, "delay")?;
    validate_ease(&description.transition.ease)?;
    Ok(description)
}

fn validate_style(style: &MotionStyle) -> Result<(), String> {
    for (name, value) in [
        ("width", style.width),
        ("height", style.height.map(|height| height.pixels)),
        ("opacity", style.opacity),
        ("blur", style.blur),
        ("top", style.top),
        ("right", style.right),
        ("bottom", style.bottom),
        ("left", style.left),
        ("borderRadius", style.border_radius),
    ] {
        if value.is_some_and(|value| !value.is_finite() || value.abs() > f32::MAX as f64) {
            return Err(format!("motion {name} must fit a finite 32-bit float"));
        }
    }
    if style.width.is_some_and(|value| value < 0.0)
        || style.height.is_some_and(|height| height.pixels < 0.0)
        || style.border_radius.is_some_and(|value| value < 0.0)
        || style.blur.is_some_and(|value| value < 0.0)
    {
        return Err("motion sizes, borderRadius and blur must be non-negative".to_string());
    }
    if style
        .opacity
        .is_some_and(|value| !(0.0..=1.0).contains(&value))
    {
        return Err("motion opacity must be between 0 and 1".to_string());
    }
    Ok(())
}

fn validate_seconds(value: f64, name: &str) -> Result<(), String> {
    if !value.is_finite() || value < 0.0 || Duration::try_from_secs_f64(value).is_err() {
        return Err(format!(
            "motion {name} must be a supported finite non-negative number"
        ));
    }
    Ok(())
}

pub(crate) fn validate_ease(ease: &MotionEase) -> Result<(), String> {
    match ease {
        MotionEase::Name(name)
            if matches!(
                name.as_str(),
                "linear" | "ease" | "easeIn" | "easeOut" | "easeInOut"
            ) => {}
        MotionEase::Name(name) => return Err(format!("unknown motion easing: {name}")),
        MotionEase::CubicBezier([x1, y1, x2, y2]) => {
            if ![x1, y1, x2, y2].iter().all(|value| value.is_finite())
                || !(0.0..=1.0).contains(x1)
                || !(0.0..=1.0).contains(x2)
            {
                return Err(
                    "motion cubic bezier values must be finite and x values must be 0..1"
                        .to_string(),
                );
            }
        }
    }
    Ok(())
}

fn seconds(value: f64) -> Duration {
    Duration::try_from_secs_f64(value).expect("motion durations are validated when parsed")
}

pub(crate) fn ease(progress: f64, ease: &MotionEase) -> f64 {
    let curve = match ease {
        MotionEase::CubicBezier(curve) => *curve,
        MotionEase::Name(name) => match name.as_str() {
            "linear" => return progress,
            "easeIn" => [0.42, 0.0, 1.0, 1.0],
            "easeInOut" => [0.42, 0.0, 0.58, 1.0],
            "ease" => [0.25, 0.1, 0.25, 1.0],
            _ => [0.0, 0.0, 0.58, 1.0],
        },
    };
    cubic_bezier(progress, curve)
}

fn cubic_bezier(x: f64, [x1, y1, x2, y2]: [f64; 4]) -> f64 {
    fn sample(t: f64, a: f64, b: f64) -> f64 {
        let c = 3.0 * a;
        let b = 3.0 * (b - a) - c;
        let a = 1.0 - c - b;
        ((a * t + b) * t + c) * t
    }

    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..20 {
        let middle = (low + high) / 2.0;
        if sample(middle, x1, x2) < x {
            low = middle;
        } else {
            high = middle;
        }
    }
    sample((low + high) / 2.0, y1, y2).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_shapes_move_through_half_corner_space() {
        let round = MotionShape(1.0);
        let square = MotionShape(f64::INFINITY);
        let notch = MotionShape(f64::NEG_INFINITY);
        for k in [-3.0, -1.0, 0.0, 0.5, 1.0, 2.0, 4.0] {
            let back = MotionShape::from_half_corner(MotionShape(k).half_corner()).0;
            assert!((back - k).abs() < 1e-9, "{k} came back as {back}");
        }
        let close = |a: MotionShape, b: MotionShape| a == b || (a.0 - b.0).abs() < 1e-9;
        assert!(close(round.mix(square, 0.0), round));
        assert!(close(round.mix(square, 1.0), square));
        assert_eq!(MotionShape(0.0).half_corner(), 0.5);
        // Half way from round to square sits between the two, not at infinity.
        let mid = round.mix(square, 0.5).0;
        assert!(mid > 1.0 && mid.is_finite(), "{mid}");
        assert!(close(notch.mix(square, 0.5), MotionShape(0.0)));

        let started = Instant::now();
        let spec = serde_json::json!({
            "initial": { "cornerShape": "notch" },
            "animate": { "cornerShape": "square" },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let state = MotionState::new(&spec, started).unwrap();
        let frame = state.frame(started + Duration::from_millis(500));
        let mut style = StyleDesc::default();
        frame.style.apply_to(&mut style);
        assert_eq!(style.corner_shape.as_deref(), Some("superellipse(0)"));
        let bad = serde_json::json!({ "animate": { "cornerShape": "oval" } });
        assert!(MotionState::new(&bad, started).is_err());
    }

    #[test]
    fn interpolates_and_retargets_from_the_visible_value() {
        let started = Instant::now();
        let initial = serde_json::json!({
            "initial": { "width": 0.0 },
            "animate": { "width": 100.0 },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let mut state = MotionState::new(&initial, started).unwrap();

        let middle = state.frame(started + Duration::from_millis(500));
        assert_eq!(middle.style.width, Some(50.0));
        assert!(middle.active);

        let reversed = serde_json::json!({
            "initial": false,
            "animate": { "width": 0.0 },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let reversed_at = started + Duration::from_millis(500);
        state.sync(&reversed, reversed_at).unwrap();
        assert_eq!(state.frame(reversed_at).style.width, Some(50.0));
        assert_eq!(
            state
                .frame(reversed_at + Duration::from_millis(500))
                .style
                .width,
            Some(25.0)
        );
    }

    #[test]
    fn blur_interpolates_and_folds_into_the_filter() {
        let started = Instant::now();
        let spec = serde_json::json!({
            "initial": { "blur": 0.0 },
            "animate": { "blur": 8.0 },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let state = MotionState::new(&spec, started).unwrap();
        let frame = state.frame(started + Duration::from_millis(500));
        assert_eq!(frame.style.blur, Some(4.0));
        let mut style = StyleDesc::default();
        frame.style.apply_to(&mut style);
        assert_eq!(style.filter.as_deref(), Some("blur(4px)"));

        let bad = serde_json::json!({ "animate": { "blur": -1.0 }, "transition": {} });
        assert!(MotionState::new(&bad, started).is_err());
    }

    #[test]
    fn disabled_initial_state_starts_at_the_target() {
        let now = Instant::now();
        let description = serde_json::json!({
            "initial": false,
            "animate": { "width": 260.0 },
            "transition": { "duration": 0.2 }
        });
        let frame = MotionState::new(&description, now).unwrap().frame(now);

        assert_eq!(frame.style.width, Some(260.0));
        assert!(!frame.active);
    }

    #[test]
    fn rejects_unsafe_numbers_and_invalid_initial_booleans() {
        let now = Instant::now();
        for description in [
            serde_json::json!({ "animate": { "width": 1e300 }, "transition": {} }),
            serde_json::json!({ "animate": { "opacity": 2.0 }, "transition": {} }),
            serde_json::json!({ "animate": {}, "transition": { "duration": 1e300 } }),
            serde_json::json!({ "initial": true, "animate": {}, "transition": {} }),
        ] {
            assert!(MotionState::new(&description, now).is_err());
        }
    }

    /// The height the frame reports, given a content height of 200.
    fn at(frame: MotionFrame) -> Option<f64> {
        frame.style.height.map(|height| height.resolve(200.0))
    }

    #[test]
    fn opens_toward_the_height_the_content_takes() {
        let started = Instant::now();
        let description = serde_json::json!({
            "initial": { "height": 0.0 },
            "animate": { "height": "auto" },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let state = MotionState::new(&description, started).unwrap();

        assert_eq!(at(state.frame(started)), Some(0.0));
        assert_eq!(
            at(state.frame(started + Duration::from_millis(500))),
            Some(100.0)
        );
        assert_eq!(
            at(state.frame(started + Duration::from_secs(1))),
            Some(200.0)
        );
    }

    #[test]
    fn collapses_from_the_height_auto_reached() {
        let started = Instant::now();
        let opening = serde_json::json!({
            "initial": { "height": 0.0 },
            "animate": { "height": "auto" },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let mut state = MotionState::new(&opening, started).unwrap();

        let settled = started + Duration::from_secs(1);
        let closing = serde_json::json!({
            "initial": false,
            "animate": { "height": 0.0 },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        state.sync(&closing, settled).unwrap();

        assert_eq!(at(state.frame(settled)), Some(200.0));
        assert_eq!(
            at(state.frame(settled + Duration::from_millis(500))),
            Some(100.0)
        );
        assert_eq!(at(state.frame(settled + Duration::from_secs(1))), Some(0.0));
    }

    #[test]
    fn reverses_mid_open_without_a_jump() {
        let started = Instant::now();
        let opening = serde_json::json!({
            "initial": { "height": 0.0 },
            "animate": { "height": "auto" },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let mut state = MotionState::new(&opening, started).unwrap();

        let turned = started + Duration::from_millis(500);
        let closing = serde_json::json!({
            "initial": false,
            "animate": { "height": 0.0 },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        state.sync(&closing, turned).unwrap();

        // Half open when it turned, so the collapse starts at half.
        assert_eq!(at(state.frame(turned)), Some(100.0));
        assert_eq!(
            at(state.frame(turned + Duration::from_millis(500))),
            Some(50.0)
        );
    }

    #[test]
    fn bends_toward_content_that_grows_while_it_opens() {
        let started = Instant::now();
        let description = serde_json::json!({
            "initial": { "height": 0.0 },
            "animate": { "height": "auto" },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let mut state = MotionState::new(&description, started).unwrap();

        // The first frame measured the content at 100.
        state.frame(started).measured.report(100.0);
        let half = started + Duration::from_millis(500);
        state.sync(&description, half).unwrap();
        let frame = state.frame(half);
        assert_eq!(frame.content, Some(100.0));
        assert_eq!(frame.style.height.map(|h| h.resolve(100.0)), Some(50.0));

        // The content grew to 200 during that frame.
        frame.measured.report(200.0);
        state.sync(&description, half).unwrap();
        let frame = state.frame(half);
        assert_eq!(frame.content, Some(200.0));
        // Still at 50 with the new content, where the last frame was.
        assert_eq!(frame.style.height.map(|h| h.resolve(200.0)), Some(50.0));
        // And it ends on the new content, at the time it would have.
        let later = state.frame(started + Duration::from_millis(750));
        assert_eq!(later.style.height.map(|h| h.resolve(200.0)), Some(125.0));
        let done = state.frame(started + Duration::from_secs(1));
        assert_eq!(done.style.height.map(|h| h.resolve(200.0)), Some(200.0));
    }

    #[test]
    fn rejects_a_height_keyword_it_cannot_measure() {
        let now = Instant::now();
        let description = serde_json::json!({
            "animate": { "height": "min-content" },
            "transition": {}
        });
        assert!(MotionState::new(&description, now).is_err());
    }

    #[test]
    fn finishes_at_the_exact_target() {
        let started = Instant::now();
        let description = serde_json::json!({
            "initial": { "width": 0.0 },
            "animate": { "width": 100.0 },
            "transition": { "duration": 0.2, "ease": "linear" }
        });
        let state = MotionState::new(&description, started).unwrap();
        let frame = state.frame(started + Duration::from_millis(200));

        assert_eq!(frame.style.width, Some(100.0));
        assert!(!frame.active);
    }
}

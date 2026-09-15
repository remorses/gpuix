//! Native motion tracks resolved during GPUI rendering, outside React.
//! Tween (duration/ease) plus spring (stiffness/damping/mass/velocity) integrators.

use std::time::Duration;

use serde::Deserialize;
use web_time::Instant;

use crate::style::{DimensionValue, StyleDesc};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MotionStyle {
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub opacity: Option<f64>,
    pub top: Option<f64>,
    pub right: Option<f64>,
    pub bottom: Option<f64>,
    pub left: Option<f64>,
    pub border_radius: Option<f64>,
}

impl MotionStyle {
    fn interpolate(self, target: Self, progress: f64) -> Self {
        fn value(from: Option<f64>, to: Option<f64>, progress: f64) -> Option<f64> {
            to.map(|to| from.unwrap_or(to) + (to - from.unwrap_or(to)) * progress)
        }

        Self {
            width: value(self.width, target.width, progress),
            height: value(self.height, target.height, progress),
            opacity: value(self.opacity, target.opacity, progress),
            top: value(self.top, target.top, progress),
            right: value(self.right, target.right, progress),
            bottom: value(self.bottom, target.bottom, progress),
            left: value(self.left, target.left, progress),
            border_radius: value(self.border_radius, target.border_radius, progress),
        }
    }

    fn channels(self) -> [( &'static str, Option<f64>); 8] {
        [
            ("width", self.width),
            ("height", self.height),
            ("opacity", self.opacity),
            ("top", self.top),
            ("right", self.right),
            ("bottom", self.bottom),
            ("left", self.left),
            ("borderRadius", self.border_radius),
        ]
    }

    fn set(&mut self, name: &str, value: f64) {
        match name {
            "width" => self.width = Some(value),
            "height" => self.height = Some(value),
            "opacity" => self.opacity = Some(value),
            "top" => self.top = Some(value),
            "right" => self.right = Some(value),
            "bottom" => self.bottom = Some(value),
            "left" => self.left = Some(value),
            "borderRadius" => self.border_radius = Some(value),
            _ => {}
        }
    }

    pub(crate) fn apply_to(self, style: &mut StyleDesc) {
        if let Some(value) = self.width {
            style.width = Some(DimensionValue::Pixels(value));
        }
        if let Some(value) = self.height {
            style.height = Some(DimensionValue::Pixels(value));
        }
        if let Some(value) = self.opacity {
            style.opacity = Some(value);
        }
        if let Some(value) = self.top {
            style.top = Some(value);
        }
        if let Some(value) = self.right {
            style.right = Some(value);
        }
        if let Some(value) = self.bottom {
            style.bottom = Some(value);
        }
        if let Some(value) = self.left {
            style.left = Some(value);
        }
        if let Some(value) = self.border_radius {
            style.border_radius = Some(value);
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
enum MotionEase {
    Name(String),
    CubicBezier([f64; 4]),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct TweenTransition {
    #[serde(default = "default_duration")]
    duration: f64,
    #[serde(default)]
    delay: f64,
    #[serde(default = "default_ease")]
    ease: MotionEase,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SpringTransition {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default = "default_stiffness")]
    stiffness: f64,
    #[serde(default = "default_damping")]
    damping: f64,
    #[serde(default = "default_mass")]
    mass: f64,
    #[serde(default)]
    velocity: f64,
    #[serde(default)]
    delay: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum MotionTransition {
    Spring(SpringTransition),
    Tween(TweenTransition),
}

impl Default for MotionTransition {
    fn default() -> Self {
        Self::Tween(TweenTransition {
            duration: default_duration(),
            delay: 0.0,
            ease: default_ease(),
        })
    }
}

fn default_duration() -> f64 {
    0.3
}

fn default_ease() -> MotionEase {
    MotionEase::Name("easeOut".to_string())
}

fn default_stiffness() -> f64 {
    36.0
}

fn default_damping() -> f64 {
    8.0
}

fn default_mass() -> f64 {
    1.2
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct MotionDescription {
    #[serde(default)]
    initial: Option<MotionInitial>,
    animate: MotionStyle,
    #[serde(default)]
    transition: MotionTransition,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MotionFrame {
    pub style: MotionStyle,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct SpringTrack {
    pos: f64,
    vel: f64,
}

pub(crate) struct MotionState {
    source: serde_json::Value,
    from: MotionStyle,
    target: MotionStyle,
    current: MotionStyle,
    transition: MotionTransition,
    started: Instant,
    last: Instant,
    springs: [SpringTrack; 8],
    valid: bool,
}

const CHANNELS: [&str; 8] = [
    "width",
    "height",
    "opacity",
    "top",
    "right",
    "bottom",
    "left",
    "borderRadius",
];

impl MotionState {
    pub(crate) fn new(source: &serde_json::Value, now: Instant) -> Result<Self, String> {
        let description = parse_description(source)?;
        let from = match description.initial {
            Some(MotionInitial::Style(style)) => style,
            Some(MotionInitial::Disabled(false)) | None => description.animate,
            Some(MotionInitial::Disabled(true)) => unreachable!("validated above"),
        };
        let kick = spring_kick(&description.transition);
        Ok(Self {
            source: source.clone(),
            from,
            target: description.animate,
            current: from,
            transition: description.transition,
            started: now,
            last: now,
            springs: seed_springs(from, description.animate, kick),
            valid: true,
        })
    }

    pub(crate) fn invalid(source: &serde_json::Value, now: Instant) -> Self {
        Self {
            source: source.clone(),
            from: MotionStyle::default(),
            target: MotionStyle::default(),
            current: MotionStyle::default(),
            transition: MotionTransition::default(),
            started: now,
            last: now,
            springs: [SpringTrack::default(); 8],
            valid: false,
        }
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.valid
    }

    pub(crate) fn sync(&mut self, source: &serde_json::Value, now: Instant) -> Result<(), String> {
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
        let visible = if self.valid {
            self.frame(now).style
        } else {
            match description.initial {
                Some(MotionInitial::Style(style)) => style,
                Some(MotionInitial::Disabled(false)) | None => description.animate,
                Some(MotionInitial::Disabled(true)) => unreachable!("validated above"),
            }
        };
        self.from = visible;
        self.current = visible;
        self.target = description.animate;
        let kick = spring_kick(&description.transition);
        if matches!(description.transition, MotionTransition::Spring(_)) {
            // Keep velocity; retarget in place so overshoot carries.
            for (index, name) in CHANNELS.iter().enumerate() {
                let pos = channel(visible, name).unwrap_or_else(|| channel(description.animate, name).unwrap_or(0.0));
                self.springs[index].pos = pos;
                if self.springs[index].vel.abs() < f64::EPSILON {
                    self.springs[index].vel = kick;
                }
            }
        } else {
            self.springs = seed_springs(visible, description.animate, kick);
        }
        self.transition = description.transition;
        self.started = now;
        self.last = now;
        self.source = source.clone();
        self.valid = true;
        Ok(())
    }

    pub(crate) fn frame(&mut self, now: Instant) -> MotionFrame {
        match &self.transition {
            MotionTransition::Spring(spring) => self.spring_frame(now, spring.clone()),
            MotionTransition::Tween(tween) => self.tween_frame(now, tween.clone()),
        }
    }

    fn tween_frame(&self, now: Instant, tween: TweenTransition) -> MotionFrame {
        let delay = seconds(tween.delay);
        let duration = seconds(tween.duration);
        let elapsed = now.saturating_duration_since(self.started);
        let raw = if elapsed <= delay {
            0.0
        } else if duration.is_zero() {
            1.0
        } else {
            elapsed.saturating_sub(delay).as_secs_f64() / duration.as_secs_f64()
        };
        let active = self.from != self.target && raw < 1.0;
        let progress = ease(raw.clamp(0.0, 1.0), &tween.ease);
        MotionFrame {
            style: self.from.interpolate(self.target, progress),
            active,
        }
    }

    fn spring_frame(&mut self, now: Instant, spring: SpringTransition) -> MotionFrame {
        let delay = seconds(spring.delay);
        if now.saturating_duration_since(self.started) < delay {
            self.last = now;
            return MotionFrame {
                style: self.current,
                active: self.from != self.target,
            };
        }
        let mut dt = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        if dt <= 0.0 {
            return MotionFrame {
                style: self.current,
                active: !settled(&self.springs, self.target),
            };
        }
        dt = dt.min(0.032);
        let mut style = self.current;
        let mut active = false;
        for (index, name) in CHANNELS.iter().enumerate() {
            let Some(target) = channel(self.target, name) else {
                continue;
            };
            let rest = if *name == "opacity" { 0.002 } else { 0.05 };
            let next = step_spring(self.springs[index], target, dt, spring.stiffness, spring.damping, spring.mass, rest);
            self.springs[index] = next;
            style.set(name, next.pos);
            if (next.pos - target).abs() > rest || next.vel.abs() > rest {
                active = true;
            }
        }
        self.current = style;
        MotionFrame { style, active }
    }
}

fn channel(style: MotionStyle, name: &str) -> Option<f64> {
    match name {
        "width" => style.width,
        "height" => style.height,
        "opacity" => style.opacity,
        "top" => style.top,
        "right" => style.right,
        "bottom" => style.bottom,
        "left" => style.left,
        "borderRadius" => style.border_radius,
        _ => None,
    }
}

fn spring_kick(transition: &MotionTransition) -> f64 {
    match transition {
        MotionTransition::Spring(spring) => spring.velocity,
        MotionTransition::Tween(_) => 0.0,
    }
}

fn seed_springs(from: MotionStyle, target: MotionStyle, kick: f64) -> [SpringTrack; 8] {
    let mut tracks = [SpringTrack::default(); 8];
    for (index, name) in CHANNELS.iter().enumerate() {
        let pos = channel(from, name).or_else(|| channel(target, name)).unwrap_or(0.0);
        tracks[index] = SpringTrack { pos, vel: kick };
    }
    tracks
}

fn settled(tracks: &[SpringTrack; 8], target: MotionStyle) -> bool {
    for (index, name) in CHANNELS.iter().enumerate() {
        let Some(to) = channel(target, name) else {
            continue;
        };
        if (tracks[index].pos - to).abs() > 0.05 || tracks[index].vel.abs() > 0.05 {
            return false;
        }
    }
    true
}

fn step_spring(track: SpringTrack, target: f64, dt: f64, stiffness: f64, damping: f64, mass: f64, rest: f64) -> SpringTrack {
    let mass = mass.max(0.001);
    let x = track.pos - target;
    let accel = (-stiffness * x - damping * track.vel) / mass;
    let vel = track.vel + accel * dt;
    let pos = track.pos + vel * dt;
    if (pos - target).abs() < rest && vel.abs() < rest {
        SpringTrack { pos: target, vel: 0.0 }
    } else {
        SpringTrack { pos, vel }
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
    match &description.transition {
        MotionTransition::Tween(tween) => {
            validate_seconds(tween.duration, "duration")?;
            validate_seconds(tween.delay, "delay")?;
            validate_ease(&tween.ease)?;
        }
        MotionTransition::Spring(spring) => {
            if spring.kind != "spring" {
                return Err(format!("unknown motion type: {}", spring.kind));
            }
            validate_positive(spring.stiffness, "stiffness")?;
            validate_positive(spring.damping, "damping")?;
            validate_positive(spring.mass, "mass")?;
            validate_seconds(spring.delay, "delay")?;
            if !spring.velocity.is_finite() {
                return Err("motion velocity must be finite".to_string());
            }
        }
    }
    Ok(description)
}

fn validate_style(style: &MotionStyle) -> Result<(), String> {
    for (name, value) in style.channels() {
        if value.is_some_and(|value| !value.is_finite() || value.abs() > f32::MAX as f64) {
            return Err(format!("motion {name} must fit a finite 32-bit float"));
        }
    }
    if style.width.is_some_and(|value| value < 0.0)
        || style.height.is_some_and(|value| value < 0.0)
        || style.border_radius.is_some_and(|value| value < 0.0)
    {
        return Err("motion sizes and borderRadius must be non-negative".to_string());
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

fn validate_positive(value: f64, name: &str) -> Result<(), String> {
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("motion {name} must be a finite number greater than 0"));
    }
    Ok(())
}

fn validate_ease(ease: &MotionEase) -> Result<(), String> {
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

fn ease(progress: f64, ease: &MotionEase) -> f64 {
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

    #[test]
    fn finishes_at_the_exact_target() {
        let started = Instant::now();
        let description = serde_json::json!({
            "initial": { "width": 0.0 },
            "animate": { "width": 100.0 },
            "transition": { "duration": 0.2, "ease": "linear" }
        });
        let mut state = MotionState::new(&description, started).unwrap();
        let frame = state.frame(started + Duration::from_millis(200));

        assert_eq!(frame.style.width, Some(100.0));
        assert!(!frame.active);
    }

    #[test]
    fn spring_overshoots_then_settles() {
        let started = Instant::now();
        let description = serde_json::json!({
            "initial": { "width": 0.0 },
            "animate": { "width": 100.0 },
            "transition": { "type": "spring", "stiffness": 40.0, "damping": 6.0, "mass": 1.0 }
        });
        let mut state = MotionState::new(&description, started).unwrap();
        let mut max_width = 0.0;
        let mut now = started;
        for _ in 0..120 {
            now += Duration::from_millis(8);
            let frame = state.frame(now);
            max_width = max_width.max(frame.style.width.unwrap_or(0.0));
        }
        assert!(max_width > 100.0, "gelatinous spring must overshoot, got {max_width}");
        let settled = state.frame(now);
        assert!((settled.style.width.unwrap_or(0.0) - 100.0).abs() < 1.0);
        assert!(!settled.active);
    }
}

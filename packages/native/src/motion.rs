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
    fn with_fallback(self, fallback: Self) -> Self {
        Self {
            width: self.width.or(fallback.width),
            height: self.height.or(fallback.height),
            opacity: self.opacity.or(fallback.opacity),
            top: self.top.or(fallback.top),
            right: self.right.or(fallback.right),
            bottom: self.bottom.or(fallback.bottom),
            left: self.left.or(fallback.left),
            border_radius: self.border_radius.or(fallback.border_radius),
        }
    }

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

    fn channels(self) -> [(&'static str, Option<f64>); 8] {
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
#[serde(rename_all = "camelCase")]
struct MotionDescription {
    #[serde(default)]
    generation: u64,
    #[serde(default)]
    is_exit: bool,
    #[serde(default)]
    initial: Option<MotionInitial>,
    animate: MotionStyle,
    #[serde(default)]
    transition: MotionTransition,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MotionFrame {
    pub active: bool,
    pub just_settled: bool,
    pub generation: u64,
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
    /// Last integrated spring style; tweens sample from `from`/`target` instead.
    current: MotionStyle,
    transition: MotionTransition,
    started: Instant,
    last: Instant,
    springs: [SpringTrack; 8],
    valid: bool,
    needs_settle: bool,
    generation: u64,
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

        let target = description.animate.with_fallback(from);
        let kick = spring_kick(&description.transition);
        Ok(Self {
            source: source.clone(),
            from,
            target,
            current: from,
            transition: description.transition,
            started: now,
            last: now,
            springs: seed_springs(from, target, kick),
            valid: true,
            needs_settle: description.is_exit || from != target,
            generation: description.generation,
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
            needs_settle: source_is_exit(source),
            generation: source_generation(source),
        }
    }

    pub(crate) fn sync(&mut self, source: &serde_json::Value, now: Instant) -> Result<(), String> {
        if self.source == *source {
            return Ok(());
        }

        let previous_generation = self.generation;
        let description = match parse_description(source) {
            Ok(description) => description,
            Err(error) => {
                self.source = source.clone();
                self.valid = false;
                self.generation = source_generation(source);
                self.needs_settle =
                    source_is_exit(source) || self.generation != previous_generation;
                return Err(error);
            }
        };
        self.from = if self.valid {
            self.visible_style(now).unwrap_or(self.target)
        } else {
            match description.initial {
                Some(MotionInitial::Style(style)) => style,
                Some(MotionInitial::Disabled(false)) | None => description.animate,
                Some(MotionInitial::Disabled(true)) => unreachable!("validated above"),
            }
        };
        self.target = description.animate.with_fallback(self.from);
        self.current = self.from;
        let kick = spring_kick(&description.transition);
        if matches!(description.transition, MotionTransition::Spring(_)) {
            // Keep velocity; retarget in place so overshoot carries.
            for (index, name) in CHANNELS.iter().enumerate() {
                let pos = channel(self.from, name)
                    .or_else(|| channel(self.target, name))
                    .unwrap_or(0.0);
                self.springs[index].pos = pos;
                if self.springs[index].vel.abs() < f64::EPSILON {
                    self.springs[index].vel = kick;
                }
            }
        } else {
            self.springs = seed_springs(self.from, self.target, kick);
        }
        self.transition = description.transition;
        self.started = now;
        self.last = now;
        self.source = source.clone();
        self.valid = true;
        self.generation = description.generation;
        self.needs_settle = description.is_exit
            || self.generation != previous_generation
            || self.from != self.target;
        Ok(())
    }

    pub(crate) fn visible_style(&self, now: Instant) -> Option<MotionStyle> {
        self.valid.then(|| self.sample(now).0)
    }

    fn sample(&self, now: Instant) -> (MotionStyle, bool) {
        match &self.transition {
            // Springs are integrated in `frame`; the visible value is the last step.
            MotionTransition::Spring(_) => {
                (self.current, !settled(&self.springs, self.target))
            }
            MotionTransition::Tween(tween) => self.sample_tween(now, tween),
        }
    }

    fn sample_tween(&self, now: Instant, tween: &TweenTransition) -> (MotionStyle, bool) {
        let delay = seconds(tween.delay);
        let duration = seconds(tween.duration);
        let elapsed = now.saturating_duration_since(self.started);
        let raw = if duration.is_zero() {
            if elapsed < delay {
                0.0
            } else {
                1.0
            }
        } else if elapsed <= delay {
            0.0
        } else {
            elapsed.saturating_sub(delay).as_secs_f64() / duration.as_secs_f64()
        };
        let active = self.from != self.target && raw < 1.0;
        let progress = ease(raw.clamp(0.0, 1.0), &tween.ease);
        (self.from.interpolate(self.target, progress), active)
    }

    pub(crate) fn frame(&mut self, now: Instant) -> MotionFrame {
        let active = if !self.valid {
            false
        } else if let MotionTransition::Spring(spring) = &self.transition {
            let spring = spring.clone();
            self.step_springs(now, &spring)
        } else {
            self.sample(now).1
        };
        if active {
            self.needs_settle = true;
        }
        let just_settled = self.needs_settle && !active;
        if just_settled {
            self.needs_settle = false;
        }
        MotionFrame {
            active,
            just_settled,
            generation: self.generation,
        }
    }

    /// Advance every targeted spring channel to `now` and return whether any is still moving.
    fn step_springs(&mut self, now: Instant, spring: &SpringTransition) -> bool {
        let delay = seconds(spring.delay);
        if now.saturating_duration_since(self.started) < delay {
            self.last = now;
            return self.from != self.target;
        }
        let dt = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        if dt <= 0.0 {
            return !settled(&self.springs, self.target);
        }
        let dt = dt.min(0.032);
        let mut style = self.current;
        let mut active = false;
        for (index, name) in CHANNELS.iter().enumerate() {
            let Some(target) = channel(self.target, name) else {
                continue;
            };
            let rest = if *name == "opacity" { 0.002 } else { 0.05 };
            let next = step_spring(
                self.springs[index],
                target,
                dt,
                spring.stiffness,
                spring.damping,
                spring.mass,
                rest,
            );
            self.springs[index] = next;
            style.set(name, next.pos);
            if (next.pos - target).abs() > rest || next.vel.abs() > rest {
                active = true;
            }
        }
        self.current = style;
        active
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
        let pos = channel(from, name)
            .or_else(|| channel(target, name))
            .unwrap_or(0.0);
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

fn step_spring(
    track: SpringTrack,
    target: f64,
    dt: f64,
    stiffness: f64,
    damping: f64,
    mass: f64,
    rest: f64,
) -> SpringTrack {
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

fn source_generation(source: &serde_json::Value) -> u64 {
    source
        .get("generation")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default()
}

fn source_is_exit(source: &serde_json::Value) -> bool {
    source
        .get("isExit")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_default()
}

fn parse_description(source: &serde_json::Value) -> Result<MotionDescription, String> {
    let mut description: MotionDescription =
        serde_json::from_value(source.clone()).map_err(|error| error.to_string())?;
    // `type` makes any transition match the untagged Spring variant first; an
    // explicit `type: "tween"` is a tween.
    if let MotionTransition::Spring(spring) = &description.transition {
        if spring.kind == "tween" {
            let tween = source
                .get("transition")
                .cloned()
                .unwrap_or_default();
            description.transition = MotionTransition::Tween(
                serde_json::from_value(tween).map_err(|error| error.to_string())?,
            );
        }
    }

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
        return Err(format!(
            "motion {name} must be a finite number greater than 0"
        ));
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
        assert_eq!(
            state
                .visible_style(started + Duration::from_millis(500))
                .unwrap()
                .width,
            Some(50.0)
        );
        assert!(middle.active);
        assert!(!middle.just_settled);

        let reversed = serde_json::json!({
            "initial": false,
            "animate": { "width": 0.0 },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let reversed_at = started + Duration::from_millis(500);
        state.sync(&reversed, reversed_at).unwrap();
        assert_eq!(state.visible_style(reversed_at).unwrap().width, Some(50.0));
        assert_eq!(
            state
                .visible_style(reversed_at + Duration::from_millis(500))
                .unwrap()
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
        let mut state = MotionState::new(&description, now).unwrap();
        let frame = state.frame(now);

        assert_eq!(state.visible_style(now).unwrap().width, Some(260.0));
        assert!(!frame.active);
        assert!(!frame.just_settled);
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

        assert_eq!(
            state
                .visible_style(started + Duration::from_millis(200))
                .unwrap()
                .width,
            Some(100.0)
        );
        assert!(!frame.active);
        assert!(frame.just_settled);
        assert!(
            !state
                .frame(started + Duration::from_millis(201))
                .just_settled
        );
    }

    #[test]
    fn retarget_with_zero_duration_settles_on_the_next_frame() {
        let started = Instant::now();
        let initial = serde_json::json!({
            "initial": false,
            "animate": { "opacity": 1.0 },
            "transition": { "duration": 0.2, "ease": "linear" }
        });
        let mut state = MotionState::new(&initial, started).unwrap();
        assert!(!state.frame(started).just_settled);

        let exit = serde_json::json!({
            "initial": false,
            "animate": { "opacity": 0.0 },
            "transition": { "duration": 0.0, "ease": "linear" }
        });
        state.sync(&exit, started).unwrap();
        let frame = state.frame(started);
        assert_eq!(state.visible_style(started).unwrap().opacity, Some(0.0));
        assert!(!frame.active);
        assert!(frame.just_settled);
    }

    #[test]
    fn a_new_generation_settles_when_the_target_already_matches() {
        let started = Instant::now();
        let initial = serde_json::json!({
            "generation": 1,
            "initial": false,
            "animate": { "opacity": 1.0 }
        });
        let mut state = MotionState::new(&initial, started).unwrap();

        let exit = serde_json::json!({
            "generation": 2,
            "initial": false,
            "animate": { "opacity": 1.0 }
        });
        state.sync(&exit, started).unwrap();

        assert!(state.frame(started).just_settled);
        assert!(!state.frame(started).just_settled);
    }

    #[test]
    fn retarget_keeps_values_omitted_from_the_new_target() {
        let started = Instant::now();
        let initial = serde_json::json!({
            "generation": 1,
            "initial": false,
            "animate": { "width": 100.0, "opacity": 1.0 },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let mut state = MotionState::new(&initial, started).unwrap();

        let exit = serde_json::json!({
            "generation": 2,
            "initial": false,
            "animate": { "opacity": 0.0 },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        state.sync(&exit, started).unwrap();

        state.frame(started + Duration::from_millis(500));
        let style = state
            .visible_style(started + Duration::from_millis(500))
            .unwrap();
        assert_eq!(style.width, Some(100.0));
        assert_eq!(style.opacity, Some(0.5));
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
        let mut max_width = 0.0_f64;
        let mut saw_settle = false;
        let mut now = started;
        for _ in 0..500 {
            now += Duration::from_millis(8);
            let frame = state.frame(now);
            saw_settle |= frame.just_settled;
            max_width = max_width.max(state.visible_style(now).unwrap().width.unwrap_or(0.0));
        }
        assert!(max_width > 100.0, "gelatinous spring must overshoot, got {max_width}");
        now += Duration::from_millis(8);
        let frame = state.frame(now);
        saw_settle |= frame.just_settled;
        let width = state.visible_style(now).unwrap().width.unwrap_or(0.0);
        assert!((width - 100.0).abs() < 1.0, "spring must settle near target, got {width}");
        assert!(!frame.active);
        assert!(saw_settle, "spring must report just_settled once at rest");
    }

    #[test]
    fn explicit_tween_type_is_a_tween() {
        let started = Instant::now();
        let description = serde_json::json!({
            "initial": { "width": 0.0 },
            "animate": { "width": 100.0 },
            "transition": { "type": "tween", "duration": 1.0, "ease": "linear" }
        });
        let mut state = MotionState::new(&description, started).unwrap();
        state.frame(started + Duration::from_millis(500));
        assert_eq!(
            state
                .visible_style(started + Duration::from_millis(500))
                .unwrap()
                .width,
            Some(50.0)
        );
    }

    #[test]
    fn spring_rejects_non_positive_parameters() {
        let now = Instant::now();
        let description = serde_json::json!({
            "animate": { "width": 1.0 },
            "transition": { "type": "spring", "stiffness": 0.0 }
        });
        assert!(MotionState::new(&description, now).is_err());
    }
}

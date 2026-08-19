use std::time::{Duration, Instant};

use super::config::{OverlayConfig, OverlayInAnimation, OverlayOutAnimation};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverlayPhase {
    Entering,
    Stable,
    Fading,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transform {
    pub offset_x: f32,
    pub offset_y: f32,
    pub opacity: f32,
    pub scale: f32,
}

impl Transform {
    pub fn identity() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            opacity: 1.0,
            scale: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverlayTiming {
    pub enter: Duration,
    pub hold: Duration,
    pub exit: Duration,
}

impl OverlayTiming {
    pub fn from_config(config: &OverlayConfig) -> Self {
        Self {
            enter: Duration::from_millis(config.animation.enter_ms),
            hold: Duration::from_millis(config.display_duration_ms),
            exit: Duration::from_millis(config.animation.exit_ms),
        }
    }

    pub fn stable_at(&self, created_at: Instant) -> Instant {
        created_at + self.enter
    }

    pub fn exit_at(&self, created_at: Instant) -> Instant {
        created_at + self.enter + self.hold
    }

    pub fn remove_at(&self, created_at: Instant) -> Instant {
        created_at + self.enter + self.hold + self.exit
    }
}

pub fn phase_at(now: Instant, created_at: Instant, timing: &OverlayTiming) -> Option<OverlayPhase> {
    if now < timing.stable_at(created_at) {
        Some(OverlayPhase::Entering)
    } else if now < timing.exit_at(created_at) {
        Some(OverlayPhase::Stable)
    } else if now < timing.remove_at(created_at) {
        Some(OverlayPhase::Fading)
    } else {
        None
    }
}

pub fn progress(now: Instant, start: Instant, duration_ms: u64) -> f32 {
    if duration_ms == 0 {
        return 1.0;
    }
    let elapsed = now.saturating_duration_since(start).as_secs_f32() * 1000.0;
    (elapsed / duration_ms as f32).clamp(0.0, 1.0)
}

pub fn enter_transform(
    animation: OverlayInAnimation,
    progress: f32,
    distance: f32,
    start_scale: f32,
) -> Transform {
    let p = ease_out_cubic(progress);
    match animation {
        OverlayInAnimation::None => Transform::identity(),
        OverlayInAnimation::Fade => Transform {
            opacity: p,
            ..Transform::identity()
        },
        OverlayInAnimation::SlideUp => Transform {
            offset_y: distance * (1.0 - p),
            opacity: p,
            ..Transform::identity()
        },
        OverlayInAnimation::SlideDown => Transform {
            offset_y: -distance * (1.0 - p),
            opacity: p,
            ..Transform::identity()
        },
        OverlayInAnimation::SlideLeft => Transform {
            offset_x: distance * (1.0 - p),
            opacity: p,
            ..Transform::identity()
        },
        OverlayInAnimation::SlideRight => Transform {
            offset_x: -distance * (1.0 - p),
            opacity: p,
            ..Transform::identity()
        },
        OverlayInAnimation::Pop => Transform {
            opacity: p,
            scale: start_scale + (1.0 - start_scale) * p,
            ..Transform::identity()
        },
    }
}

pub fn exit_transform(animation: OverlayOutAnimation, progress: f32, distance: f32) -> Transform {
    let p = ease_in_cubic(progress);
    match animation {
        OverlayOutAnimation::None => Transform::identity(),
        OverlayOutAnimation::Fade => Transform {
            opacity: 1.0 - p,
            ..Transform::identity()
        },
        OverlayOutAnimation::SlideUp => Transform {
            offset_y: -distance * p,
            opacity: 1.0 - p,
            ..Transform::identity()
        },
        OverlayOutAnimation::SlideDown => Transform {
            offset_y: distance * p,
            opacity: 1.0 - p,
            ..Transform::identity()
        },
        OverlayOutAnimation::SlideLeft => Transform {
            offset_x: -distance * p,
            opacity: 1.0 - p,
            ..Transform::identity()
        },
        OverlayOutAnimation::SlideRight => Transform {
            offset_x: distance * p,
            opacity: 1.0 - p,
            ..Transform::identity()
        },
        OverlayOutAnimation::Shrink => Transform {
            opacity: 1.0 - p,
            scale: 1.0 - 0.08 * p,
            ..Transform::identity()
        },
    }
}

fn ease_out_cubic(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    1.0 - (1.0 - value).powi(3)
}

fn ease_in_cubic(value: f32) -> f32 {
    value.clamp(0.0, 1.0).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::config::OverlayAnimationConfig;

    #[test]
    fn timing_uses_configured_hold_time() {
        let config = OverlayConfig {
            display_duration_ms: 1_500,
            animation: OverlayAnimationConfig {
                enter_ms: 200,
                exit_ms: 300,
                ..OverlayAnimationConfig::default()
            },
            ..OverlayConfig::default()
        };
        let timing = OverlayTiming::from_config(&config);
        let t0 = Instant::now();
        assert_eq!(
            timing.remove_at(t0).duration_since(t0),
            Duration::from_millis(2_000)
        );
    }

    #[test]
    fn fade_enter_starts_transparent_and_ends_opaque() {
        let first = enter_transform(OverlayInAnimation::Fade, 0.0, 18.0, 0.96);
        let last = enter_transform(OverlayInAnimation::Fade, 1.0, 18.0, 0.96);
        assert_eq!(first.opacity, 0.0);
        assert_eq!(last.opacity, 1.0);
    }

    #[test]
    fn zero_duration_is_complete_without_division_by_zero() {
        let now = Instant::now();
        assert_eq!(progress(now, now, 0), 1.0);
    }
}

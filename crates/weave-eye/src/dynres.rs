use std::time::Duration;

const STEP: f32 = 0.05;
const DWELL_FRAMES: u32 = 8;
const COOLDOWN_FRAMES: u32 = 4;
const RAISE_BELOW: f32 = 0.85;
const AIM: f32 = 0.97;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicResolution {
    budget: Duration,
    min_scale: f32,
    max_scale: f32,
    scale: f32,
    calm: u32,
    cooldown: u32,
}

impl DynamicResolution {
    pub fn new(budget: Duration, min_scale: f32, max_scale: f32) -> Self {
        let max_scale = ((max_scale.clamp(0.25, 1.0) / STEP).floor() * STEP).max(STEP * 5.0);
        let min_scale = ((min_scale.clamp(0.25, max_scale) / STEP).ceil() * STEP).min(max_scale);
        Self {
            budget,
            min_scale,
            max_scale,
            scale: max_scale,
            calm: 0,
            cooldown: 0,
        }
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn budget(&self) -> Duration {
        self.budget
    }

    pub fn observe(&mut self, frame: Duration) -> f32 {
        let budget = self.budget.as_secs_f32();
        let time = frame.as_secs_f32();
        self.cooldown = self.cooldown.saturating_sub(1);
        if time > budget {
            self.calm = 0;
            let ideal = self.scale * (AIM * budget / time).sqrt();
            let lower = ((ideal / STEP).floor() * STEP).min(self.scale - STEP);
            self.scale = lower.max(self.min_scale);
            self.cooldown = COOLDOWN_FRAMES;
        } else if time < budget * RAISE_BELOW {
            self.calm += 1;
            if self.calm >= DWELL_FRAMES && self.cooldown == 0 && self.scale < self.max_scale {
                self.scale = (self.scale + STEP).min(self.max_scale);
                self.calm = 0;
                self.cooldown = COOLDOWN_FRAMES;
            }
        } else {
            self.calm = 0;
        }
        self.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_at(full_cost_ms: f32, scale: f32) -> Duration {
        Duration::from_secs_f32(full_cost_ms * scale * scale / 1000.0)
    }

    fn run(controller: &mut DynamicResolution, full_cost_ms: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|_| {
                let frame = frame_at(full_cost_ms, controller.scale());
                controller.observe(frame)
            })
            .collect()
    }

    fn budget_60hz() -> Duration {
        Duration::from_micros(16_667)
    }

    #[test]
    fn under_budget_stays_at_full_resolution() {
        let mut controller = DynamicResolution::new(budget_60hz(), 0.5, 1.0);
        let scales = run(&mut controller, 8.0, 100);
        assert!(scales.iter().all(|scale| *scale == 1.0));
    }

    #[test]
    fn one_heavy_frame_drops_resolution_immediately() {
        let mut controller = DynamicResolution::new(budget_60hz(), 0.5, 1.0);
        let scale = controller.observe(Duration::from_millis(33));
        assert!((0.5..1.0).contains(&scale), "scale {scale}");
    }

    #[test]
    fn never_goes_below_the_minimum() {
        let mut controller = DynamicResolution::new(budget_60hz(), 0.5, 1.0);
        let scales = run(&mut controller, 100.0, 100);
        assert!(scales.iter().all(|scale| *scale >= 0.5));
        assert_eq!(*scales.last().unwrap(), 0.5);
    }

    #[test]
    fn settles_near_the_budget_without_oscillating() {
        let mut controller = DynamicResolution::new(budget_60hz(), 0.5, 1.0);
        let scales = run(&mut controller, 25.0, 200);
        let tail = &scales[150..];
        assert!(tail.iter().all(|scale| (0.75..=0.85).contains(scale)));
        let changes = tail.windows(2).filter(|pair| pair[0] != pair[1]).count();
        assert_eq!(changes, 0, "tail {tail:?}");
    }

    #[test]
    fn recovers_to_full_resolution_when_load_drops() {
        let mut controller = DynamicResolution::new(budget_60hz(), 0.5, 1.0);
        run(&mut controller, 25.0, 100);
        assert!(controller.scale() < 1.0);
        let scales = run(&mut controller, 5.0, 100);
        assert_eq!(*scales.last().unwrap(), 1.0);
    }

    #[test]
    fn scale_always_sits_on_the_step_grid() {
        let mut controller = DynamicResolution::new(budget_60hz(), 0.5, 1.0);
        for scale in run(&mut controller, 19.0, 150) {
            let steps = scale / STEP;
            assert!((steps - steps.round()).abs() < 1e-4, "{scale}");
        }
    }
}

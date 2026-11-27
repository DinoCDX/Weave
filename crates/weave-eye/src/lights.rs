use glam::Vec3;

pub const MAX_DIRECTIONAL: usize = 4;
pub const MAX_LIGHTS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Light {
    Directional {
        direction: Vec3,
        color: Vec3,
        intensity: f32,
    },
    Point {
        position: Vec3,
        color: Vec3,
        intensity: f32,
        range: f32,
    },
    Spot {
        position: Vec3,
        direction: Vec3,
        color: Vec3,
        intensity: f32,
        range: f32,
        inner_degrees: f32,
        outer_degrees: f32,
    },
}

impl Default for Light {
    fn default() -> Self {
        Self::Directional {
            direction: Vec3::new(-0.4, -1.0, -0.6).normalize(),
            color: Vec3::ONE,
            intensity: 1.0,
        }
    }
}

impl Light {
    pub fn key(direction: Vec3, color: Vec3, intensity: f32) -> Self {
        Self::Directional {
            direction: direction.normalize_or_zero(),
            color,
            intensity,
        }
    }

    pub fn flood(position: Vec3, target: Vec3, color: Vec3, intensity: f32, range: f32) -> Self {
        Self::Spot {
            position,
            direction: (target - position).normalize_or_zero(),
            color,
            intensity,
            range,
            inner_degrees: 35.0,
            outer_degrees: 62.0,
        }
    }

    pub fn accent(position: Vec3, color: Vec3, intensity: f32, range: f32) -> Self {
        Self::Point {
            position,
            color,
            intensity,
            range,
        }
    }

    pub(crate) fn encode(&self) -> [f32; 16] {
        match *self {
            Self::Directional {
                direction,
                color,
                intensity,
            } => [
                0.0,
                0.0,
                0.0,
                0.0,
                direction.x,
                direction.y,
                direction.z,
                0.0,
                color.x,
                color.y,
                color.z,
                intensity,
                0.0,
                0.0,
                0.0,
                0.0,
            ],
            Self::Point {
                position,
                color,
                intensity,
                range,
            } => [
                position.x, position.y, position.z, 1.0, 0.0, 0.0, 0.0, 0.0, color.x, color.y,
                color.z, intensity, range, 0.0, 0.0, 0.0,
            ],
            Self::Spot {
                position,
                direction,
                color,
                intensity,
                range,
                inner_degrees,
                outer_degrees,
            } => [
                position.x,
                position.y,
                position.z,
                2.0,
                direction.x,
                direction.y,
                direction.z,
                0.0,
                color.x,
                color.y,
                color.z,
                intensity,
                range,
                inner_degrees.to_radians().cos(),
                outer_degrees.to_radians().cos(),
                0.0,
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directional_light_encodes_its_type_direction_and_colour() {
        let light = Light::Directional {
            direction: Vec3::new(0.0, -1.0, 0.0),
            color: Vec3::new(0.5, 0.25, 1.0),
            intensity: 2.0,
        };
        let e = light.encode();
        assert_eq!(e[3], 0.0, "type is directional");
        assert_eq!(&e[4..7], &[0.0, -1.0, 0.0]);
        assert_eq!(&e[8..12], &[0.5, 0.25, 1.0, 2.0]);
    }

    #[test]
    fn a_point_light_encodes_its_position_and_range() {
        let light = Light::Point {
            position: Vec3::new(1.0, 2.0, 3.0),
            color: Vec3::ONE,
            intensity: 4.0,
            range: 9.0,
        };
        let e = light.encode();
        assert_eq!(&e[0..4], &[1.0, 2.0, 3.0, 1.0]);
        assert_eq!(e[12], 9.0);
    }

    #[test]
    fn a_spot_light_stores_the_cosines_of_its_cone_angles() {
        let light = Light::Spot {
            position: Vec3::ZERO,
            direction: Vec3::NEG_Y,
            color: Vec3::ONE,
            intensity: 1.0,
            range: 10.0,
            inner_degrees: 0.0,
            outer_degrees: 60.0,
        };
        let e = light.encode();
        assert_eq!(e[3], 2.0, "type is spot");
        assert!(
            (e[13] - 1.0).abs() < 1e-6,
            "inner cone at 0 degrees has cosine 1"
        );
        assert!(
            (e[14] - 0.5).abs() < 1e-6,
            "outer cone at 60 degrees has cosine 0.5"
        );
    }

    #[test]
    fn the_default_light_is_a_directional_sun() {
        assert!(matches!(Light::default(), Light::Directional { .. }));
    }
}

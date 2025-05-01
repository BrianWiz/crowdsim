use bevy::{math::I16Vec2, prelude::*};

/// Maintains 2 decimal places of precision when converting to integer
pub fn quantize_vec3(position: Vec3) -> I16Vec2 {
    const SCALE: f32 = 100.0;
    I16Vec2::new((position.x * SCALE) as i16, (position.z * SCALE) as i16)
}

/// Maintains 2 decimal places of precision when converting to float
pub fn dequantize_vec3(position: I16Vec2) -> Vec3 {
    const SCALE: f32 = 100.0;
    Vec3::new(position.x as f32 / SCALE, 0.0, position.y as f32 / SCALE)
}

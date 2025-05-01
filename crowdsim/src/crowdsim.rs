use bevy::prelude::*;

use crate::PersonDensity;
use crate::PersonGoalPosition;
use crate::PersonPressure;
use crate::PersonVelocity;
use crate::spatial::*;
use crate::utils::*;

use crate::ARENA_SIZE;
use crate::PERSON_ACCELERATION;
use crate::PERSON_AVOIDANCE_STRENGTH;
use crate::PERSON_FRICTION;
use crate::PERSON_GOAL_DETECTION_RADIUS;
use crate::PERSON_MAX_SPEED;
use crate::PERSON_SIZE;
use crate::SIMULATION_COMPUTE_EVERY_N_FRAMES;
use crate::SIMULATION_PERSON_AVOIDANCE_RADIUS;
use crate::SIMULATION_PRESSURE_CONSTANT;
use crate::SIMULATION_SPEED;
use crate::SIMULATION_TARGET_DENSITY;

pub struct CrowdSimPlugin;

impl Plugin for CrowdSimPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                calculate_density_pressure,
                calculate_forces,
                handle_collisions,
                update_movement,
            ),
        );
    }
}

fn calculate_density_pressure(
    mut grid: ResMut<SpatialGrid>,
    mut query: Query<(&Transform, &mut PersonDensity, &mut PersonPressure)>,
    mut frame: Local<u32>,
) {
    *frame += 1;

    if *frame % SIMULATION_COMPUTE_EVERY_N_FRAMES == 0 {
        return;
    }

    for (transform, mut person_density, mut person_pressure) in query.iter_mut() {
        let position = transform.translation;
        let nearby_entities = grid.get_nearby_entities(position);

        let mut density = 0.0;

        for &(_, other_position) in nearby_entities {
            let distance = position.distance(other_position);

            if distance < SIMULATION_PERSON_AVOIDANCE_RADIUS {
                let factor = SIMULATION_PERSON_AVOIDANCE_RADIUS - distance;
                density += factor * factor;
            }
        }

        density *= 0.5;
        person_density.0 = density.max(0.01);
        person_pressure.0 =
            SIMULATION_PRESSURE_CONSTANT * (density - SIMULATION_TARGET_DENSITY).max(0.0);
    }
}

fn calculate_forces(
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(
        &Transform,
        &mut PersonVelocity,
        &mut PersonGoalPosition,
        &PersonPressure,
    )>,
    mut grid: ResMut<SpatialGrid>,
    mut frame: Local<u32>,
) {
    *frame += 1;

    if *frame % SIMULATION_COMPUTE_EVERY_N_FRAMES == 0 {
        return;
    }

    let dt = fixed_time.delta_secs() * SIMULATION_SPEED;

    for (transform, mut person_velocity, mut person_goal_position, person_pressure) in
        query.iter_mut()
    {
        let position = transform.translation;
        let nearby_entities = grid.get_nearby_entities(position);

        let mut pressure_force = Vec3::ZERO;
        let mut avoidance_force = Vec3::ZERO;
        let mut slide_force = Vec3::ZERO;
        person_velocity.1 = person_velocity.0; // marks previous velocity
        let mut velocity = dequantize_vec3(person_velocity.0);
        let last_velocity = velocity;

        for &(_, other_position) in nearby_entities {
            let offset = other_position - position;
            let distance = offset.length();

            if distance < SIMULATION_PERSON_AVOIDANCE_RADIUS && distance > 0.01 {
                let normalized_offset = offset / distance;
                let perpendicular_offset =
                    Vec3::new(normalized_offset.z, 0.0, -normalized_offset.x);
                let factor = 1.0 - distance / SIMULATION_PERSON_AVOIDANCE_RADIUS;

                pressure_force -= normalized_offset * person_pressure.0 * factor * factor * dt;

                if distance < SIMULATION_PERSON_AVOIDANCE_RADIUS {
                    avoidance_force -= normalized_offset
                        * (SIMULATION_PERSON_AVOIDANCE_RADIUS - distance)
                        * PERSON_AVOIDANCE_STRENGTH
                        * dt;
                    slide_force -= perpendicular_offset * velocity.dot(perpendicular_offset) * dt;
                }
            }
        }

        // Mark cell as dirty if velocity changed direction or magnitude significantly
        let velocity_angle_change = last_velocity.dot(velocity) < 0.0;
        let velocity_magnitude_change =
            (velocity.length() - last_velocity.length()).abs() > PERSON_MAX_SPEED * 0.5;
        if velocity_angle_change || velocity_magnitude_change {
            grid.mark_cell_dirty(position);
        }

        velocity.x += pressure_force.x + avoidance_force.x + slide_force.x;
        velocity.z += pressure_force.z + avoidance_force.z + slide_force.z;

        velocity.y = 0.0;

        let goal_position = dequantize_vec3(person_goal_position.0);
        let to_goal = goal_position - position;
        let distance_to_goal = to_goal.length();

        if distance_to_goal < PERSON_GOAL_DETECTION_RADIUS {
            person_goal_position.choose_new_goal_position();
        } else {
            // Still heading to goal
            let goal_direction = to_goal.normalize();
            let goal_force = goal_direction * PERSON_ACCELERATION * dt;

            velocity.x += goal_force.x;
            velocity.z += goal_force.z;
        }

        person_velocity.0 = quantize_vec3(velocity);
        person_goal_position.0 = quantize_vec3(goal_position);
    }
}

fn update_movement(
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(&mut Transform, &mut PersonVelocity)>,
) {
    let dt = fixed_time.delta_secs() * SIMULATION_SPEED;

    for (mut transform, mut person_velocity) in query.iter_mut() {
        let mut v = dequantize_vec3(person_velocity.0);

        // apply drag, we do this here because we want to apply it to the client as well
        v *= 1.0 - PERSON_FRICTION * dt;

        // if we are moving faster than the max speed, clamp it
        if v.length() > PERSON_MAX_SPEED {
            v = v.normalize() * PERSON_MAX_SPEED;
        }

        person_velocity.0 = quantize_vec3(v);

        let velocity = dequantize_vec3(person_velocity.0);
        transform.translation.x += velocity.x * dt;
        transform.translation.z += velocity.z * dt;

        let horizontal_speed = Vec3::new(velocity.x, 0.0, velocity.z).length();
        if horizontal_speed > 0.2 {
            let target = Quat::from_rotation_y((-velocity.z).atan2(velocity.x));
            transform.rotation = transform.rotation.slerp(target, 0.1);
        }
    }
}

fn handle_collisions(mut query: Query<(&mut Transform, &mut PersonVelocity)>) {
    let chunk_size = 250;
    let total_entities = query.iter().count();
    let chunks = (total_entities + chunk_size - 1) / chunk_size;

    for chunk_idx in 0..chunks {
        let start_idx = chunk_idx * chunk_size;
        let end_idx = (start_idx + chunk_size).min(total_entities);

        for (_, (mut transform, mut person_velocity)) in query
            .iter_mut()
            .enumerate()
            .skip(start_idx)
            .take(end_idx - start_idx)
        {
            let position = &mut transform.translation;

            let actual_boundary = ARENA_SIZE * 0.5 - PERSON_SIZE;
            let mut wall_normal = Vec3::ZERO;

            if position.x > actual_boundary {
                position.x = actual_boundary;
                wall_normal.x = -1.0;
            } else if position.x < -actual_boundary {
                position.x = -actual_boundary;
                wall_normal.x = 1.0;
            }

            if position.z > actual_boundary {
                position.z = actual_boundary;
                wall_normal.z = -1.0;
            } else if position.z < -actual_boundary {
                position.z = -actual_boundary;
                wall_normal.z = 1.0;
            }

            // if we hit a wall try and move away from it
            if wall_normal != Vec3::ZERO {
                let mut velocity = dequantize_vec3(person_velocity.0);
                let normal_component = velocity.dot(wall_normal);
                velocity -= wall_normal * normal_component;
                person_velocity.0 = quantize_vec3(velocity);
            }
        }
    }
}

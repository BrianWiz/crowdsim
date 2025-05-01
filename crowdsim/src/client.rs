use bevy::{pbr::NotShadowCaster, prelude::*};

use std::collections::HashMap;

use crate::FlyCamera;
use crate::IsClient;
use crate::Person;
use crate::PersonColor;
use crate::PersonDensity;
use crate::PersonGoalPosition;
use crate::PersonInitialStateSyncPoint;
use crate::PersonPressure;
use crate::PersonStateSyncPoint;
use crate::PersonStateSyncPointLastReceived;
use crate::PersonVelocity;
use crate::PersonVisuals;
use crate::PersonVisualsReference;
use crate::PersonsMesh;
use crate::ViewFrustumEvent;
use crate::utils::*;

use crate::SPATIALIZATION_CELL_SIZE;

pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (client_send_view_frustum.run_if(resource_exists::<IsClient>),),
        )
        .add_systems(
            Update,
            (
                camera_movement,
                visuals_interpolation_system,
                on_spawn_person_system,
                on_receive_person_state_sync_point.run_if(resource_exists::<IsClient>),
            ),
        );
    }
}

fn on_spawn_person_system(
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
    persons_mesh: Option<Res<PersonsMesh>>,
    people: Query<(Entity, &PersonColor, &PersonInitialStateSyncPoint), Added<Person>>,
) {
    if let Some(persons_mesh) = persons_mesh {
        for (person, person_color, sync_point) in people.iter() {
            let dequantized_position = dequantize_vec3(sync_point.position);

            let entity = commands
                .entity(person)
                .insert((
                    Transform::from_xyz(
                        dequantized_position.x,
                        dequantized_position.y,
                        dequantized_position.z,
                    ),
                    PersonDensity(sync_point.density),
                    PersonPressure(sync_point.pressure),
                    PersonVelocity(sync_point.velocity, sync_point.velocity),
                    PersonStateSyncPointLastReceived(0),
                ))
                .id();

            let visual_entity = commands
                .spawn((
                    PersonVisuals {
                        simulation_entity: entity,
                    },
                    Mesh3d::from(persons_mesh.0.clone()),
                    MeshMaterial3d::from(materials.add(StandardMaterial {
                        base_color: person_color.0,
                        ..default()
                    })),
                    Transform::from_xyz(
                        dequantized_position.x,
                        dequantized_position.y,
                        dequantized_position.z,
                    ),
                    NotShadowCaster,
                ))
                .id();

            commands
                .entity(entity)
                .insert(PersonVisualsReference(visual_entity));
        }
    }
}

fn on_receive_person_state_sync_point(
    fixed_time: Res<Time<Fixed>>,
    mut sync_point: Query<(
        &mut Transform,
        &mut PersonGoalPosition,
        &mut PersonDensity,
        &mut PersonPressure,
        &mut PersonVelocity,
        &mut PersonStateSyncPointLastReceived,
        &PersonStateSyncPoint,
    )>,
    mut visuals: Query<&mut PersonVisuals>,
) {
    //let nudge_time = replicon_client.stats().rtt as f32 * 0.5;
    let nudge_time = 0.0;

    let mut person_to_visuals = HashMap::new();
    for visuals in visuals.iter_mut() {
        person_to_visuals.insert(visuals.simulation_entity, visuals);
    }

    for (
        mut transform,
        mut person_goal_position,
        mut person_density,
        mut person_pressure,
        mut person_velocity,
        mut last_received,
        sync_point,
    ) in sync_point.iter_mut()
    {
        if sync_point.seq_num <= last_received.0 {
            continue;
        }

        let mut new_position = transform.translation;
        let mut new_rotation = transform.rotation;

        let mut received_position_update = false;

        if let Some(position) = sync_point.position {
            new_position = dequantize_vec3(position);
            received_position_update = true;
        }

        if let Some(rotation) = sync_point.rotation {
            new_rotation = Quat::from_rotation_y(rotation);
        }

        if let Some(velocity) = sync_point.velocity {
            person_velocity.1 = person_velocity.0;
            person_velocity.0 = velocity;
        }

        // Apply simple prediction if we received a position update
        // if received_position_update > 0 {
        //     let prediction_velocity = dequantize_vec3(person_velocity.0);
        //     let prediction_offset = prediction_velocity * nudge_time;
        //     new_position += prediction_offset;
        // }

        transform.translation = new_position;
        transform.rotation = new_rotation;

        if let Some(density) = sync_point.density {
            person_density.0 = density;
        }

        if let Some(pressure) = sync_point.pressure {
            person_pressure.0 = pressure;
        }

        last_received.0 = sync_point.seq_num;
    }
}

fn camera_movement(
    fixed_time: Res<Time<Fixed>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Transform, &FlyCamera)>,
) {
    for (mut transform, fly_camera) in query.iter_mut() {
        let dt = fixed_time.delta_secs() * fly_camera.speed;
        let mut move_direction = Vec3::ZERO;
        if keyboard_input.pressed(KeyCode::KeyW) {
            move_direction.z -= 1.0;
        }
        if keyboard_input.pressed(KeyCode::KeyS) {
            move_direction.z += 1.0;
        }
        if keyboard_input.pressed(KeyCode::KeyA) {
            move_direction.x -= 1.0;
        }
        if keyboard_input.pressed(KeyCode::KeyD) {
            move_direction.x += 1.0;
        }
        if keyboard_input.pressed(KeyCode::Space) {
            move_direction.y += 1.0;
        }
        if keyboard_input.pressed(KeyCode::ShiftLeft) {
            move_direction.y -= 1.0;
        }

        move_direction = move_direction.normalize_or_zero();
        transform.translation.x += move_direction.x * dt;
        transform.translation.z += move_direction.z * dt;
        transform.translation.y += move_direction.y * dt;
    }
}

fn visuals_interpolation_system(
    time: Res<Time>,
    people: Query<&Transform, With<Person>>,
    mut visuals: Query<(&mut Transform, &PersonVisuals), Without<Person>>,
) {
    let dt = time.delta_secs();

    for (mut transform, person_visuals) in visuals.iter_mut() {
        if let Ok(person_transform) = people.get(person_visuals.simulation_entity) {
            let distance = transform.translation.distance(person_transform.translation);
            if distance < SPATIALIZATION_CELL_SIZE * 0.5 {
                transform.translation = transform
                    .translation
                    .lerp(person_transform.translation, 6.0 * dt);
                transform.rotation = transform.rotation.lerp(person_transform.rotation, 6.0 * dt);
            } else {
                transform.translation = person_transform.translation;
                transform.rotation = person_transform.rotation;
            }
        }
    }
}

fn client_send_view_frustum(
    camera_query: Query<(&Transform, &PerspectiveProjection), With<Camera3d>>,
    mut view_frustum_events: EventWriter<ViewFrustumEvent>,
) {
    if let Ok((camera_transform, camera_projection)) = camera_query.get_single() {
        let camera_position = camera_transform.translation;
        let camera_forward = camera_transform.forward();

        let view_info = ViewFrustumEvent {
            position: camera_position,
            direction: camera_forward.into(),
            fov: camera_projection.fov,
        };

        view_frustum_events.send(view_info);
    }
}

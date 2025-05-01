use bevy::{math::I16Vec2, pbr::NotShadowCaster, prelude::*, utils::HashSet};
use bevy_replicon::prelude::*;

use rand::prelude::*;
use std::collections::HashMap;

use crate::{
    ARENA_SIZE, SPATIALIZATION_CELL_SIZE, IsServer, PERSON_COUNT, Person, PersonColor, PersonDensity,
    PersonGoalPosition, PersonInitialStateSyncPoint, PersonPressure, PersonStateSyncPoint,
    PersonVelocity, SPATIALIZATION_GRID_CELLS_PER_AXIS, ViewFrustum, ViewFrustumEvent,
    spatial::SpatialGrid, utils::quantize_vec3,
};

#[derive(Component)]
struct ServerPersonLastState {
    position: I16Vec2,
    rotation: f32,
    velocity: I16Vec2,
    density: f32,
    pressure: f32,
    goal_position: I16Vec2,
}

#[derive(Resource)]
pub struct SyncTracker {
    // Frame number when each cell was last synced
    cell_last_sync: Vec<u32>,
    // Whether each cell was visible in the previous frame
    previously_visible: Vec<bool>,
    current_frame: u32,
}

impl Default for SyncTracker {
    fn default() -> Self {
        let total_cells = SPATIALIZATION_GRID_CELLS_PER_AXIS * SPATIALIZATION_GRID_CELLS_PER_AXIS;
        Self {
            cell_last_sync: vec![0; total_cells],
            current_frame: 0,
            previously_visible: vec![false; total_cells],
        }
    }
}

#[derive(Component)]
struct VisibleCellMarker {
    cell_idx: usize,
}

#[derive(Resource)]
struct DebugVisualization {
    cell_entities: HashMap<usize, Entity>,
    active: bool,
}

impl Default for DebugVisualization {
    fn default() -> Self {
        Self {
            cell_entities: HashMap::new(),
            active: true,
        }
    }
}

pub struct ServerPlugin;

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SyncTracker::default())
            .insert_resource(DebugVisualization::default())
            .add_systems(
                FixedUpdate,
                (
                    rolling_sync_system.run_if(resource_exists::<IsServer>),
                    handle_client_view_frustum_event.run_if(resource_exists::<IsServer>),
                    visualize_visible_cells.run_if(resource_exists::<IsServer>),
                ),
            )
            .add_systems(Update, (toggle_debug_visualization,));
    }
}

pub fn server_spawn_people(commands: &mut Commands) {
    let mut rng = rand::rng();

    for _ in 0..PERSON_COUNT {
        let x = rng.random_range(-ARENA_SIZE * 0.5..ARENA_SIZE * 0.5);
        let z = rng.random_range(-ARENA_SIZE * 0.5..ARENA_SIZE * 0.5);
        let y = 0.0;

        let goal_x = -ARENA_SIZE * 0.5;
        let goal_z = -ARENA_SIZE * 0.5;
        let goal_y = 0.0;

        let velocity_x = rng.random_range(-1.0..1.0);
        let velocity_z = rng.random_range(-1.0..1.0);
        let velocity = Vec3::new(velocity_x, 0.0, velocity_z);

        commands.spawn((
            Transform::from_xyz(x, y, z).with_scale(Vec3::new(
                1.0,
                1.0 + rng.random_range(-0.05..0.05),
                1.0,
            )),
            Person,
            PersonStateSyncPoint {
                seq_num: 1,
                position: None,
                rotation: None,
                velocity: None,
                density: None,
                pressure: None,
                goal_position: None,
            },
            PersonInitialStateSyncPoint {
                position: quantize_vec3(Vec3::new(x, y, z)),
                rotation: 0.0,
                velocity: quantize_vec3(velocity),
                density: 0.0,
                pressure: 0.0,
                goal_position: quantize_vec3(Vec3::new(goal_x, goal_y, goal_z)),
            },
            PersonColor(Color::hsl(
                rng.random_range(0.0..360.0),
                rng.random_range(0.0..1.0),
                rng.random_range(0.0..1.0),
            )),
            ServerPersonLastState {
                position: I16Vec2::ZERO,
                rotation: 0.0,
                velocity: I16Vec2::ZERO,
                density: 0.0,
                pressure: 0.0,
                goal_position: I16Vec2::ZERO,
            },
            Replicated,
        ));
    }
}

fn rolling_sync_system(
    grid: Res<SpatialGrid>,
    mut tracker: ResMut<SyncTracker>,
    clients: Query<(Entity, &ViewFrustum)>,
    mut query: Query<(
        Entity,
        &Transform,
        &mut ServerPersonLastState,
        &mut PersonVelocity,
        &mut PersonDensity,
        &mut PersonPressure,
        &mut PersonGoalPosition,
        &mut PersonStateSyncPoint,
    )>,
) {
    tracker.current_frame += 1;

    const CELLS_PER_FRAME: usize = 5;
    const SYNC_INTERVAL_IN_FRUSTUM: u32 = 0;
    const SYNC_INTERVAL_OUTSIDE: u32 = 4;

    // Get a list of all view frustums from connected clients
    let view_frustums: Vec<_> = clients.iter().map(|(_, f)| f).collect();

    // Simple version: mark all cells as either in frustum or not
    let mut in_frustum_cells = vec![false; grid.cells.len()];

    // Very simple frustum check - only if we have clients
    if !view_frustums.is_empty() {
        // Process all cells, even empty ones
        for cell_idx in 0..grid.cells.len() {
            // Calculate the actual center position of this cell
            let cell_x = (cell_idx % SPATIALIZATION_GRID_CELLS_PER_AXIS) as f32;
            let cell_z = (cell_idx / SPATIALIZATION_GRID_CELLS_PER_AXIS) as f32;

            let cell_center_x = (cell_x + 0.5) * SPATIALIZATION_CELL_SIZE - ARENA_SIZE / 2.0;
            let cell_center_z = (cell_z + 0.5) * SPATIALIZATION_CELL_SIZE - ARENA_SIZE / 2.0;
            let cell_center = Vec3::new(cell_center_x, 0.0, cell_center_z);

            // Check against all frustums
            for frustum in &view_frustums {
                let camera_pos = frustum.position;

                // Use the exact direction vector from the camera
                let camera_dir = frustum.direction;

                // Calculate vector from camera to cell center
                let to_cell = cell_center - camera_pos;
                let distance = to_cell.length();

                // Skip cells that are too far away
                if distance > 200.0 {
                    continue;
                }

                // Check if the cell is within the view frustum
                let normalized_to_cell = to_cell.normalize();
                let dot = camera_dir.dot(normalized_to_cell);

                // Calculate the angle from the camera's forward direction
                let angle = dot.acos();

                // Check if the cell is within the field of view (half angle)
                // Add a buffer so things dont pop in and out of view
                let half_fov = frustum.fov * 0.5 + 1.0;

                if angle <= half_fov {
                    in_frustum_cells[cell_idx] = true;
                    break;
                }
            }
        }
    }

    let mut cells_to_sync = Vec::with_capacity(CELLS_PER_FRAME);

    // First, always add cells that are in the frustum and need syncing
    for cell_idx in 0..grid.cells.len() {
        if in_frustum_cells[cell_idx] {
            let is_newly_visible = !tracker.previously_visible[cell_idx];
            let time_since_sync = tracker.current_frame - tracker.cell_last_sync[cell_idx];

            if is_newly_visible || time_since_sync >= SYNC_INTERVAL_IN_FRUSTUM {
                cells_to_sync.push(cell_idx);
            }
        }
    }

    // Then add out-of-frustum cells only if we have space left
    // and they need periodic updates
    if cells_to_sync.len() < CELLS_PER_FRAME {
        for cell_idx in 0..grid.cells.len() {
            if cells_to_sync.contains(&cell_idx)
                || in_frustum_cells[cell_idx]
                || grid.cells[cell_idx].is_empty()
            {
                continue;
            }

            // For cells outside frustum, sync less frequently
            let time_since_sync = tracker.current_frame - tracker.cell_last_sync[cell_idx];
            if time_since_sync >= SYNC_INTERVAL_OUTSIDE {
                cells_to_sync.push(cell_idx);

                // Stop adding non-visible cells if we've reached our limit
                if cells_to_sync.len() >= CELLS_PER_FRAME {
                    break;
                }
            }
        }
    }

    // Process all the selected cells
    for &cell_idx in &cells_to_sync {
        tracker.cell_last_sync[cell_idx] = tracker.current_frame;

        for &(entity, _) in &grid.cells[cell_idx] {
            if let Ok((
                _,
                transform,
                mut server_person_last_state,
                person_velocity,
                person_density,
                person_pressure,
                person_goal_position,
                mut sync_point,
            )) = query.get_mut(entity)
            {
                // @todo-brian: need to figure out how to properly diff, the problem right now is the
                // client may not have a correct state, so sending a diff can cause a desync.
                //let force_update = in_frustum_cells[cell_idx] && !tracker.previously_visible[cell_idx];
                let force_update = true;
                let new_position = quantize_vec3(transform.translation);
                let new_rotation = transform.rotation.to_euler(EulerRot::YXZ).0;
                let new_velocity = person_velocity.0;
                let new_density = person_density.0;
                let new_pressure = person_pressure.0;
                let new_goal_position = person_goal_position.0;

                if force_update || new_position != server_person_last_state.position {
                    sync_point.position = Some(new_position);
                } else {
                    sync_point.position = None;
                }

                if force_update || new_rotation != server_person_last_state.rotation {
                    sync_point.rotation = Some(new_rotation);
                } else {
                    sync_point.rotation = None;
                }

                if force_update || new_velocity != server_person_last_state.velocity {
                    sync_point.velocity = Some(new_velocity);
                } else {
                    sync_point.velocity = None;
                }

                if force_update || new_density != server_person_last_state.density {
                    sync_point.density = Some(new_density);
                } else {
                    sync_point.density = None;
                }
                if force_update || new_pressure != server_person_last_state.pressure {
                    sync_point.pressure = Some(new_pressure);
                } else {
                    sync_point.pressure = None;
                }

                if force_update || new_goal_position != server_person_last_state.goal_position {
                    sync_point.goal_position = Some(new_goal_position);
                } else {
                    sync_point.goal_position = None;
                }

                // mark server person last state as updated
                server_person_last_state.position = new_position;
                server_person_last_state.rotation = new_rotation;
                server_person_last_state.velocity = new_velocity;
                server_person_last_state.density = new_density;
                server_person_last_state.pressure = new_pressure;
                server_person_last_state.goal_position = new_goal_position;

                if sync_point.position.is_some()
                    || sync_point.rotation.is_some()
                    || sync_point.velocity.is_some()
                    || sync_point.density.is_some()
                    || sync_point.pressure.is_some()
                    || sync_point.goal_position.is_some()
                {
                    sync_point.seq_num += 1;
                }
            }
        }
    }

    tracker.previously_visible = in_frustum_cells;
}

fn handle_client_view_frustum_event(
    mut commands: Commands,
    mut events: EventReader<FromClient<ViewFrustumEvent>>,
) {
    for event in events.read() {
        commands.entity(event.client_entity).insert(ViewFrustum {
            position: event.position,
            direction: event.direction,
            fov: event.fov,
        });
    }
}

fn toggle_debug_visualization(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut debug_vis: ResMut<DebugVisualization>,
) {
    if keyboard_input.just_pressed(KeyCode::KeyV) {
        debug_vis.active = !debug_vis.active;
        println!(
            "Debug visualization: {}",
            if debug_vis.active { "ON" } else { "OFF" }
        );
    }
}

fn visualize_visible_cells(
    mut commands: Commands,
    grid: Res<SpatialGrid>,
    tracker: Res<SyncTracker>,
    mut debug_vis: ResMut<DebugVisualization>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    existing_markers: Query<(Entity, &VisibleCellMarker)>,
) {
    if !debug_vis.active {
        if !debug_vis.cell_entities.is_empty() {
            for entity in debug_vis.cell_entities.values() {
                commands.entity(*entity).despawn();
            }
            debug_vis.cell_entities.clear();
        }
        return;
    }

    let mut visible_cells = HashSet::new();
    for (cell_idx, _) in grid.cells.iter().enumerate() {
        if tracker.previously_visible[cell_idx] {
            visible_cells.insert(cell_idx);
        }
    }

    let mut to_remove = Vec::new();
    for (entity, marker) in existing_markers.iter() {
        if !visible_cells.contains(&marker.cell_idx) {
            commands.entity(entity).despawn();
            to_remove.push(marker.cell_idx);
        }
    }

    for cell_idx in to_remove {
        debug_vis.cell_entities.remove(&cell_idx);
    }

    for cell_idx in visible_cells {
        if debug_vis.cell_entities.contains_key(&cell_idx) {
            continue;
        }

        let x = (cell_idx % SPATIALIZATION_GRID_CELLS_PER_AXIS) as f32;
        let z = (cell_idx / SPATIALIZATION_GRID_CELLS_PER_AXIS) as f32;

        let world_x = (x as f32 + 0.5) * SPATIALIZATION_CELL_SIZE - ARENA_SIZE / 2.0;
        let world_z = (z as f32 + 0.5) * SPATIALIZATION_CELL_SIZE - ARENA_SIZE / 2.0;

        let entity = commands
            .spawn((
                Mesh3d::from(meshes.add(Cuboid::new(SPATIALIZATION_CELL_SIZE, 1.0, SPATIALIZATION_CELL_SIZE))),
                MeshMaterial3d::from(materials.add(StandardMaterial {
                    base_color: Color::srgba(0.0, 1.0, 0.0, 0.3),
                    alpha_mode: AlphaMode::Blend,
                    ..default()
                })),
                Transform::from_xyz(world_x, 5.0, world_z),
                VisibleCellMarker { cell_idx },
                NotShadowCaster,
            ))
            .id();

        debug_vis.cell_entities.insert(cell_idx, entity);
    }
}

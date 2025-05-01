mod cli;
mod client;
mod crowdsim;
mod server;
mod spatial;
mod utils;

use bevy::{math::I16Vec2, prelude::*, winit::WinitSettings};
use bevy_replicon::{RepliconPlugins, prelude::*};
use bevy_replicon_renet::RepliconRenetPlugins;
use rand::prelude::*;
use serde::{Deserialize, Serialize};

use utils::quantize_vec3;

pub const ARENA_SIZE: f32 = 250.0;

pub const SPATIALIZATION_GRID_CELLS_PER_AXIS: usize = 50;
pub const SPATIALIZATION_CELL_SIZE: f32 = ARENA_SIZE / SPATIALIZATION_GRID_CELLS_PER_AXIS as f32;
pub const SPATIALIZATION_MAX_NEIGHBORS: usize = 200;

pub const PERSON_COUNT: usize = 5000;
pub const PERSON_GOAL_DETECTION_RADIUS: f32 = 20.0;
pub const PERSON_FRICTION: f32 = 0.75;
pub const PERSON_ACCELERATION: f32 = 2.5;
pub const PERSON_SIZE: f32 = 1.0;
pub const PERSON_MAX_SPEED: f32 = 2.0;
pub const PERSON_AVOIDANCE_STRENGTH: f32 = 1.0;

pub const SIMULATION_SPEED: f32 = 1.0;
pub const SIMULATION_PERSON_AVOIDANCE_RADIUS: f32 = PERSON_SIZE * 2.0;
pub const SIMULATION_PRESSURE_CONSTANT: f32 = 30.0;
pub const SIMULATION_TARGET_DENSITY: f32 = 20.0;
pub const SIMULATION_COMPUTE_EVERY_N_FRAMES: Option<u32> = None;

#[derive(Resource)]
pub struct IsServer;

#[derive(Resource)]
pub struct IsClient;

#[derive(Component, Serialize, Deserialize)]
pub struct Person;

#[derive(Component)]
pub struct PersonVelocity(I16Vec2, I16Vec2 /* Previous velocity */);

#[derive(Component)]
pub struct PersonGoalPosition(I16Vec2);

impl PersonGoalPosition {
    fn choose_new_goal_position(&mut self) {
        let mut rng = rand::rng();
        self.0 = quantize_vec3(Vec3::new(
            rng.random_range(-ARENA_SIZE * 0.5..ARENA_SIZE * 0.5),
            0.0,
            rng.random_range(-ARENA_SIZE * 0.5..ARENA_SIZE * 0.5),
        ));
    }
}

#[derive(Component, Serialize, Deserialize)]
pub struct PersonDensity(f32);

#[derive(Component, Serialize, Deserialize)]
pub struct PersonPressure(f32);

#[derive(Component, Serialize, Deserialize)]
pub struct PersonColor(Color);

#[derive(Component, Serialize, Deserialize)]
pub struct PersonStateSyncPoint {
    seq_num: u32,
    position: Option<I16Vec2>,
    rotation: Option<f32>,
    velocity: Option<I16Vec2>,
    density: Option<f32>,
    pressure: Option<f32>,
    goal_position: Option<I16Vec2>,
}

#[derive(Component, Serialize, Deserialize)]
pub struct PersonInitialStateSyncPoint {
    position: I16Vec2,
    rotation: f32,
    velocity: I16Vec2,
    density: f32,
    pressure: f32,
    goal_position: I16Vec2,
}

#[derive(Component)]
pub struct PersonStateSyncPointLastReceived(u32);

#[derive(Component)]
pub struct FlyCamera {
    speed: f32,
}

#[derive(Resource)]
pub struct PersonsMesh(Handle<Mesh>);

#[derive(Component)]
pub struct PersonVisuals {
    simulation_entity: Entity,
}

#[derive(Component)]
pub struct PersonVisualsReference(pub Entity);

#[derive(Component)]
pub struct ViewFrustum {
    position: Vec3,
    direction: Vec3,
    fov: f32,
}

#[derive(Event, Serialize, Deserialize, Clone)]
struct ViewFrustumEvent {
    position: Vec3,
    direction: Vec3,
    fov: f32,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(WinitSettings {
            focused_mode: bevy::winit::UpdateMode::Continuous,
            unfocused_mode: bevy::winit::UpdateMode::Continuous,
        })
        .add_systems(Startup, setup_system)
        .add_plugins((RepliconPlugins, RepliconRenetPlugins))
        .add_client_event::<ViewFrustumEvent>(Channel::Unordered)
        .replicate::<Person>()
        .replicate::<PersonStateSyncPoint>()
        .replicate::<PersonInitialStateSyncPoint>()
        .replicate::<PersonColor>()
        .add_plugins(crate::cli::CliPlugin)
        .add_plugins(crate::client::ClientPlugin)
        .add_plugins(crate::server::ServerPlugin)
        .add_plugins(crate::spatial::SpatialGridPlugin)
        .add_plugins(crate::crowdsim::CrowdSimPlugin)
        .run();
}

fn setup_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let person_mesh = meshes.add(Cuboid::new(PERSON_SIZE, PERSON_SIZE, PERSON_SIZE));
    commands.insert_resource(PersonsMesh(person_mesh));

    // Light
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 10000.0,
            ..default()
        },
        Transform::from_xyz(50.0, 100.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Add ambient light
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 0.3,
    });

    // Camera
    commands.spawn((
        Camera3d::default(),
        PerspectiveProjection {
            fov: 90.0f32.to_radians(),
            ..default()
        },
        FlyCamera { speed: 60.0 },
        Transform::from_xyz(0.0, 50.0, 50.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Floor
    commands.spawn((
        Mesh3d::from(meshes.add(Cuboid::new(ARENA_SIZE, 1.0, ARENA_SIZE))),
        MeshMaterial3d::from(materials.add(StandardMaterial {
            base_color: Color::srgb(0.1, 0.1, 0.1),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::from_xyz(0.0, -1.0, 0.0),
    ));
}

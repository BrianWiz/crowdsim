use bevy::prelude::*;

use crate::{
    ARENA_SIZE, PERSON_COUNT, Person, SPATIALIZATION_CELL_SIZE, SPATIALIZATION_GRID_CELLS_PER_AXIS,
    SPATIALIZATION_MAX_NEIGHBORS,
};

#[derive(Resource)]
pub struct SpatialGrid {
    pub cells: Vec<Vec<(Entity, Vec3)>>,
    pub nearby_cache: Vec<(Entity, Vec3)>,
    pub dirty_cells: Vec<bool>,
}

impl Default for SpatialGrid {
    fn default() -> Self {
        let total_cells = SPATIALIZATION_GRID_CELLS_PER_AXIS * SPATIALIZATION_GRID_CELLS_PER_AXIS;
        let mut cells = Vec::with_capacity(total_cells);

        for _ in 0..total_cells {
            cells.push(Vec::with_capacity(PERSON_COUNT));
        }

        Self {
            cells,
            nearby_cache: Vec::with_capacity(SPATIALIZATION_MAX_NEIGHBORS * 4),
            dirty_cells: vec![false; total_cells],
        }
    }
}

impl SpatialGrid {
    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.clear();
        }
    }

    pub fn position_to_cell_index(&self, position: Vec3) -> Option<usize> {
        let half_arena = ARENA_SIZE * 0.5;
        let x = ((position.x + half_arena) / SPATIALIZATION_CELL_SIZE) as isize;
        let z = ((position.z + half_arena) / SPATIALIZATION_CELL_SIZE) as isize;
        if x >= 0
            && x < SPATIALIZATION_GRID_CELLS_PER_AXIS as isize
            && z >= 0
            && z < SPATIALIZATION_GRID_CELLS_PER_AXIS as isize
        {
            Some((z as usize * SPATIALIZATION_GRID_CELLS_PER_AXIS) + x as usize)
        } else {
            None
        }
    }

    pub fn add_entity(&mut self, entity: Entity, position: Vec3) {
        if let Some(index) = self.position_to_cell_index(position) {
            if index < self.cells.len() {
                self.cells[index].push((entity, position));
            }
        }
    }

    pub fn get_nearby_entities(&mut self, position: Vec3) -> &[(Entity, Vec3)] {
        self.nearby_cache.clear();

        if let Some(cell_index) = self.position_to_cell_index(position) {
            let cell_x = cell_index % SPATIALIZATION_GRID_CELLS_PER_AXIS;
            let cell_z = cell_index / SPATIALIZATION_GRID_CELLS_PER_AXIS;

            // Check this cell and neighboring cells
            for z_offset in -1..=1 {
                let z = cell_z as isize + z_offset;
                if z < 0 || z >= SPATIALIZATION_GRID_CELLS_PER_AXIS as isize {
                    continue;
                }

                for x_offset in -1..=1 {
                    let x = cell_x as isize + x_offset;
                    if x < 0 || x >= SPATIALIZATION_GRID_CELLS_PER_AXIS as isize {
                        continue;
                    }

                    let neighbor_index =
                        (z as usize * SPATIALIZATION_GRID_CELLS_PER_AXIS) + x as usize;
                    if neighbor_index < self.cells.len() {
                        for &entity_data in &self.cells[neighbor_index] {
                            self.nearby_cache.push(entity_data);

                            if self.nearby_cache.len() >= SPATIALIZATION_MAX_NEIGHBORS {
                                return &self.nearby_cache;
                            }
                        }
                    }
                }
            }
        }

        &self.nearby_cache
    }

    pub fn mark_cell_dirty(&mut self, position: Vec3) {
        if let Some(index) = self.position_to_cell_index(position) {
            self.dirty_cells[index] = true;
        }
    }
}

pub struct SpatialGridPlugin;

impl Plugin for SpatialGridPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SpatialGrid::default());
        app.add_systems(FixedUpdate, update_spatial_grid);
    }
}

fn update_spatial_grid(
    mut grid: ResMut<SpatialGrid>,
    query: Query<(Entity, &Transform), With<Person>>,
) {
    grid.clear();

    for (entity, transform) in query.iter() {
        grid.add_entity(entity, transform.translation);
    }
}

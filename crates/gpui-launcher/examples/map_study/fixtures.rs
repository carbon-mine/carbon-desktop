// SPDX-License-Identifier: MPL-2.0
use super::terrain::{Camera, Coverage};

pub const DIMENSIONS: [&str; 2] = ["Overworld", "Nether"];

pub struct Instance {
    pub name: &'static str,
    pub detail: &'static str,
    pub worlds: &'static [&'static str],
    pub last_dimension: usize,
}
pub const INSTANCES: [Instance; 4] = [
    Instance {
        name: "Timberline Trail",
        detail: "Local instance",
        worlds: &[
            "Alder Reach",
            "Cedar Hollow",
            "Alder Reach SMP",
            "Stonefall Creative",
        ],
        last_dimension: 0,
    },
    Instance {
        name: "North Crossing",
        detail: "Server-bound instance",
        worlds: &["North Crossing"],
        last_dimension: 0,
    },
    Instance {
        name: "Redstone Workshop",
        detail: "Local instance",
        worlds: &["Redstone Workshop"],
        last_dimension: 0,
    },
    Instance {
        name: "Field Notes",
        detail: "Local instance",
        worlds: &["Field Notes"],
        last_dimension: 1,
    },
];

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection {
    pub instance: usize,
    pub world: usize,
    pub dimension: usize,
}
impl Selection {
    pub fn last(instance: usize) -> Self {
        Self {
            instance,
            world: 0,
            dimension: INSTANCES[instance].last_dimension,
        }
    }
    pub fn title(self) -> &'static str {
        INSTANCES[self.instance].worlds[self.world]
    }
    pub fn coverage(self) -> Coverage {
        match (self.instance, self.world, self.dimension) {
            (0, 0, 0) | (3, 0, 1) => Coverage::Saved,
            (1, 0, 0) => Coverage::Partial,
            _ => Coverage::Empty,
        }
    }
    pub fn position(self) -> Option<(f32, f32)> {
        (self == Self::default()).then_some((261., 243.))
    }
    pub fn camera(self) -> Camera {
        let mut camera = Camera::default();
        if let Some((x, z)) = self.position() {
            camera.x = x;
            camera.z = z;
        }
        camera
    }
    pub fn dimensions(self) -> usize {
        if self.instance == 1 || (self.instance == 0 && self.world > 0) {
            1
        } else {
            2
        }
    }
}

//! The top-down tree sprite every generated tree is stamped from.

/// A top-down tree sprite. `cls` is 0 where empty, 1 for leaf and 2 for bark.
pub struct Sprite {
    pub size: usize,
    /// Height in render pixels.
    pub z: Vec<f32>,
    /// Surface tilt direction (the normal's x and y, with z fixed at 1 before normalising).
    pub nx: Vec<f32>,
    pub ny: Vec<f32>,
    pub tone: Vec<f32>,
    /// Height relative to the tallest point of this tree.
    pub depth: Vec<f32>,
    pub cls: Vec<u8>,
    pub zmax: f32,
}

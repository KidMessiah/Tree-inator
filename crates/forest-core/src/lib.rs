//! Forest generation engine. No UI, no windowing.

pub mod brush;
pub mod compose;
pub mod dead;
pub mod engine;
pub mod extra;
pub mod field;
#[cfg(feature = "gpu")]
pub mod gpu;
pub mod groups;
pub mod mushrooms;
pub mod noise;
pub mod params;
pub mod render;
pub mod rng;
pub mod stack;
pub mod treegen;
pub mod treegen_data;
pub mod treegen_sprite;
pub mod wp;

pub use brush::{erase_dab, paint_dab, stamp_one, BrushMode, BrushSettings};
pub use compose::{compose, ComposeOptions};
pub use engine::{generate, render_size, LayerBuf, LayerKind, RenderLayers};
#[cfg(feature = "gpu")]
pub use render::export_strips_gpu;
pub use render::{export_strips, view_pad, RenderCache};
pub use groups::{diff_value, merge_value, Group, SPECIES_KEYS};
pub use params::*;
pub use stack::{compose_stack, generate_stack, zone_passes};

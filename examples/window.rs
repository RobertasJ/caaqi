//! Opens a window with a circle in the middle.
//!
//! Run with `cargo run --example window`.

use caaqi_ui::vello::{
    kurbo::{Affine, Circle},
    peniko::{Color, Fill},
};

fn main() -> Result<(), caaqi_ui::RunError> {
    caaqi_ui::run(|scene, size| {
        let center = (f64::from(size.width) / 2.0, f64::from(size.height) / 2.0);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(0x4f, 0x8c, 0xd6),
            None,
            &Circle::new(center, 100.0),
        );
    })
}

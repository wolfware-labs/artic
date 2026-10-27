//! Measure and arrange protocol, geometry and text types, and panels for artic.
//!
//! Every value type a UI language sets from text is also at the root, where generated code names it.

pub mod finite;
pub mod geometry;
mod layout;
mod sizing;
pub mod text;

pub use geometry::{CornerRadius, Point, Rect, RelativePoint, RelativeScalar, Size, Thickness, Transform};
pub use layout::{ArrangeCx, LayoutManager, LayoutNodes, LayoutStats, Layoutable, MeasureCx, PixelGrid};
pub use sizing::{HorizontalAlignment, Orientation, Sizing, VerticalAlignment, arrange_with, measure_with};
pub use text::{FamilyName, FontFamily, FontStyle, FontWeight, GenericFamily, TextAlignment, TextTrimming, TextWrapping};

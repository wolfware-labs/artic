//! The renderer seam and the retained scene.
//!
//! Elements record their drawing into a [`SceneBuilder`]; each node's recording is a
//! [`SceneFragment`], kept until the node is render-dirty again. Composition wraps the fragments in
//! their nodes' transforms, clips and opacity, in visual order, to make one frame: a [`Scene`]. A
//! renderer, from any adapter crate, draws the scene. This crate holds no backend: see the
//! [backend design](https://github.com/wolfware-labs/artic/blob/main/docs/design/backends.md).

mod brush;
mod builder;
mod color;
mod compose;
mod paint;
mod renderer;
mod scene;
mod shadow;
mod shape;
mod stroke;

pub use artic_layout::finite::NumberProblem;
pub use brush::{Brush, GradientStop, LinearGradientBrush, RadialGradientBrush, SolidColorBrush};
pub use builder::SceneBuilder;
pub use color::{Color, ColorProblem, ParseColorError};
pub use compose::{Compositor, Layers, Nodes};
pub use paint::Paint;
pub use renderer::{
  PhysicalSize, PixelBuffer, PixelFormat, RawHandles, RenderError, Renderer, SurfaceId, SurfaceView, Target, TargetKind,
};
pub use scene::{Command, Layer, Scene, SceneFragment, SceneItem, drawable_opacity};
pub use shadow::{BoxShadow, ParseBoxShadowError, ShadowProblem};
pub use shape::Shape;
pub use stroke::{LineCap, LineJoin, Stroke};

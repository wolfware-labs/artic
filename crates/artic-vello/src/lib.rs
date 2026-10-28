//! The vello renderer for artic: draws [`Scene`](artic_render::Scene)s with `vello` on `wgpu`.
//!
//! [`Renderer`] is an adapter: it implements artic's [`Renderer`](artic_render::Renderer) trait,
//! and nothing else of artic depends on it. It draws on
//! [`Target::Handles`](artic_render::Target::Handles) (windows, through a swapchain per surface) and
//! on [`Target::Pixels`](artic_render::Target::Pixels) (offscreen, read back from the GPU), using
//! DX12 on Windows, Metal on macOS and Vulkan elsewhere. `WGPU_BACKEND` overrides the choice.
//!
//! Run `cargo run -p artic-examples --example primitives` to see it draw every primitive.

mod backend;
mod error;
mod renderer;
mod translate;

pub use error::Error;
pub use renderer::Renderer;

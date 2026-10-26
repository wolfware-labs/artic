//! The winit host for artic: desktop windows and the event loop on Windows, macOS and Linux.
//!
//! A launcher passes [`Host`] to `artic::run` with a renderer and the app. It implements
//! `artic_platform::Host`, and no `winit` type appears in its public items. The design is
//! `docs/design/host.md`.

mod displays;
mod error;
mod host;
mod surfaces;
mod translate;

pub use error::Error;
pub use host::Host;

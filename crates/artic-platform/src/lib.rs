//! The host seam: how artic talks to whatever drives the screen.
//!
//! - [`Host`] owns the event loop and runs an [`AppHandler`], artic's side of it.
//! - [`HostContext`] is handed to every [`AppHandler`] call: frames, exiting, and the capabilities,
//!   such as [`Windows`], each `None` on hosts without it. [`WindowDesc`] is what a [`Windows`]
//!   host opens.
//! - [`SurfaceInfo`], [`DisplayId`] and [`HostEvent`] are what a host reports, and [`Waker`] wakes
//!   its loop from other threads.
//! - [`Pipeline`] draws one surface's frames, and [`Runner`] presents an `Application`'s surfaces
//!   and drives their frames from a host through [`run`], with [`Hooks`] for the stages above the
//!   platform.
//!
//! This crate has no backend dependencies; adapter crates such as `artic-winit` implement it. The
//! design is `docs/design/host.md`.

mod error;
mod event;
mod hooks;
mod host;
mod pipeline;
mod run;
mod runner;
mod surface;
mod waker;
mod window;

pub use artic_element::Chrome;
pub use error::Error;
pub use event::HostEvent;
pub use hooks::Hooks;
pub use host::{AppHandler, Host, HostContext, Windows};
pub use pipeline::{Frame, FrameServices, FrameStats, Pipeline};
pub use run::run;
pub use runner::Runner;
pub use surface::{DisplayId, SurfaceInfo, SurfaceLoss};
pub use waker::Waker;
pub use window::{Axes, LogicalSize, WindowDesc};

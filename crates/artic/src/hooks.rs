//! The frame hooks the facade registers on every app's runner.

use artic_platform::Runner;
use artic_render::Renderer;

/// Registers the facade's frame hooks on `runner`.
pub(crate) fn register<R: Renderer>(_runner: &mut Runner<R>) {}

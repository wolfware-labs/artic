//! Examples that open a window: each file in `examples/` runs artic through the `winit` host and the `vello` renderer.

/// `error`'s message, then each cause, for an example to print.
pub fn chain(error: &dyn std::error::Error) -> String {
  std::iter::successors(error.source(), |cause| cause.source())
    .fold(error.to_string(), |message, cause| format!("{message}: {cause}"))
}

//! An app whose views its build script compiles, for the compiler's integration tests.

/// The compiled views.
pub mod views {
  /// `src/views/card.xaml`.
  pub mod card {
    include!(concat!(env!("OUT_DIR"), "/views/card.rs"));
  }
}

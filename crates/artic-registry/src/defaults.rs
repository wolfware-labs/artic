/// Changes defaults the type doesn't declare itself, such as those of a flattened property group.
///
/// A type opts in with `#[ui(override_defaults)]`; its `Default` then builds every field with
/// its declared default and calls [`override_defaults`](OverrideDefaults::override_defaults) once.
/// Set values with `Property::set_default`, so an inherited value, a style, a binding or markup
/// still wins over them.
///
/// ```ignore
/// #[element]
/// #[derive(Debug)]
/// #[ui(override_defaults)]
/// pub struct Button {}
///
/// impl OverrideDefaults for Button {
///   fn override_defaults(&self) {
///     self.interaction.focusable.set_default(true);
///   }
/// }
/// ```
pub trait OverrideDefaults {
  /// Sets the type's own defaults over the ones its fields were built with.
  fn override_defaults(&self);
}

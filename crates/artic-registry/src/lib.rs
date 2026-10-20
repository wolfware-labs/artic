//! Type registry that lets UI languages find artic types and their properties.
//!
//! Every crate that defines UI types adds a [`TypeInfo`] to the [`UI_TYPES`] slice, and
//! [`Registry::from_link_time`] indexes all of them for lookup. There is no registration call: the
//! linker gathers the entries.
//!
//! The `literals` feature fills [`PropertyInfo::literal`], for a code generator in a build script.

#[doc(hidden)]
pub mod __private;
mod collection;
mod defaults;
mod descriptors;
mod registry;
mod value_error;
mod with_children;

/// What [`PropertyInfo::literal`] writes with, re-exported so a code generator needs only this crate.
pub use artic_core::{RustExpr, ToRust};
pub use collection::UiCollection;
pub use defaults::OverrideDefaults;
pub use descriptors::{
  ArgInfo, AttachedPropertyInfo, AttachedTarget, ContentInfo, ContentMethod, EventInfo, HasGroup, PropertyGroup,
  PropertyGroupInfo, PropertyInfo, PropertyRef, TypeInfo, UiType, ValueProviderInfo, Whitespace, WriteLiteral,
};
pub use registry::{Duplicate, Lookup, Registry, RegistryError};
pub use value_error::ValueError;
pub use with_children::WithChildren;

#[doc(hidden)]
pub use linkme;
use linkme::distributed_slice;

/// Every [`TypeInfo`] registered by the crates linked into the binary.
///
/// The `UiType` derive adds one entry per type. To add one by hand, use [`register_type!`]:
///
/// ```
/// use artic_core::Object;
/// use artic_core::property::AnyProperty;
/// use artic_core::tree::{NodeId, UiTree};
/// use artic_registry::{Registry, TypeInfo, Whitespace};
///
/// #[derive(Debug)]
/// struct Gauge;
/// impl Object for Gauge {
///   fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
/// }
///
/// static GAUGE: TypeInfo = TypeInfo {
///   name: "Gauge",
///   rust_path: "my_app::views::Gauge",
///   new: |tree: &mut UiTree| -> NodeId { tree.alloc(Box::new(Gauge)) },
///   content: None,
///   is_collection: false,
///   properties: &[],
///   groups: &[],
///   events: &[],
///   attached_properties: &[],
///   value_provider: None,
///   whitespace: Whitespace::Collapse,
/// };
///
/// artic_registry::register_type!(GAUGE);
///
/// let registry = Registry::from_link_time().expect("no duplicates");
/// assert!(registry.type_by_rust_path("my_app::views::Gauge").is_some());
/// ```
///
/// # Linking rules
///
/// The linker only gathers entries from crates that are part of the link, and two cases leave a
/// crate out without any error:
///
/// 1. **A crate that nothing references isn't linked.** rustc drops a dependency the code never
///    names, before the linker runs, so its entries disappear even though it is in `Cargo.toml`.
///    Naming it once keeps it: `use my_widgets as _;`.
/// 2. **An iOS static library needs `-force_load`.** `ld` only loads the archive members that
///    resolve a symbol it needs, and nothing refers to the members holding entries. Pass
///    `-force_load` with the archive, and pass the archive only once.
///
/// Entries survive fat and thin LTO, `codegen-units = 1`, `--gc-sections` and `-dead_strip`, and an
/// Android `cdylib` needs nothing extra.
#[distributed_slice]
pub static UI_TYPES: [&'static TypeInfo];

/// Adds a [`TypeInfo`] `static` to [`UI_TYPES`].
///
/// The crate calling it needs no `linkme` dependency of its own, and one module can register
/// several types. The entry only reaches the registry if the calling crate is linked; see the
/// linking rules on [`UI_TYPES`].
///
/// # Examples
///
/// ```
/// use artic_core::Object;
/// use artic_core::property::AnyProperty;
/// use artic_core::tree::{NodeId, UiTree};
/// use artic_registry::{Registry, TypeInfo, Whitespace};
///
/// #[derive(Debug)]
/// struct Dial;
/// impl Object for Dial {
///   fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
/// }
///
/// static DIAL: TypeInfo = TypeInfo {
///   name: "Dial",
///   rust_path: "my_app::views::Dial",
///   new: |tree: &mut UiTree| -> NodeId { tree.alloc(Box::new(Dial)) },
///   content: None,
///   is_collection: false,
///   properties: &[],
///   groups: &[],
///   events: &[],
///   attached_properties: &[],
///   value_provider: None,
///   whitespace: Whitespace::Collapse,
/// };
///
/// artic_registry::register_type!(DIAL);
///
/// let registry = Registry::from_link_time().expect("no duplicates");
/// assert!(registry.type_by_name("my_app", "Dial").found().is_some());
/// ```
#[macro_export]
macro_rules! register_type {
  ($info:path) => {
    const _: () = {
      #[$crate::linkme::distributed_slice($crate::UI_TYPES)]
      #[linkme(crate = $crate::linkme)]
      static ENTRY: &$crate::TypeInfo = &$info;
    };
  };
}

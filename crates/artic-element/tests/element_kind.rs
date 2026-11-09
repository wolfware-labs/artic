// Every #[element] type is its own kind, so a caller can ask for an Application root by type.

use artic_element::{Application, ElementKind, Surface};
use std::any::TypeId;

/// The `TypeId` of `T`'s kind.
fn kind_of<T: ElementKind>() -> TypeId {
  TypeId::of::<T::Kind>()
}

// An element's kind is its own type.
#[test]
fn element_is_its_own_kind() {
  assert_eq!(kind_of::<Application>(), TypeId::of::<Application>());
  assert_eq!(kind_of::<Surface>(), TypeId::of::<Surface>());
}

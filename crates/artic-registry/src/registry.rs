mod error;

use crate::{AttachedPropertyInfo, TypeInfo, UI_TYPES};
pub use error::{Duplicate, RegistryError};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

/// Lookup tables over a set of [`TypeInfo`] descriptors.
///
/// Build it once, usually with [`from_link_time`](Registry::from_link_time), and keep it: every
/// lookup returns a `&'static` descriptor, which stays valid after the registry is dropped.
#[derive(Debug, Default)]
pub struct Registry {
  by_name: HashMap<&'static str, HashMap<&'static str, &'static TypeInfo>>,
  by_rust_path: HashMap<&'static str, &'static TypeInfo>,
  attached: HashMap<&'static str, HashMap<&'static str, &'static AttachedPropertyInfo>>,
}

impl Registry {
  /// Builds a registry over every type in [`UI_TYPES`]: the types registered by the crates linked
  /// into the binary.
  ///
  /// # Errors
  ///
  /// Returns the duplicates, as [`from_types`](Registry::from_types) does.
  pub fn from_link_time() -> Result<Registry, RegistryError> {
    Self::from_types(&UI_TYPES)
  }

  /// Builds a registry over `types`.
  ///
  /// # Errors
  ///
  /// Returns every duplicate at once: two types with the same namespace and name, two types with
  /// the same Rust path, or one owner declaring the same attached property twice.
  pub fn from_types(types: &[&'static TypeInfo]) -> Result<Registry, RegistryError> {
    let mut registry = Registry::default();
    let mut duplicates = Vec::new();
    for info in types {
      match registry.by_name.entry(info.namespace).or_default().entry(info.name) {
        Entry::Occupied(existing) => {
          let (a, b) = (existing.get().rust_path, info.rust_path);
          // TODO: use `let [first, second] = std::cmp::minmax(a, b);` once `cmp_minmax` is stable.
          duplicates.push(Duplicate::Name {
            namespace: info.namespace,
            name: info.name,
            first: a.min(b),
            second: a.max(b),
          });
        }
        Entry::Vacant(slot) => {
          slot.insert(info);
        }
      }
      if registry.by_rust_path.insert(info.rust_path, info).is_some() {
        duplicates.push(Duplicate::RustPath { path: info.rust_path });
      }
      let mut names = HashSet::new();
      let groups = info.groups.iter().flat_map(|group| group.properties);
      for property in info.properties.iter().chain(groups) {
        if !names.insert(property.name) {
          duplicates.push(Duplicate::Property {
            owner: info.rust_path,
            name: property.name,
          });
        }
      }
      for attached in info.attached_properties {
        let owned = registry.attached.entry(info.rust_path).or_default();
        if owned.insert(attached.name, attached).is_some() {
          duplicates.push(Duplicate::AttachedProperty {
            owner: info.rust_path,
            name: attached.name,
          });
        }
      }
    }
    if duplicates.is_empty() {
      Ok(registry)
    } else {
      duplicates.sort();
      Err(RegistryError { duplicates })
    }
  }

  /// Returns the type named `name` in `namespace`. The match is exact and case-sensitive.
  #[must_use]
  pub fn type_by_name(&self, namespace: &str, name: &str) -> Option<&'static TypeInfo> {
    self.by_name.get(namespace)?.get(name).copied()
  }

  /// Returns the type whose full Rust path is `path`, such as `my_app::views::Gauge`.
  #[must_use]
  pub fn type_by_rust_path(&self, path: &str) -> Option<&'static TypeInfo> {
    self.by_rust_path.get(path).copied()
  }

  /// Returns the attached property `name` declared by `owner`, such as `Row` on `Grid`.
  #[must_use]
  pub fn attached_property(&self, owner: &TypeInfo, name: &str) -> Option<&'static AttachedPropertyInfo> {
    self.attached.get(owner.rust_path)?.get(name).copied()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{AttachedTarget, PropertyGroupInfo, PropertyInfo, Whitespace};
  use artic_core::Object;
  use artic_core::property::Affects;
  use artic_core::property::AnyProperty;
  use artic_core::tree::{NodeId, UiTree};
  use pretty_assertions::assert_eq;

  const UI: &str = "https://artic.rs/ui";

  #[derive(Debug)]
  struct Node;

  impl Object for Node {
    fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  }

  fn new_node(tree: &mut UiTree) -> NodeId {
    tree.alloc(Box::new(Node))
  }

  /// Returns a descriptor with no properties, events or attached properties.
  const fn info(namespace: &'static str, name: &'static str, rust_path: &'static str) -> TypeInfo {
    TypeInfo {
      name,
      namespace,
      rust_path,
      new: new_node,
      content: None,
      is_collection: false,
      properties: &[],
      groups: &[],
      events: &[],
      attached_properties: &[],
      value_provider: None,
      whitespace: Whitespace::Collapse,
    }
  }

  static GROUP: [PropertyInfo; 1] = [PropertyInfo {
    name: "Margin",
    field: "margin",
    value_type: "Thickness",
    affects: Affects::LAYOUT,
    inherits: false,
  }];
  static GROUPS: [PropertyGroupInfo; 1] = [PropertyGroupInfo {
    field: "layout",
    properties: &GROUP,
  }];

  // A type's properties include its groups', each with the field of the group it lives in.
  #[test]
  fn property_lookup_searches_groups() {
    let info = TypeInfo {
      groups: &GROUPS,
      ..info(UI, "Border", "artic_controls::Border")
    };

    let found = info.property("Margin").expect("Margin, from the group");
    assert_eq!(found.group, Some("layout"));
    assert!(info.property("Padding").is_none());
  }

  // A type's own property named like a group's is a duplicate.
  #[test]
  fn own_property_shadowing_a_group_is_a_duplicate() {
    static OWN: [PropertyInfo; 1] = [PropertyInfo {
      name: "Margin",
      field: "margin",
      value_type: "f32",
      affects: Affects::NONE,
      inherits: false,
    }];
    static BORDER: TypeInfo = TypeInfo {
      properties: &OWN,
      groups: &GROUPS,
      ..info(UI, "Border", "artic_controls::Border")
    };

    let error = Registry::from_types(&[&BORDER]).unwrap_err();
    assert_eq!(
      error.duplicates,
      [Duplicate::Property {
        owner: BORDER.rust_path,
        name: "Margin",
      }]
    );
  }

  static BUTTON: TypeInfo = info(UI, "Button", "artic_controls::Button");
  static APP_BUTTON: TypeInfo = info(UI, "Button", "app::Button");
  static GAUGE: TypeInfo = info("using:app::views", "Gauge", "app::views::Gauge");
  static GRID: TypeInfo = TypeInfo {
    attached_properties: &[AttachedPropertyInfo {
      name: "Row",
      value_type: "usize",
      target: AttachedTarget::ChildrenOfOwner,
    }],
    ..info(UI, "Grid", "artic_controls::Grid")
  };

  // A name is found in its namespace exactly as written; another case or another namespace finds
  // nothing.
  #[test]
  fn lookup_is_exact_and_case_sensitive() {
    let registry = Registry::from_types(&[&BUTTON]).expect("no duplicates");

    assert_eq!(
      registry.type_by_name(UI, "Button").map(|info| info.rust_path),
      Some("artic_controls::Button")
    );
    assert!(registry.type_by_name(UI, "button").is_none());
    assert!(registry.type_by_name("https://other.example", "Button").is_none());
  }

  // A type is found by its full Rust path, which is how `using:` prefixes resolve.
  #[test]
  fn lookup_by_rust_path() {
    let registry = Registry::from_types(&[&BUTTON, &GAUGE]).expect("no duplicates");

    assert_eq!(
      registry.type_by_rust_path("app::views::Gauge").map(|info| info.name),
      Some("Gauge")
    );
    assert!(registry.type_by_rust_path("app::views::Missing").is_none());
  }

  // An attached property is found through the type that owns it, not through other types.
  #[test]
  fn attached_property_lookup_by_owner() {
    let registry = Registry::from_types(&[&BUTTON, &GRID]).expect("no duplicates");

    assert_eq!(
      registry.attached_property(&GRID, "Row").map(|row| row.value_type),
      Some("usize")
    );
    assert!(registry.attached_property(&BUTTON, "Row").is_none());
    assert!(registry.attached_property(&GRID, "Column").is_none());
  }

  // Two types with the same name in one namespace fail the build, and the error names both Rust
  // paths, in sorted order.
  #[test]
  fn duplicate_names_error() {
    let error = Registry::from_types(&[&BUTTON, &APP_BUTTON]).expect_err("duplicate names");

    assert_eq!(
      error.duplicates,
      [Duplicate::Name {
        namespace: UI,
        name: "Button",
        first: "app::Button",
        second: "artic_controls::Button",
      }]
    );
    assert_eq!(
      error.to_string(),
      "two types are named `Button` in https://artic.rs/ui: `app::Button` and `artic_controls::Button`"
    );
  }

  // Every duplicate is reported at once, sorted, whatever order the types came in.
  #[test]
  fn every_duplicate_is_reported_in_sorted_order() {
    static GRID_AGAIN: TypeInfo = TypeInfo {
      attached_properties: &[
        AttachedPropertyInfo {
          name: "Row",
          value_type: "usize",
          target: AttachedTarget::ChildrenOfOwner,
        },
        AttachedPropertyInfo {
          name: "Row",
          value_type: "usize",
          target: AttachedTarget::ChildrenOfOwner,
        },
      ],
      ..info("https://other.example", "Grid", "artic_controls::Grid")
    };

    let forward = Registry::from_types(&[&GRID, &GRID_AGAIN, &APP_BUTTON, &BUTTON]).expect_err("duplicates");
    let backward = Registry::from_types(&[&BUTTON, &APP_BUTTON, &GRID_AGAIN, &GRID]).expect_err("duplicates");

    assert_eq!(forward, backward);
    assert_eq!(
      forward.duplicates,
      [
        Duplicate::Name {
          namespace: UI,
          name: "Button",
          first: "app::Button",
          second: "artic_controls::Button",
        },
        Duplicate::RustPath {
          path: "artic_controls::Grid"
        },
        Duplicate::AttachedProperty {
          owner: "artic_controls::Grid",
          name: "Row",
        },
        Duplicate::AttachedProperty {
          owner: "artic_controls::Grid",
          name: "Row",
        },
      ]
    );
  }
}

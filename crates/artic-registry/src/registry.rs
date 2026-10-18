mod error;

use crate::{AttachedPropertyInfo, TypeInfo, UI_TYPES};
pub use error::{Duplicate, RegistryError};
use std::collections::{HashMap, HashSet};

/// What a lookup by namespace and name found.
#[derive(Debug, Clone)]
pub enum Lookup {
  /// The one type with that name in the namespace.
  Found(&'static TypeInfo),
  /// Several types share the name in the namespace; their Rust paths tell them apart.
  Ambiguous(Vec<&'static TypeInfo>),
  /// No type has that name in the namespace.
  Missing,
}

impl Lookup {
  /// The type, if exactly one was found.
  #[must_use]
  pub fn found(self) -> Option<&'static TypeInfo> {
    match self {
      Lookup::Found(info) => Some(info),
      Lookup::Ambiguous(_) | Lookup::Missing => None,
    }
  }
}

/// Lookup tables over a set of [`TypeInfo`] descriptors.
///
/// Build it once, usually with [`from_link_time`](Registry::from_link_time), and keep it: every
/// lookup returns a `&'static` descriptor, which stays valid after the registry is dropped.
#[derive(Debug, Default)]
pub struct Registry {
  by_name: HashMap<&'static str, HashMap<&'static str, Vec<&'static TypeInfo>>>,
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
  /// Returns every duplicate at once: two types with the same Rust path, a type with two
  /// properties of one name, or one owner declaring the same attached property twice. Two types
  /// may share a name, even in one crate; a lookup by name then reports them as
  /// [`Ambiguous`](Lookup::Ambiguous).
  pub fn from_types(types: &[&'static TypeInfo]) -> Result<Registry, RegistryError> {
    let mut registry = Registry::default();
    let mut duplicates = Vec::new();
    for info in types {
      registry
        .by_name
        .entry(info.namespace())
        .or_default()
        .entry(info.name)
        .or_default()
        .push(info);
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

  /// Looks up the type named `name` in `namespace`, a crate such as `artic_controls`. Names are
  /// exact and case-sensitive. Two types of one name in one crate make the name
  /// [`Ambiguous`](Lookup::Ambiguous); [`type_by_rust_path`](Registry::type_by_rust_path) finds
  /// each.
  #[must_use]
  pub fn type_by_name(&self, namespace: &str, name: &str) -> Lookup {
    match self
      .by_name
      .get(namespace)
      .and_then(|names| names.get(name))
      .map(Vec::as_slice)
    {
      Some([one]) => Lookup::Found(one),
      Some(many) if !many.is_empty() => Lookup::Ambiguous(many.to_vec()),
      _ => Lookup::Missing,
    }
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

  #[derive(Debug)]
  struct Node;

  impl Object for Node {
    fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  }

  fn new_node(tree: &mut UiTree) -> NodeId {
    tree.alloc(Box::new(Node))
  }

  /// Returns a descriptor with no properties, events or attached properties.
  const fn info(name: &'static str, rust_path: &'static str) -> TypeInfo {
    TypeInfo {
      name,
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
      ..info("Border", "artic_controls::Border")
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
      ..info("Border", "artic_controls::Border")
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

  static BUTTON: TypeInfo = info("Button", "artic_controls::Button");
  static APP_BUTTON: TypeInfo = info("Button", "app::Button");
  static GAUGE: TypeInfo = info("Gauge", "app::views::Gauge");
  static GRID: TypeInfo = TypeInfo {
    attached_properties: &[AttachedPropertyInfo {
      name: "Row",
      value_type: "usize",
      target: AttachedTarget::ChildrenOfOwner,
    }],
    ..info("Grid", "artic_controls::Grid")
  };

  static HOME_PAGE: TypeInfo = info("Page", "my_app::home::Page");
  static SETTINGS_PAGE: TypeInfo = info("Page", "my_app::settings::Page");

  // A type's namespace is the first segment of its Rust path: its crate.
  #[test]
  fn namespace_is_the_first_segment_of_the_rust_path() {
    assert_eq!(info("Border", "artic_controls::border::Border").namespace(), "artic_controls");
  }

  // A path without segments is its own namespace.
  #[test]
  fn namespace_of_a_path_without_segments_is_the_path() {
    assert_eq!(info("Dial", "Dial").namespace(), "Dial");
  }

  // Two types of one name in one crate are both registered: the name is ambiguous, and each Rust
  // path finds its own.
  #[test]
  fn same_name_in_one_crate_is_ambiguous_by_name_and_found_by_path() {
    let registry = Registry::from_types(&[&HOME_PAGE, &SETTINGS_PAGE]).expect("not a duplicate");

    match registry.type_by_name("my_app", "Page") {
      Lookup::Ambiguous(found) => assert_eq!(found.len(), 2),
      other => panic!("expected ambiguity, got {other:?}"),
    }
    assert!(std::ptr::eq(
      registry.type_by_rust_path("my_app::home::Page").unwrap(),
      &HOME_PAGE
    ));
    assert!(std::ptr::eq(
      registry.type_by_rust_path("my_app::settings::Page").unwrap(),
      &SETTINGS_PAGE
    ));
  }

  // The same Rust path registered twice is still an error.
  #[test]
  fn same_rust_path_twice_is_an_error() {
    assert_eq!(
      Registry::from_types(&[&HOME_PAGE, &HOME_PAGE]).unwrap_err().duplicates,
      [Duplicate::RustPath {
        path: "my_app::home::Page"
      }]
    );
  }

  // A name nothing registered is missing; a found one is found.
  #[test]
  fn missing_name_is_missing() {
    let registry = Registry::from_types(&[&HOME_PAGE]).expect("no duplicates");

    assert!(matches!(registry.type_by_name("my_app", "Gone"), Lookup::Missing));
    assert!(matches!(registry.type_by_name("other", "Page"), Lookup::Missing));
    assert!(registry.type_by_name("my_app", "Page").found().is_some());
  }

  // A name is found in its namespace exactly as written; another case or another namespace finds
  // nothing.
  #[test]
  fn lookup_is_exact_and_case_sensitive() {
    let registry = Registry::from_types(&[&BUTTON]).expect("no duplicates");

    assert_eq!(
      registry
        .type_by_name("artic_controls", "Button")
        .found()
        .map(|info| info.rust_path),
      Some("artic_controls::Button")
    );
    assert!(matches!(registry.type_by_name("artic_controls", "button"), Lookup::Missing));
    assert!(matches!(registry.type_by_name("app", "Button"), Lookup::Missing));
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
      ..info("Grid", "artic_controls::Grid")
    };

    let forward = Registry::from_types(&[&GRID, &GRID_AGAIN, &APP_BUTTON, &BUTTON]).expect_err("duplicates");
    let backward = Registry::from_types(&[&BUTTON, &APP_BUTTON, &GRID_AGAIN, &GRID]).expect_err("duplicates");

    assert_eq!(forward, backward);
    assert_eq!(
      forward.duplicates,
      [
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

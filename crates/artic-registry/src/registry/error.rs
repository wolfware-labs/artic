/// The duplicates that kept a [`Registry`](crate::Registry) from being built.
///
/// Its message lists every duplicate, one per line.
#[derive(Debug, Clone, Eq, PartialEq, thiserror::Error)]
#[error("{}", lines(&self.duplicates))]
pub struct RegistryError {
  /// Every duplicate found, sorted.
  pub duplicates: Vec<Duplicate>,
}

/// One clash between registered descriptors.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, thiserror::Error)]
pub enum Duplicate {
  /// Two descriptors have the same Rust path.
  #[error("two types are registered for the Rust path `{path}`")]
  RustPath {
    /// The Rust path both descriptors use.
    path: &'static str,
  },
  /// A type has two properties with one name, counting the groups it flattens.
  #[error("`{owner}` has two properties named `{name}`, counting the groups it flattens")]
  Property {
    /// The Rust path of the type.
    owner: &'static str,
    /// The property's name.
    name: &'static str,
  },
  /// One owner declares the same attached property twice.
  #[error("`{owner}` declares the attached property `{name}` twice")]
  AttachedProperty {
    /// The Rust path of the owner.
    owner: &'static str,
    /// The attached property's name.
    name: &'static str,
  },
}

fn lines(duplicates: &[Duplicate]) -> String {
  duplicates.iter().map(Duplicate::to_string).collect::<Vec<_>>().join("\n")
}

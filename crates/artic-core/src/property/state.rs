use crate::property::PropertyKey;
use crate::property::levels::Levels;

#[derive(Debug)]
pub struct PropertyState<T>
where
  T: 'static,
{
  pub levels: Levels<T>,
  key: &'static PropertyKey<T>,
}

impl<T> PropertyState<T>
where
  T: PartialEq,
{
  pub fn new(key: &'static PropertyKey<T>, default: T) -> Self {
    Self {
      levels: Levels::new(default, key.inheritance()),
      key,
    }
  }

  pub fn key(&self) -> &'static PropertyKey<T> {
    self.key
  }
}

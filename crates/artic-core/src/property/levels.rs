use crate::property::{Inheritance, Precedence, Property};
use crate::reactive::Change;

const VALUE_LEVELS: usize = 4;

#[derive(Debug)]
pub struct Levels<T>
where
  T: 'static,
{
  values: [Option<T>; VALUE_LEVELS],
  inherited: Option<Property<T>>,
  default: T,
  inheritance: Inheritance<T>,
}

impl<T> Levels<T>
where
  T: PartialEq,
{
  pub fn new(default: T, inheritance: Inheritance<T>) -> Self {
    Self {
      values: [const { None }; VALUE_LEVELS],
      inherited: None,
      default,
      inheritance,
    }
  }

  pub fn with_effective<TResult>(&self, function: impl FnOnce(&T) -> TResult) -> TResult {
    let own = self.values.iter().rev().find_map(Option::as_ref);
    let Some(source) = self.inherited.filter(|_| own.is_none() || self.combines()) else {
      return self.inheritance.resolve(own, &self.default, None, function);
    };
    let (mut function, mut result) = (Some(function), None);
    let read: &mut dyn FnMut(&T) = &mut |inherited| {
      let function = function.take().expect("a property reads its source once");
      result = Some(self.inheritance.resolve(own, &self.default, Some(inherited), function));
    };
    source.with(read);
    result.expect("a property reads its source once")
  }

  pub fn with_own<TResult>(&self, function: impl FnOnce(&T) -> TResult) -> TResult {
    function(self.values.iter().rev().find_map(Option::as_ref).unwrap_or(&self.default))
  }

  pub fn effective_source(&self) -> Precedence {
    match self.values.iter().rposition(Option::is_some) {
      Some(index) => Self::level_at(index),
      None if self.inherited.is_some() && !self.combines() => Precedence::Inherited,
      None => Precedence::Default,
    }
  }

  pub fn combines(&self) -> bool {
    matches!(self.inheritance, Inheritance::Combine(_))
  }

  pub fn set(&mut self, level: Precedence, value: T) -> Change {
    let winner = self.effective_source();
    match Self::value_index(level) {
      Some(index) => {
        if self.values[index].as_ref() == Some(&value) {
          return Change::Hidden;
        }
        self.values[index] = Some(value);
      }
      None => {
        if self.default == value {
          return Change::Hidden;
        }
        self.default = value;
      }
    }
    if level >= winner { Change::Visible } else { Change::Hidden }
  }

  pub fn clear(&mut self, level: Precedence) -> Change {
    let winner = self.effective_source();
    let index = Self::value_index(level).expect("only value levels can be cleared");
    if self.values[index].take().is_some() && level == winner {
      Change::Visible
    } else {
      Change::Hidden
    }
  }

  pub fn inherited(&self) -> Option<Property<T>> {
    self.inherited
  }

  pub fn set_inherited(&mut self, source: Option<Property<T>>) -> Change {
    if self.inherited == source {
      return Change::Hidden;
    }
    let winner = self.effective_source();
    self.inherited = source;
    if self.combines() || winner <= Precedence::Inherited {
      Change::Visible
    } else {
      Change::Hidden
    }
  }

  fn value_index(level: Precedence) -> Option<usize> {
    (level as usize).checked_sub(Precedence::StyleSetter as usize)
  }

  fn level_at(index: usize) -> Precedence {
    [
      Precedence::StyleSetter,
      Precedence::Binding,
      Precedence::Local,
      Precedence::Animation,
    ][index]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  /// Returns the effective value of `levels`.
  fn effective(levels: &Levels<i32>) -> i32 {
    levels.with_effective(|value| *value)
  }

  // Each value level maps to its own slot and back; Default and Inherited have no slot.
  #[test]
  fn value_index_round_trips() {
    for level in [
      Precedence::StyleSetter,
      Precedence::Binding,
      Precedence::Local,
      Precedence::Animation,
    ] {
      let index = Levels::<i32>::value_index(level).expect("a value level has a slot");
      assert_eq!(Levels::<i32>::level_at(index), level);
    }
    assert_eq!(Levels::<i32>::value_index(Precedence::Default), None);
    assert_eq!(Levels::<i32>::value_index(Precedence::Inherited), None);
  }

  // Writing the winning level or a level above it is visible; writing below it is hidden.
  #[test]
  fn set_is_visible_only_at_or_above_the_winner() {
    let mut levels = Levels::new(0, Inheritance::None);
    assert_eq!(levels.set(Precedence::Binding, 1), Change::Visible);

    assert_eq!(levels.set(Precedence::StyleSetter, 2), Change::Hidden);
    assert_eq!(levels.set(Precedence::Binding, 3), Change::Visible);
    assert_eq!(levels.set(Precedence::Local, 3), Change::Visible);
    assert_eq!(effective(&levels), 3);
    assert_eq!(levels.effective_source(), Precedence::Local);
  }

  // Writing the value a level already holds stores nothing and is hidden.
  #[test]
  fn set_with_the_same_value_is_hidden() {
    let mut levels = Levels::new(0, Inheritance::None);
    levels.set(Precedence::Local, 1);

    assert_eq!(levels.set(Precedence::Local, 1), Change::Hidden);
    assert_eq!(levels.set(Precedence::Default, 0), Change::Hidden);
  }

  // Writing the default is visible only while no level above it is set.
  #[test]
  fn set_default_is_visible_only_when_nothing_overrides_it() {
    let mut levels = Levels::new(0, Inheritance::None);
    assert_eq!(levels.set(Precedence::Default, 1), Change::Visible);

    levels.set(Precedence::StyleSetter, 2);

    assert_eq!(levels.set(Precedence::Default, 3), Change::Hidden);
    assert_eq!(effective(&levels), 2);
  }

  // Clearing the winning level is visible and reveals the next one; clearing any other level is
  // hidden.
  #[test]
  fn clear_is_visible_only_for_the_winner() {
    let mut levels = Levels::new(0, Inheritance::None);
    levels.set(Precedence::StyleSetter, 1);
    levels.set(Precedence::Local, 2);

    assert_eq!(levels.clear(Precedence::Binding), Change::Hidden);
    assert_eq!(levels.clear(Precedence::StyleSetter), Change::Hidden);
    assert_eq!(levels.clear(Precedence::Local), Change::Visible);
    assert_eq!(effective(&levels), 0);
    assert_eq!(levels.effective_source(), Precedence::Default);
  }
}

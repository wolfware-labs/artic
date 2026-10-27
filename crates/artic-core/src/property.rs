//! Properties: reactive values that several sources can write at once, ranked by precedence.
//!
//! An animation, app code, a binding, a style, inheritance and a default can all hold a value for
//! the same [`Property`]. Each writes its own [`Precedence`] level, and the highest level holding a
//! value wins. When a source stops having an opinion it clears its level, and the next one shows
//! through.
//!
//! ```
//! use artic_core::property::{Affects, Inheritance, Precedence, Property, PropertyKey};
//! use artic_core::reactive::{effect, flush_effects};
//! use std::cell::Cell;
//! use std::rc::Rc;
//!
//! static BACKGROUND: PropertyKey<&str> = PropertyKey::new("Background", Affects::RENDER, Inheritance::None);
//!
//! let background = Property::new(&BACKGROUND, "white");
//! let painted = Rc::new(Cell::new(""));
//! let painted_in_effect = Rc::clone(&painted);
//! effect(move || painted_in_effect.set(background.get()));
//!
//! background.set_at(Precedence::StyleSetter, "blue");
//! background.set_at(Precedence::Animation, "lightblue"); // hover starts
//! flush_effects();
//! assert_eq!(painted.get(), "lightblue");
//!
//! background.clear(Precedence::Animation); // hover ends
//! flush_effects();
//! assert_eq!(painted.get(), "blue");
//! ```
//!
//! # Keys and metadata
//!
//! Every property is created with a [`PropertyKey`]: a `static` per property kind that names it,
//! says which frame stages a change [`Affects`], and says how it inherits ([`Inheritance`]).
//! Properties on different node types are the same kind when they share the key.
//!
//! Once a node is in a [`UiTree`](crate::tree::UiTree), a change to the effective value of
//! one of its properties adds the key's stages to the node's dirty flags right away. A write that
//! readers can't see marks nothing.
//!
//! # Inheritance
//!
//! A property whose key inherits follows the nearest logical ancestor that has a property of the
//! same kind. Under [`Inheritance::Replace`] it takes that ancestor's value when nothing above the
//! [`Precedence::Inherited`] level is set. Under [`Inheritance::Combine`] its effective value is
//! the key's function of its own value and the ancestor's, such as [`inherit::and`], so a
//! disabled panel disables a child that enables itself; [`own`](Property::own) reads the child's
//! own value. The link is to that ancestor's property rather than a copy, so the inherited value
//! is never stale, and a change on the ancestor marks the nodes that depend on it dirty. The tree
//! sets the link when a node moves in the logical tree; the visual tree plays no part.
//!
//! A property has two owners: the [`Scope`](crate::reactive::Scope) that was active when it was
//! created, and the node it ends up on. Whichever lets go first disposes it: disposing the scope,
//! or the tree [freeing](crate::tree::UiTree::free) the node. A copy kept past that panics when
//! read, and so does reading it through the node, so dispose a scope only once the nodes built in
//! it have left the tree. Attached values are created by the tree and belong to their node alone.
//! Dropping the tree disposes what its nodes still hold.

mod affects;
mod any;
mod attached;
pub mod inherit;
mod key;
mod levels;
mod parse_enum_error;
mod shared;
mod state;

use crate::reactive::{Change, Signal};
pub use affects::Affects;
pub(crate) use any::Erased;
pub use any::{AnyProperty, KeyId};
pub use attached::AttachedKey;
pub use key::{Inheritance, PropertyKey};
use levels::Levels;
pub use parse_enum_error::ParseEnumError;
pub use shared::SharedKey;
use state::PropertyState;

/// Who supplied a property's value, from lowest to highest priority.
///
/// The ordering follows priority, so a higher level compares greater:
/// `Animation` > `Local` > `Binding` > `StyleSetter` > `Inherited` > `Default`.
///
/// # Examples
///
/// ```
/// use artic_core::property::Precedence;
///
/// assert!(Precedence::Animation > Precedence::Local);
/// assert!(Precedence::StyleSetter > Precedence::Default);
/// ```
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Precedence {
  /// The value a property has when no other level is set.
  Default,
  /// The value of the same kind of property on the nearest logical ancestor that has one. The tree
  /// sets this level; [`Property::set_at`] and [`Property::clear`] refuse it.
  Inherited,
  /// A value set by a matching style.
  StyleSetter,
  /// A value written by a data binding.
  Binding,
  /// A value set directly: by a UI language, or with `set` in code.
  Local,
  /// A value written by a running animation.
  Animation,
}

/// A reactive value that several sources can write at once, each at its own [`Precedence`] level.
///
/// Every level holds its own value, or none; the `Inherited` level instead follows the same kind of
/// property on a logical ancestor, which the tree sets. The property's *effective* value comes from
/// the highest level that holds one; `Default` always does. Writing a level only replaces that
/// level's value, and [`clear`](Property::clear) empties it so the next level down shows through. A
/// style that stops matching or an animation that ends clears its level, and nobody has to remember
/// what the value was before.
///
/// Reading the property through [`get`](Property::get) or [`with`](Property::with) subscribes the
/// running observer. Readers are marked dirty only when what they can see changes: the effective
/// value or the level it comes from. A write to a level below the winning one wakes nobody.
///
/// `Property<T>` is a `Copy` handle, like [`Signal`], and is neither `Send` nor `Sync`. Two handles
/// compare equal when they refer to the same property. It's disposed with the
/// [`Scope`](crate::reactive::Scope) that was active when it was created, or with its node when the
/// tree frees it, whichever comes first.
///
/// # Examples
///
/// ```
/// use artic_core::property::{Affects, Inheritance, Precedence, Property, PropertyKey};
///
/// static WIDTH: PropertyKey<f64> = PropertyKey::new("Width", Affects::LAYOUT, Inheritance::None);
///
/// let width = Property::new(&WIDTH, 0.0);
/// width.set_at(Precedence::StyleSetter, 50.0);
/// width.set(100.0); // the Local level
/// assert_eq!(width.get(), 100.0);
///
/// width.clear(Precedence::Local);
/// assert_eq!(width.get(), 50.0);
/// ```
#[derive(Debug)]
pub struct Property<T>(Signal<PropertyState<T>>)
where
  T: 'static;

impl<T> Clone for Property<T> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<T> Copy for Property<T> {}

/// Two handles are equal when they refer to the same property, whatever its value.
impl<T> PartialEq for Property<T> {
  fn eq(&self, other: &Self) -> bool {
    self.0 == other.0
  }
}

impl<T> Eq for Property<T> {}

impl<T> Property<T>
where
  T: PartialEq,
{
  /// Creates a property of the kind `key` whose `Default` level holds `default` and whose other
  /// levels are empty.
  #[track_caller]
  pub fn new(key: &'static PropertyKey<T>, default: T) -> Property<T> {
    Property(Signal::new(PropertyState::new(key, default)))
  }

  /// Creates a property no scope owns, for the tree, which disposes it with its node.
  #[track_caller]
  pub(crate) fn new_unowned(key: &'static PropertyKey<T>, default: T) -> Property<T> {
    Property(Signal::new_unowned(PropertyState::new(key, default)))
  }

  /// The property without its value type, for
  /// [`Object::visit_properties`](crate::Object::visit_properties) to hand to the tree.
  #[must_use]
  pub fn as_ref(&self) -> AnyProperty<'_> {
    AnyProperty::new(self)
  }

  /// Returns the key that identifies this property's kind and holds its metadata.
  ///
  /// The key never changes, so reading it doesn't subscribe the running observer.
  pub fn key(&self) -> &'static PropertyKey<T> {
    self.0.with_untracked(PropertyState::key)
  }

  /// Calls `function` with a reference to the effective value and returns its result.
  ///
  /// Use this instead of [`get`](Property::get) to read a value that isn't `Clone`, or part of one,
  /// without cloning it.
  pub fn with<TResult>(&self, function: impl FnOnce(&T) -> TResult) -> TResult {
    self.0.with(|state| state.levels.with_effective(function))
  }

  /// Calls `function` with a reference to the property's own value: its highest set level, or its
  /// default, without what it inherits. Under [`Inheritance::Combine`] that is what the property
  /// asks for before its ancestors have their say.
  pub fn with_own<TResult>(&self, function: impl FnOnce(&T) -> TResult) -> TResult {
    self.0.with(|state| state.levels.with_own(function))
  }

  /// Returns the level that supplies the effective value.
  ///
  /// Like [`get`](Property::get), this subscribes the running observer. Clearing the winning level
  /// changes the source even when the next level holds an equal value, and readers are marked dirty
  /// then too. Under [`Inheritance::Combine`] it names the property's own level: the ancestor's
  /// part isn't a level, so a `Local` source can still read `false` under a disabled ancestor.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::property::{Affects, Inheritance, Precedence, Property, PropertyKey};
  ///
  /// static WIDTH: PropertyKey<f64> = PropertyKey::new("Width", Affects::LAYOUT, Inheritance::None);
  ///
  /// let width = Property::new(&WIDTH, 0.0);
  /// assert_eq!(width.effective_source(), Precedence::Default);
  ///
  /// width.set_at(Precedence::Binding, 80.0);
  /// assert_eq!(width.effective_source(), Precedence::Binding);
  /// ```
  pub fn effective_source(&self) -> Precedence {
    self.0.with(|state| state.levels.effective_source())
  }

  /// Sets the `Local` level, the level for values set in code or by a UI language.
  pub fn set(&self, value: T) {
    self.set_at(Precedence::Local, value);
  }

  /// Sets the `Default` level: the value the property has while no other level holds one. A
  /// control uses it in `OverrideDefaults::override_defaults` to change a default it doesn't declare.
  pub fn set_default(&self, value: T) {
    self.set_at(Precedence::Default, value);
  }

  /// Sets `level` to `value`.
  ///
  /// If a higher level holds a value, the effective value doesn't change and readers aren't marked
  /// dirty; `value` shows once the levels above it are cleared. Setting a level to the value it
  /// already holds does nothing.
  ///
  /// # Panics
  ///
  /// Panics if `level` is [`Precedence::Inherited`]: that level follows the logical tree, and the
  /// tree sets it.
  pub fn set_at(&self, level: Precedence, value: T) {
    assert_ne!(level, Precedence::Inherited, "the Inherited level follows the logical tree");
    self.edit(|levels| levels.set(level, value));
  }

  /// Empties `level`, so the next level down that holds a value becomes effective.
  ///
  /// Clearing a level that holds nothing does nothing.
  ///
  /// # Panics
  ///
  /// Panics if `level` is [`Precedence::Default`], because a property always has a default value,
  /// or [`Precedence::Inherited`], because the tree sets that level.
  pub fn clear(&self, level: Precedence) {
    assert_ne!(level, Precedence::Default, "the Default level can't be cleared");
    assert_ne!(level, Precedence::Inherited, "the Inherited level follows the logical tree");
    self.edit(|levels| levels.clear(level));
  }

  /// Frees the property's reactive state, as the tree does when it frees the property's node. A
  /// handle kept past that panics when read. A property already disposed, by its scope or an
  /// earlier call, is left as it is.
  fn dispose(self) {
    if !self.0.is_alive() {
      return;
    }
    self.unfollow(self.0.with_untracked(|state| state.levels.inherited()));
    self.0.dispose();
  }

  /// Makes the `Inherited` level follow `source`, or no property, and keeps `source`'s followers
  /// list in step.
  fn follow(&self, source: Option<Property<T>>) {
    let previous = self.0.with_untracked(|state| state.levels.inherited());
    if previous == source {
      return;
    }
    self.unfollow(previous);
    if let Some(source) = source {
      source.0.update_with(|state| {
        state.add_follower(*self);
        Change::Hidden
      });
    }
    self.edit(|levels| levels.set_inherited(source));
  }

  /// Takes the property off `source`'s followers list, if `source` is still alive.
  fn unfollow(self, source: Option<Property<T>>) {
    if let Some(source) = source.filter(|source| source.0.is_alive()) {
      source.0.update_with(|state| {
        state.remove_follower(self);
        Change::Hidden
      });
    }
  }

  /// Applies `edit` to the levels and, if readers can see it, marks the property's node dirty and
  /// the nodes of the followers that inherit the new value.
  fn edit(&self, edit: impl FnOnce(&mut Levels<T>) -> Change) {
    let mut change = Change::Hidden;
    self.0.update_with(|state| {
      change = edit(&mut state.levels);
      if change == Change::Visible {
        state.mark_dirty();
      }
      change
    });
    if change == Change::Visible {
      self.mark_followers_dirty();
    }
  }

  /// Marks dirty the node of every follower whose effective value comes from this property, and
  /// passes the walk on to their own followers. Reads are untracked, so a write inside an effect
  /// doesn't subscribe it to every descendant. Followers whose scope was disposed are dropped.
  fn mark_followers_dirty(&self) {
    let followers = self.0.with_untracked(|state| state.followers().to_vec());
    let mut has_disposed = false;
    for follower in followers {
      if !follower.0.is_alive() {
        has_disposed = true;
        continue;
      }
      let inherits_from_here = follower.0.with_untracked(|state| {
        let inherits = state.levels.combines() || state.levels.effective_source() == Precedence::Inherited;
        if inherits {
          state.mark_dirty();
        }
        inherits
      });
      if inherits_from_here {
        follower.mark_followers_dirty();
      }
    }
    if has_disposed {
      self.0.update_with(|state| {
        state.retain_followers(|follower| follower.0.is_alive());
        Change::Hidden
      });
    }
  }
}

impl<T> Property<T>
where
  T: PartialEq + Clone,
{
  /// Returns a clone of the effective value.
  ///
  /// To read without cloning, use [`with`](Property::with).
  pub fn get(&self) -> T {
    self.with(T::clone)
  }

  /// Returns a clone of the property's own value, without what it inherits.
  ///
  /// To read without cloning, use [`with_own`](Property::with_own).
  pub fn own(&self) -> T {
    self.with_own(T::clone)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::reactive::{Scope, effect, flush_effects};
  use crate::tree::DirtyHandle;
  use pretty_assertions::assert_eq;
  use static_assertions::{assert_impl_all, assert_not_impl_any};
  use std::cell::Cell;
  use std::rc::Rc;

  assert_not_impl_any!(Property<i32>: Send, Sync);
  assert_impl_all!(Property<String>: Copy);

  static TEST: PropertyKey<i32> = PropertyKey::new("Test", Affects::LAYOUT, Inheritance::None);

  /// Every level, lowest first.
  const LEVELS: [Precedence; 6] = [
    Precedence::Default,
    Precedence::Inherited,
    Precedence::StyleSetter,
    Precedence::Binding,
    Precedence::Local,
    Precedence::Animation,
  ];

  /// Puts `value` at `level` of `property`. The Inherited level gets it from a new property to
  /// follow, because only the tree sets that level directly.
  fn put(property: Property<i32>, level: Precedence, value: i32) {
    if level == Precedence::Inherited {
      property.follow(Some(Property::new(&TEST, value)));
    } else {
      property.set_at(level, value);
    }
  }

  /// Empties `level` of `property`, unfollowing for the Inherited level.
  fn take(property: Property<i32>, level: Precedence) {
    if level == Precedence::Inherited {
      property.follow(None);
    } else {
      property.clear(level);
    }
  }

  // Sorting the levels puts them in priority order, lowest first.
  #[test]
  fn precedence_orders_levels_by_priority() {
    let mut levels = [
      Precedence::Local,
      Precedence::Default,
      Precedence::Animation,
      Precedence::StyleSetter,
      Precedence::Inherited,
      Precedence::Binding,
    ];

    levels.sort();

    assert_eq!(
      levels,
      [
        Precedence::Default,
        Precedence::Inherited,
        Precedence::StyleSetter,
        Precedence::Binding,
        Precedence::Local,
        Precedence::Animation,
      ]
    );
  }

  // For each pair of adjacent levels, the higher one's value wins over the lower one's.
  #[test]
  fn higher_level_overrides_lower() {
    for pair in LEVELS.windows(2) {
      let (lower, higher) = (pair[0], pair[1]);
      let property = Property::new(&TEST, -1);

      put(property, lower, 1);
      put(property, higher, 2);

      assert_eq!(property.get(), 2, "{higher:?} over {lower:?}");
    }
  }

  // Clearing the winning level reveals the next level down that holds a value, skipping empty
  // ones.
  #[test]
  fn clear_reverts_to_next_level() {
    let property = Property::new(&TEST, 0);
    property.set_at(Precedence::StyleSetter, 50);
    property.set_at(Precedence::Animation, 200);

    property.clear(Precedence::Animation);
    assert_eq!(property.get(), 50);

    property.clear(Precedence::StyleSetter);
    assert_eq!(property.get(), 0);
  }

  static ENABLED: PropertyKey<bool> = PropertyKey::new("Enabled", Affects::RENDER, Inheritance::Combine(inherit::and));

  // A combining property ANDs its own value with its source's: a source's false wins over any
  // own true, and a source's true leaves the own value.
  #[test]
  fn combine_ands_own_with_source() {
    let source = Property::new(&ENABLED, true);
    let property = Property::new(&ENABLED, true);
    property.follow(Some(source));
    property.set(true);

    source.set(false);
    assert!(!property.get());
    source.set(true);
    assert!(property.get());
    property.set(false);
    assert!(!property.get());
  }

  // A combining property's default still counts while it follows a source.
  #[test]
  fn combine_default_is_visible_under_a_source() {
    let source = Property::new(&ENABLED, true);
    let property = Property::new(&ENABLED, true);
    property.follow(Some(source));

    property.set_default(false);

    assert!(!property.get());
  }

  // own reads the property's levels and default, without what it inherits.
  #[test]
  fn own_ignores_the_source() {
    let source = Property::new(&ENABLED, false);
    let property = Property::new(&ENABLED, true);
    property.follow(Some(source));

    assert_eq!((property.get(), property.own()), (false, true));
  }

  // A value set as the default shows only while no other level holds one.
  #[test]
  fn set_default_loses_to_every_other_level() {
    for level in &LEVELS[1..] {
      let property = Property::new(&TEST, 0);

      property.set_default(5);
      assert_eq!(property.get(), 5);
      put(property, *level, 9);

      assert_eq!(property.get(), 9, "{level:?} over the default");
    }
  }

  // An effect reading the property reruns once per change of the effective value, and not for a
  // write to a level below the winning one.
  #[test]
  fn effect_on_property_runs_once_per_effective_change() {
    let width = Property::new(&TEST, 0);
    let runs = Rc::new(Cell::new(0));
    let runs_in_effect = Rc::clone(&runs);
    effect(move || {
      runs_in_effect.set(runs_in_effect.get() + 1);
      width.get();
    });

    width.set(100);
    flush_effects();
    assert_eq!(runs.get(), 2, "set at Local changes the effective value");

    width.set_at(Precedence::StyleSetter, 50);
    flush_effects();
    assert_eq!(runs.get(), 2, "Local still wins, so nothing visible changed");

    width.clear(Precedence::Local);
    flush_effects();
    assert_eq!(runs.get(), 3, "the style value shows through");
    assert_eq!(width.get(), 50);
  }

  // Setting a level to the value it already holds doesn't rerun readers.
  #[test]
  fn setting_the_same_value_does_not_rerun_readers() {
    let width = Property::new(&TEST, 0);
    width.set(10);
    let runs = Rc::new(Cell::new(0));
    let runs_in_effect = Rc::clone(&runs);
    effect(move || {
      runs_in_effect.set(runs_in_effect.get() + 1);
      width.get();
    });

    width.set(10);
    flush_effects();

    assert_eq!(runs.get(), 1, "only the run on creation");
  }

  // Clearing the Default level panics: a property always has a default value.
  #[test]
  #[should_panic(expected = "the Default level can't be cleared")]
  fn clear_default_is_rejected() {
    Property::new(&TEST, 0).clear(Precedence::Default);
  }

  // `effective_source` names the highest level that holds a value, following sets and clears.
  #[test]
  fn effective_source_reports_level() {
    let property = Property::new(&TEST, 0);
    assert_eq!(property.effective_source(), Precedence::Default);

    for level in &LEVELS[1..] {
      put(property, *level, 1);
      assert_eq!(property.effective_source(), *level);
    }
    for pair in LEVELS.windows(2).rev() {
      take(property, pair[1]);
      assert_eq!(property.effective_source(), pair[0]);
    }
  }

  // Clearing the winning level wakes a reader of `effective_source` even when the level below holds
  // an equal value, because the source changed.
  #[test]
  fn falling_back_to_an_equal_value_still_updates_the_source() {
    let width = Property::new(&TEST, 0);
    width.set_at(Precedence::StyleSetter, 10);
    width.set(10);
    let seen = Rc::new(Cell::new(Precedence::Default));
    let seen_in_effect = Rc::clone(&seen);
    effect(move || seen_in_effect.set(width.effective_source()));

    width.clear(Precedence::Local);
    flush_effects();

    assert_eq!(seen.get(), Precedence::StyleSetter);
    assert_eq!(width.get(), 10);
  }

  // A property reports the key it was created with: the same static, so the same address.
  #[test]
  fn property_reports_its_key() {
    let property = Property::new(&TEST, 0);

    assert!(std::ptr::eq(property.key(), &TEST));
    assert_eq!(property.key().affects(), Affects::LAYOUT);
    assert!(!property.key().inherits());
  }

  // Combining `Affects` values keeps every stage, and `contains` checks each one.
  #[test]
  fn affects_combines_stages() {
    let mut affects = Affects::NONE;
    affects |= Affects::RENDER;

    assert!(affects.contains(Affects::RENDER));
    assert!(!affects.contains(Affects::LAYOUT));
    assert!((affects | Affects::LAYOUT).contains(Affects::LAYOUT | Affects::RENDER));
    assert!(Affects::NONE.is_empty() && !affects.is_empty());
  }

  // Inheritance

  // A property following another reads its effective value through the link, so a change to the
  // source shows at once, with no flush.
  #[test]
  fn inherited_level_reads_through_the_link() {
    let parent = Property::new(&TEST, 0);
    let child = Property::new(&TEST, 0);
    parent.set(20);

    child.follow(Some(parent));
    assert_eq!(child.get(), 20);
    assert_eq!(child.effective_source(), Precedence::Inherited);

    parent.set(24);
    assert_eq!(child.get(), 24);
  }

  // A value set above the Inherited level wins over the followed property.
  #[test]
  fn local_value_wins_over_the_link() {
    let parent = Property::new(&TEST, 20);
    let child = Property::new(&TEST, 0);
    child.follow(Some(parent));

    child.set(12);
    parent.set(30);

    assert_eq!(child.get(), 12);
  }

  // An effect reading a following property reruns when the followed property changes.
  #[test]
  fn effect_on_follower_reruns_when_the_source_changes() {
    let parent = Property::new(&TEST, 20);
    let child = Property::new(&TEST, 0);
    child.follow(Some(parent));
    let seen = Rc::new(Cell::new(0));
    let seen_in_effect = Rc::clone(&seen);
    effect(move || seen_in_effect.set(child.get()));

    parent.set(24);
    flush_effects();

    assert_eq!(seen.get(), 24);
  }

  // A visible change marks dirty the nodes of followers that inherit it, through every level of
  // followers, and stops at a follower with its own value.
  #[test]
  fn visible_change_marks_inheriting_followers_dirty() {
    let [root, middle, leaf, blocked] = [(); 4].map(|_| Property::new(&TEST, 0));
    let [middle_dirty, leaf_dirty, blocked_dirty] = [(); 3].map(|_| DirtyHandle::default());
    middle.attach(middle_dirty.clone());
    leaf.attach(leaf_dirty.clone());
    blocked.attach(blocked_dirty.clone());
    middle.follow(Some(root));
    leaf.follow(Some(middle));
    blocked.follow(Some(middle));
    blocked.set(5);
    for dirty in [&middle_dirty, &leaf_dirty, &blocked_dirty] {
      dirty.clear(Affects::LAYOUT | Affects::RENDER);
    }

    root.set(1);

    assert_eq!(middle_dirty.stages(), Affects::LAYOUT);
    assert_eq!(leaf_dirty.stages(), Affects::LAYOUT);
    assert_eq!(blocked_dirty.stages(), Affects::NONE);
  }

  // A follower whose scope was disposed is skipped and dropped from the followers list.
  #[test]
  fn disposed_follower_is_dropped() {
    let parent = Property::new(&TEST, 0);
    let scope = Scope::new_root();
    let child = scope.run(|| Property::new(&TEST, 0));
    child.follow(Some(parent));

    scope.dispose();
    parent.set(1);

    assert_eq!(parent.0.with_untracked(|state| state.followers().len()), 0);
  }

  // Following another property moves the follower from the old source's list to the new one's.
  #[test]
  fn refollowing_moves_the_follower() {
    let [first, second, child] = [(); 3].map(|_| Property::new(&TEST, 0));
    child.follow(Some(first));

    child.follow(Some(second));

    assert_eq!(first.0.with_untracked(|state| state.followers().len()), 0);
    assert_eq!(second.0.with_untracked(|state| state.followers().to_vec()), [child]);
  }

  // Writing the Inherited level directly panics: the tree sets it.
  #[test]
  #[should_panic(expected = "the Inherited level follows the logical tree")]
  fn set_at_inherited_is_rejected() {
    Property::new(&TEST, 0).set_at(Precedence::Inherited, 1);
  }

  // Starting or stopping to follow a source is a visible change while nothing above the Inherited
  // level is set: readers rerun and the node is marked dirty.
  #[test]
  fn following_or_unfollowing_a_source_is_visible() {
    let parent = Property::new(&TEST, 20);
    let child = Property::new(&TEST, 0);
    let dirty = DirtyHandle::default();
    child.attach(dirty.clone());
    let seen = Rc::new(Cell::new(-1));
    let seen_in_effect = Rc::clone(&seen);
    effect(move || seen_in_effect.set(child.get()));

    child.follow(Some(parent));
    flush_effects();
    assert_eq!((seen.get(), dirty.stages()), (20, Affects::LAYOUT));

    dirty.clear(Affects::LAYOUT | Affects::RENDER);
    child.follow(None);
    flush_effects();
    assert_eq!((seen.get(), dirty.stages()), (0, Affects::LAYOUT));
  }
}

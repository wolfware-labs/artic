use std::fmt::Debug;
use std::mem::replace;
use std::num::NonZeroU32;

type SlotIndex = u32;
type SlotGeneration = NonZeroU32;

#[derive(Debug)]
pub struct Arena<TValue> {
  slots: Vec<Slot<TValue>>,
  first_free: Option<SlotIndex>,
  occupied_slots: usize,
}

/// Storage cell of an [`Arena`].
#[derive(Debug)]
enum Slot<TValue> {
  /// An empty slot, linked into the arena's free list through `next_free`.
  Free {
    generation: SlotGeneration,
    next_free: Option<SlotIndex>,
  },
  /// A slot holding a live value.
  Occupied { generation: SlotGeneration, value: TValue },
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct SlotId {
  index: SlotIndex,
  generation: SlotGeneration,
}

impl<TValue> Default for Arena<TValue> {
  fn default() -> Self {
    Self::new()
  }
}

impl<TValue> Arena<TValue> {
  #[must_use]
  pub const fn new() -> Self {
    Self {
      slots: Vec::new(),
      occupied_slots: 0,
      first_free: None,
    }
  }

  #[must_use = "slot ID needed to get or free the value from the arena"]
  pub fn alloc(&mut self, value: TValue) -> SlotId {
    let Some(index) = self.first_free else {
      return self.alloc_new_slot(value);
    };

    let Some(slot) = self.slots.get_mut(index as usize) else {
      panic!("slot free index out of bounds");
    };

    let Slot::Free { next_free, generation } = slot else {
      panic!("slot free index does not belong to a free slot");
    };

    let generation = *generation;
    self.first_free = *next_free;
    *slot = Slot::Occupied { generation, value };
    self.occupied_slots += 1;
    SlotId { index, generation }
  }

  pub fn free(&mut self, id: SlotId) -> Option<TValue> {
    let slot = self.slots.get_mut(id.index as usize)?;

    let Slot::Occupied { generation, .. } = &slot else {
      return None;
    };

    if *generation != id.generation {
      return None;
    }

    self.occupied_slots -= 1;

    let old_slot = replace(
      slot,
      Slot::Free {
        generation: generation.checked_add(1).unwrap_or(SlotGeneration::MIN),
        next_free: self.first_free,
      },
    );

    let Slot::Occupied { value, .. } = old_slot else {
      unreachable!("checked above that the slot is occupied");
    };

    self.first_free = Some(id.index);

    Some(value)
  }

  pub fn get(&self, id: SlotId) -> Option<&TValue> {
    let Slot::Occupied { generation, value } = self.slots.get(id.index as usize)? else {
      return None;
    };

    if generation == &id.generation { Some(value) } else { None }
  }

  pub fn get_mut(&mut self, id: SlotId) -> Option<&mut TValue> {
    let Slot::Occupied { generation, value } = self.slots.get_mut(id.index as usize)? else {
      return None;
    };

    if generation == &id.generation { Some(value) } else { None }
  }

  pub fn len(&self) -> usize {
    self.occupied_slots
  }

  pub fn is_empty(&self) -> bool {
    self.occupied_slots == 0
  }

  pub fn contains(&self, id: SlotId) -> bool {
    self.get(id).is_some()
  }

  /// The ids of the occupied slots, in slot order.
  pub fn ids(&self) -> impl Iterator<Item = SlotId> + '_ {
    self.slots.iter().zip(0..).filter_map(|(slot, index)| match slot {
      Slot::Occupied { generation, .. } => Some(SlotId {
        index,
        generation: *generation,
      }),
      Slot::Free { .. } => None,
    })
  }

  fn alloc_new_slot(&mut self, value: TValue) -> SlotId {
    let Ok(index) = self.slots.len().try_into() else {
      panic!("the arena capacity is full");
    };
    let generation = SlotGeneration::MIN;
    let new_slot = Slot::Occupied { generation, value };
    self.slots.push(new_slot);
    self.occupied_slots += 1;
    SlotId { index, generation }
  }
}

/// Values stored under ids an [`Arena`] issued, at most one per id.
///
/// It never issues ids: a value goes in under an id from an arena and is found only by that exact
/// id. Once the arena reuses the slot, the new id finds nothing until a value is inserted for it,
/// and the old id finds nothing after that. Whether an id is still live is the arena's to say: a
/// value stays until it's removed or replaced.
#[derive(Debug)]
pub struct SlotMap<TValue> {
  entries: Vec<Option<(SlotGeneration, TValue)>>,
}

impl<TValue> Default for SlotMap<TValue> {
  fn default() -> Self {
    Self::new()
  }
}

impl<TValue> SlotMap<TValue> {
  #[must_use]
  pub const fn new() -> Self {
    Self { entries: Vec::new() }
  }

  pub fn insert(&mut self, id: SlotId, value: TValue) {
    let index = id.index as usize;
    if index >= self.entries.len() {
      self.entries.resize_with(index + 1, || None);
    }
    self.entries[index] = Some((id.generation, value));
  }

  pub fn get(&self, id: SlotId) -> Option<&TValue> {
    match self.entries.get(id.index as usize)? {
      Some((generation, value)) if *generation == id.generation => Some(value),
      _ => None,
    }
  }

  pub fn get_mut(&mut self, id: SlotId) -> Option<&mut TValue> {
    match self.entries.get_mut(id.index as usize)? {
      Some((generation, value)) if *generation == id.generation => Some(value),
      _ => None,
    }
  }

  pub fn remove(&mut self, id: SlotId) -> Option<TValue> {
    let entry = self.entries.get_mut(id.index as usize)?;
    if entry.as_ref().is_some_and(|(generation, _)| *generation == id.generation) {
      entry.take().map(|(_, value)| value)
    } else {
      None
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::{assert_eq, assert_ne};
  use static_assertions::{assert_eq_size, assert_impl_all};
  use std::rc::Rc;

  assert_eq_size!(SlotId, Option<SlotId>);
  assert_eq_size!(SlotId, u64);
  assert_impl_all!(SlotId: Copy, Send, Sync);

  #[derive(Debug, PartialEq, Eq)]
  struct Item;

  #[test]
  fn stale_id_after_free_returns_none() {
    let mut arena = Arena::new();
    let n1 = arena.alloc(Item);
    arena.free(n1);
    assert!(arena.get(n1).is_none());
  }

  #[test]
  fn generation_distinguishes_reused_slots() {
    let mut arena = Arena::new();
    let mut control = Item;
    let n1 = arena.alloc(control);
    control = arena.free(n1).unwrap();
    let n2 = arena.alloc(control);
    assert!(arena.get(n1).is_none());
    assert_eq!(arena.get(n2), Some(&Item));
    assert_eq!(n1.index, n2.index);
    assert_ne!(n2.generation, n1.generation);
  }

  #[test]
  fn stale_id_does_not_resolve_after_slot_reuse() {
    let mut arena = Arena::new();
    let n1 = arena.alloc(Item);
    arena.free(n1).unwrap();
    let n2 = arena.alloc(Item);
    assert!(arena.get(n1).is_none());
    assert_eq!(n2.index, n1.index);
  }

  #[test]
  fn len_tracks_allocs_and_frees() {
    let mut arena = Arena::new();
    assert_eq!(arena.len(), 0);
    let n1 = arena.alloc(Item);
    assert_eq!(arena.len(), 1);
    let n2 = arena.alloc(Item);
    assert_eq!(arena.len(), 2);
    assert!(arena.free(n1).is_some());
    assert_eq!(arena.len(), 1);
    assert!(arena.free(n2).is_some());
    assert_eq!(arena.len(), 0);
  }

  #[test]
  fn double_free_returns_none_and_keeps_len() {
    let mut arena = Arena::new();
    let n1 = arena.alloc(Item);
    let _ = arena.alloc(Item);
    assert!(arena.free(n1).is_some());
    assert!(arena.free(n1).is_none());
    assert_eq!(arena.len(), 1);
  }

  #[test]
  fn generation_wraps_from_max_to_min() {
    let mut arena = Arena::new();
    let mut n1 = arena.alloc(Item);
    n1.generation = SlotGeneration::MAX;
    if let Slot::Occupied { generation, .. } = &mut arena.slots[n1.index as usize] {
      *generation = SlotGeneration::MAX;
    } else {
      panic!("test setup: the new node's slot should be occupied");
    }
    assert!(arena.free(n1).is_some());
    let n2 = arena.alloc(Item);
    assert_eq!(n2.index, n1.index);
    assert_eq!(n2.generation, SlotGeneration::MIN);
  }

  #[test]
  fn out_of_range_id_returns_none() {
    let mut arena = Arena::new();
    let mut n1 = arena.alloc(Item);
    n1.index = 1;

    assert!(arena.free(n1).is_none());
    assert!(arena.get(n1).is_none());
    assert!(arena.get_mut(n1).is_none());
  }

  #[test]
  fn len_is_correct_after_slot_reuse() {
    let mut arena = Arena::new();
    let n1 = arena.alloc(Item);
    let _ = arena.alloc(Item);
    arena.free(n1);
    let _ = arena.alloc(Item);
    assert_eq!(arena.len(), 2);
  }

  #[test]
  fn is_not_empty_after_alloc() {
    let mut arena = Arena::new();
    let _ = arena.alloc(Item);
    assert!(!arena.is_empty());
  }

  #[test]
  fn new_and_default_arenas_are_empty() {
    let new: Arena<Item> = Arena::new();
    let default: Arena<Item> = Arena::default();
    assert!(new.is_empty());
    assert_eq!(new.len(), 0);
    assert!(default.is_empty());
    assert_eq!(default.len(), 0);
  }

  #[test]
  fn get_returns_the_value_for_each_id() {
    let mut arena = Arena::new();
    let n1 = arena.alloc("first");
    let n2 = arena.alloc("second");
    assert_ne!(n1, n2);
    assert_eq!(arena.get(n1), Some(&"first"));
    assert_eq!(arena.get(n2), Some(&"second"));
  }

  #[test]
  fn get_mut_changes_are_visible_through_get() {
    let mut arena = Arena::new();
    let n1 = arena.alloc(1);
    *arena.get_mut(n1).unwrap() += 41;
    assert_eq!(arena.get(n1), Some(&42));
  }

  // `ids` lists the occupied slots and skips freed ones.
  #[test]
  fn ids_lists_the_occupied_slots() {
    let mut arena = Arena::new();
    let [first, second, third] = [1, 2, 3].map(|value| arena.alloc(value));
    arena.free(second);
    assert_eq!(arena.ids().collect::<Vec<_>>(), [first, third]);
  }

  #[test]
  fn free_returns_the_stored_value() {
    let mut arena = Arena::new();
    let n1 = arena.alloc(String::from("hello"));
    assert_eq!(arena.free(n1), Some(String::from("hello")));
  }

  #[test]
  fn contains_tracks_liveness() {
    let mut arena = Arena::new();
    let n1 = arena.alloc(Item);
    assert!(arena.contains(n1));
    arena.free(n1);
    assert!(!arena.contains(n1));
    let n2 = arena.alloc(Item);
    assert!(!arena.contains(n1));
    assert!(arena.contains(n2));
  }

  #[test]
  fn stale_id_get_mut_returns_none_after_slot_reuse() {
    let mut arena = Arena::new();
    let n1 = arena.alloc(1);
    arena.free(n1);
    let _ = arena.alloc(2);
    assert!(arena.get_mut(n1).is_none());
  }

  // A stale id must not free the newer value that reused its slot.
  #[test]
  fn stale_free_after_slot_reuse_leaves_new_value() {
    let mut arena = Arena::new();
    let n1 = arena.alloc(1);
    arena.free(n1);
    let n2 = arena.alloc(2);
    assert!(arena.free(n1).is_none());
    assert_eq!(arena.get(n2), Some(&2));
    assert_eq!(arena.len(), 1);
  }

  #[test]
  fn alloc_reuses_the_most_recently_freed_slot() {
    let mut arena = Arena::new();
    let n1 = arena.alloc(Item);
    let n2 = arena.alloc(Item);
    arena.free(n1);
    arena.free(n2);
    assert_eq!(arena.alloc(Item).index, n2.index);
    assert_eq!(arena.alloc(Item).index, n1.index);
  }

  #[test]
  fn freed_slots_are_reused_before_storage_grows() {
    let mut arena = Arena::new();
    let ids: Vec<SlotId> = (0..4).map(|_| arena.alloc(Item)).collect();
    for id in &ids {
      arena.free(*id);
    }
    for _ in 0..4 {
      let _ = arena.alloc(Item);
    }
    assert_eq!(arena.slots.len(), 4);
    let _ = arena.alloc(Item);
    assert_eq!(arena.slots.len(), 5);
  }

  // If a double free pushed the slot onto the free list twice, two allocations would share a slot.
  #[test]
  fn double_free_does_not_hand_out_the_same_slot_twice() {
    let mut arena = Arena::new();
    let n1 = arena.alloc(1);
    arena.free(n1);
    arena.free(n1);
    let n2 = arena.alloc(2);
    let n3 = arena.alloc(3);
    assert_ne!(n2.index, n3.index);
    assert_eq!(arena.get(n2), Some(&2));
    assert_eq!(arena.get(n3), Some(&3));
  }

  #[test]
  fn dropping_the_arena_drops_live_values() {
    let value = Rc::new(());
    let mut arena = Arena::new();
    let _ = arena.alloc(Rc::clone(&value));
    let freed = arena.alloc(Rc::clone(&value));
    drop(arena.free(freed));
    assert_eq!(Rc::strong_count(&value), 2);
    drop(arena);
    assert_eq!(Rc::strong_count(&value), 1);
  }

  // A value is found by the id it was inserted under, and by nothing else: not the id of the node
  // that reuses the slot, and not the old id once that node has its own value.
  #[test]
  fn slot_map_finds_values_by_exact_id() {
    let mut arena = Arena::new();
    let mut map = SlotMap::new();
    let button = arena.alloc(Item);
    map.insert(button, "button");
    assert_eq!(map.get(button), Some(&"button"));

    arena.free(button);
    let label = arena.alloc(Item);
    assert_eq!(label.index, button.index);
    assert_eq!(map.get(label), None);

    map.insert(label, "label");
    assert_eq!(map.get(label), Some(&"label"));
    assert_eq!(map.get(button), None);
  }

  // Writing through an id reaches only that id's value: the id of the node that reused the slot
  // finds nothing to write.
  #[test]
  fn slot_map_writes_only_by_exact_id() {
    let mut arena = Arena::new();
    let mut map = SlotMap::new();
    let button = arena.alloc(Item);
    map.insert(button, "button");
    arena.free(button);
    let label = arena.alloc(Item);

    assert_eq!(map.get_mut(label), None);
    assert_eq!(map.get_mut(button), Some(&mut "button"));
  }

  // Removing takes the value out only for its own id.
  #[test]
  fn slot_map_removes_only_its_own_id() {
    let mut arena = Arena::new();
    let mut map = SlotMap::new();
    let button = arena.alloc(Item);
    map.insert(button, 1);
    arena.free(button);
    let label = arena.alloc(Item);

    assert_eq!(map.remove(label), None);
    assert_eq!(map.remove(button), Some(1));
    assert_eq!(map.get(button), None);
  }

  // Ids can be inserted in any order: the map grows to fit the highest slot and leaves the gaps
  // empty.
  #[test]
  fn slot_map_grows_to_fit_any_slot() {
    let mut arena = Arena::new();
    let mut map = SlotMap::new();
    let ids = [(); 3].map(|()| arena.alloc(Item));
    map.insert(ids[2], 'c');

    *map.get_mut(ids[2]).unwrap() = 'C';

    assert_eq!(map.get(ids[2]), Some(&'C'));
    assert_eq!(map.get(ids[0]), None);
    assert_eq!(map.get_mut(ids[1]), None);
  }
}

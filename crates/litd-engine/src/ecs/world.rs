use std::{
    any::{TypeId, type_name},
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::EngineError;

use super::{
    Entity,
    entity::{EntityLocation, EntitySlot},
    storage::{Archetype, Column, Component, ComponentBundle, Event, validate_and_sort},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchetypeInfo {
    pub entity_count: usize,
    pub component_names: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldSlotStats {
    pub slot_count: usize,
    pub free_slot_count: usize,
    pub alive_entities: usize,
    pub archetype_count: usize,
    pub current_tick: u64,
}

#[derive(Default)]
pub struct World {
    slots: Vec<EntitySlot>,
    free_slots: Vec<u32>,
    archetypes: Vec<Archetype>,
    events: HashMap<TypeId, Box<dyn EventQueueDyn>>,
    alive_entities: usize,
    tick: u64,
}

impl World {
    pub fn spawn<B: ComponentBundle>(&mut self, bundle: B) -> Result<Entity, EngineError> {
        let mut entries = bundle.into_entries();
        validate_and_sort(&mut entries)?;
        let entity = self.allocate_entity()?;
        let archetype = self.archetype_for(&entries);
        let tick = self.tick.saturating_add(1);
        let row = match self.archetypes[archetype].push(entity, entries, tick) {
            Ok(row) => row,
            Err(error) => {
                self.release_unspawned_entity(entity);
                return Err(error);
            }
        };
        self.tick = tick;
        self.slots[entity.index() as usize].location = Some(EntityLocation { archetype, row });
        self.alive_entities += 1;
        Ok(entity)
    }

    pub fn despawn(&mut self, entity: Entity) -> Result<(), EngineError> {
        let location = self.location(entity)?;
        let slot_index = entity.index() as usize;
        let next_generation = self.slots[slot_index].generation.checked_add(1).ok_or(
            EngineError::EcsEntityGenerationOverflow {
                index: entity.index(),
                generation: entity.generation(),
            },
        )?;
        self.bump_tick();
        self.archetypes[location.archetype].swap_remove(location.row);
        if location.row < self.archetypes[location.archetype].entities.len() {
            let moved = self.archetypes[location.archetype].entities[location.row];
            self.slots[moved.index() as usize].location = Some(EntityLocation {
                archetype: location.archetype,
                row: location.row,
            });
        }
        let slot = &mut self.slots[slot_index];
        slot.generation = next_generation;
        slot.location = None;
        self.free_slots.push(entity.index());
        self.alive_entities -= 1;
        Ok(())
    }

    pub fn get<T: Component>(&self, entity: Entity) -> Result<&T, EngineError> {
        let location = self.location(entity)?;
        self.archetypes[location.archetype]
            .column::<T>()
            .map(|column| column.value(location.row))
            .ok_or_else(|| missing_component::<T>(entity))
    }

    pub fn get_mut<T: Component>(&mut self, entity: Entity) -> Result<&mut T, EngineError> {
        let location = self.location(entity)?;
        let tick = self.bump_tick();
        let column = self.archetypes[location.archetype]
            .column_mut::<T>()
            .ok_or_else(|| missing_component::<T>(entity))?;
        column.set_changed_tick(location.row, tick);
        Ok(column.value_mut(location.row))
    }

    pub fn for_each<T: Component>(&self, mut f: impl FnMut(Entity, &T)) -> usize {
        let mut count = 0;
        for archetype in &self.archetypes {
            if let Some(column) = archetype.column::<T>() {
                for (row, entity) in archetype.entities.iter().copied().enumerate() {
                    f(entity, column.value(row));
                    count += 1;
                }
            }
        }
        count
    }

    pub fn for_each_mut<T: Component>(&mut self, mut f: impl FnMut(Entity, &mut T)) -> usize {
        let tick = self.bump_tick();
        let mut count = 0;
        for archetype in &mut self.archetypes {
            if let Some((entities, column)) = archetype.entities_and_column_mut::<T>() {
                for (row, entity) in entities.iter().copied().enumerate() {
                    column.set_changed_tick(row, tick);
                    f(entity, column.value_mut(row));
                    count += 1;
                }
            }
        }
        count
    }

    pub fn for_each2<A: Component, B: Component>(
        &self,
        mut f: impl FnMut(Entity, &A, &B),
    ) -> Result<usize, EngineError> {
        reject_duplicate_query::<A, B>()?;
        let mut count = 0;
        for archetype in &self.archetypes {
            if let (Some(a), Some(b)) = (archetype.column::<A>(), archetype.column::<B>()) {
                for (row, entity) in archetype.entities.iter().copied().enumerate() {
                    f(entity, a.value(row), b.value(row));
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    pub fn for_each2_mut<A: Component, B: Component>(
        &mut self,
        mut f: impl FnMut(Entity, &mut A, &B),
    ) -> Result<usize, EngineError> {
        reject_duplicate_query::<A, B>()?;
        let tick = self.bump_tick();
        let mut count = 0;
        for archetype in &mut self.archetypes {
            if let Some((entities, a, b)) = archetype.entities_and_two_columns_mut::<A, B>() {
                count += run_query2_mut(entities, a, b, tick, &mut f);
            }
        }
        Ok(count)
    }

    pub fn for_each2_mut_parallel<A, B, F>(&mut self, f: F) -> Result<usize, EngineError>
    where
        A: Component,
        B: Component,
        F: Fn(Entity, &mut A, &B) + Sync,
    {
        reject_duplicate_query::<A, B>()?;
        let tick = self.bump_tick();
        let total = AtomicUsize::new(0);
        let f = &f;
        // Fan the per-archetype work out over Rayon's persistent, process-wide
        // work-stealing pool instead of `std::thread::scope`, which launched a fresh
        // OS thread per archetype **per call** (issue #1524 item 1). Rayon's pool is
        // created once (lazily) and its worker threads live for the process, so a hot
        // per-tick query reuses them and never pays thread-creation cost again. The
        // pool is shared, but each archetype is still visited exactly once on disjoint
        // `&mut` data and the result is an order-independent atomic count, so the
        // output is identical and determinism is preserved.
        // Ref: rayon `ThreadPool` docs (work-stealing, long-lived workers) — a shared
        // pool "avoids the per-task cost of `std::thread::spawn`".
        rayon::scope(|scope| {
            for archetype in &mut self.archetypes {
                let total = &total;
                scope.spawn(move |_| {
                    if let Some((entities, a, b)) = archetype.entities_and_two_columns_mut::<A, B>()
                    {
                        total.fetch_add(
                            run_query2_mut(entities, a, b, tick, &mut |entity, a, b| {
                                f(entity, a, b);
                            }),
                            Ordering::Relaxed,
                        );
                    }
                });
            }
        });
        Ok(total.load(Ordering::Relaxed))
    }

    pub fn component_changed_since<T: Component>(
        &self,
        entity: Entity,
        since_tick: u64,
    ) -> Result<bool, EngineError> {
        let location = self.location(entity)?;
        let column = self.archetypes[location.archetype]
            .column::<T>()
            .ok_or_else(|| missing_component::<T>(entity))?;
        Ok(column.changed_tick(location.row) > since_tick)
    }

    pub fn send_event<T: Event>(&mut self, event: T) {
        self.bump_tick();
        let queue = self
            .events
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(EventQueue::<T>::default()));
        queue
            .as_any_mut()
            .downcast_mut::<EventQueue<T>>()
            .expect("event queue type must match TypeId")
            .events
            .push(event);
    }

    pub fn drain_events<T: Event>(&mut self) -> Vec<T> {
        self.events
            .get_mut(&TypeId::of::<T>())
            .and_then(|queue| queue.as_any_mut().downcast_mut::<EventQueue<T>>())
            .map(|queue| std::mem::take(&mut queue.events))
            .unwrap_or_default()
    }

    pub fn pending_event_count<T: Event>(&self) -> usize {
        self.events
            .get(&TypeId::of::<T>())
            .and_then(|queue| queue.as_any().downcast_ref::<EventQueue<T>>())
            .map(|queue| queue.events.len())
            .unwrap_or(0)
    }

    pub fn current_tick(&self) -> u64 {
        self.tick
    }

    pub fn len(&self) -> usize {
        self.alive_entities
    }

    pub fn is_empty(&self) -> bool {
        self.alive_entities == 0
    }

    pub fn archetype_count(&self) -> usize {
        self.archetypes.len()
    }

    pub fn archetype_infos(&self) -> Vec<ArchetypeInfo> {
        self.archetypes
            .iter()
            .map(|archetype| ArchetypeInfo {
                entity_count: archetype.entities.len(),
                component_names: archetype.component_names().to_vec(),
            })
            .collect()
    }

    pub fn slot_stats(&self) -> WorldSlotStats {
        WorldSlotStats {
            slot_count: self.slots.len(),
            free_slot_count: self.free_slots.len(),
            alive_entities: self.alive_entities,
            archetype_count: self.archetypes.len(),
            current_tick: self.tick,
        }
    }

    fn allocate_entity(&mut self) -> Result<Entity, EngineError> {
        if let Some(&index) = self.free_slots.last() {
            let Some(slot) = self.slots.get(index as usize) else {
                return Err(EngineError::EcsFreeListCorrupt {
                    index,
                    reason: "free-list index is outside the slot table".to_string(),
                });
            };
            if slot.location.is_some() {
                return Err(EngineError::EcsFreeListCorrupt {
                    index,
                    reason: "free-list index points at a live entity slot".to_string(),
                });
            }
            self.free_slots.pop();
            return Ok(Entity::new(index, slot.generation));
        }
        if self.slots.len() > u32::MAX as usize {
            return Err(EngineError::EcsEntityIndexOverflow {
                slot_count: self.slots.len(),
            });
        }
        let index = self.slots.len() as u32;
        self.slots.push(EntitySlot {
            generation: 0,
            location: None,
        });
        Ok(Entity::new(index, 0))
    }

    fn release_unspawned_entity(&mut self, entity: Entity) {
        if let Some(slot) = self.slots.get(entity.index() as usize)
            && slot.location.is_none()
            && slot.generation == entity.generation()
        {
            self.free_slots.push(entity.index());
        }
    }

    #[cfg(test)]
    pub(crate) fn issue1331_set_slot_generation_for_test(
        &mut self,
        entity: Entity,
        generation: u32,
    ) {
        self.slots[entity.index() as usize].generation = generation;
    }

    #[cfg(test)]
    pub(crate) fn issue1331_entity_for_slot_for_test(&self, index: u32) -> Entity {
        Entity::new(index, self.slots[index as usize].generation)
    }

    #[cfg(test)]
    pub(crate) fn issue1331_push_free_slot_for_test(&mut self, index: u32) {
        self.free_slots.push(index);
    }

    fn archetype_for(&mut self, entries: &[super::storage::ComponentEntry]) -> usize {
        if let Some(index) = self
            .archetypes
            .iter()
            .position(|archetype| archetype.matches(entries))
        {
            return index;
        }
        self.archetypes.push(Archetype::new(entries));
        self.archetypes.len() - 1
    }

    fn location(&self, entity: Entity) -> Result<EntityLocation, EngineError> {
        let Some(slot) = self.slots.get(entity.index() as usize) else {
            return Err(entity_not_alive(entity));
        };
        if slot.generation != entity.generation() {
            return Err(entity_not_alive(entity));
        }
        slot.location.ok_or_else(|| entity_not_alive(entity))
    }

    fn bump_tick(&mut self) -> u64 {
        self.tick = self.tick.saturating_add(1);
        self.tick
    }
}

fn run_query2_mut<A: Component, B: Component>(
    entities: &[Entity],
    a: &mut Column<A>,
    b: &Column<B>,
    tick: u64,
    f: &mut impl FnMut(Entity, &mut A, &B),
) -> usize {
    for (row, entity) in entities.iter().copied().enumerate() {
        a.set_changed_tick(row, tick);
        f(entity, a.value_mut(row), b.value(row));
    }
    entities.len()
}

fn reject_duplicate_query<A: Component, B: Component>() -> Result<(), EngineError> {
    if TypeId::of::<A>() == TypeId::of::<B>() {
        return Err(EngineError::EcsQueryDuplicateComponent {
            component: type_name::<A>(),
        });
    }
    Ok(())
}

fn missing_component<T: Component>(entity: Entity) -> EngineError {
    EngineError::EcsMissingComponent {
        entity: entity.to_string(),
        component: type_name::<T>(),
    }
}

fn entity_not_alive(entity: Entity) -> EngineError {
    EngineError::EcsEntityNotAlive {
        index: entity.index(),
        generation: entity.generation(),
    }
}

trait EventQueueDyn: Send + Sync {
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

struct EventQueue<T: Event> {
    events: Vec<T>,
}

impl<T: Event> Default for EventQueue<T> {
    fn default() -> Self {
        Self { events: Vec::new() }
    }
}

impl<T: Event> EventQueueDyn for EventQueue<T> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct BoundaryMarker;

    #[test]
    fn issue1331_generation_overflow_fails_before_mutation() {
        let mut world = World::default();
        let entity = world.spawn((BoundaryMarker,)).unwrap();
        world.slots[entity.index() as usize].generation = u32::MAX;
        let max_generation_entity = Entity::new(entity.index(), u32::MAX);
        let before = world.slot_stats();

        let error = world.despawn(max_generation_entity).unwrap_err();
        let after = world.slot_stats();

        assert!(matches!(
            error,
            EngineError::EcsEntityGenerationOverflow {
                index: 0,
                generation: u32::MAX
            }
        ));
        assert_eq!(before, after);
        assert_eq!(
            world.slots[entity.index() as usize].location.unwrap().row,
            0
        );
        println!(
            "ISSUE1331_GENERATION_OVERFLOW_EDGE before={before:?} after={after:?} error_code={}",
            error.code()
        );
    }

    #[test]
    fn issue1331_corrupt_max_free_index_fails_closed() {
        let mut world = World::default();
        world.free_slots.push(u32::MAX);
        let before = world.slot_stats();

        let error = world.spawn((BoundaryMarker,)).unwrap_err();
        let after = world.slot_stats();

        assert!(matches!(
            error,
            EngineError::EcsFreeListCorrupt {
                index: u32::MAX,
                ..
            }
        ));
        assert_eq!(before, after);
        assert_eq!(world.free_slots, vec![u32::MAX]);
        println!(
            "ISSUE1331_MAX_FREE_INDEX_EDGE before={before:?} after={after:?} error_code={}",
            error.code()
        );
    }
}

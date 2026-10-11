use std::{
    any::{Any, TypeId, type_name},
    collections::HashSet,
};

use crate::EngineError;

use super::Entity;

pub trait Component: Send + Sync + 'static {}

impl<T> Component for T where T: Send + Sync + 'static {}

pub trait Event: Send + Sync + 'static {}

impl<T> Event for T where T: Send + Sync + 'static {}

pub trait ComponentBundle {
    fn into_entries(self) -> Vec<ComponentEntry>;
}

#[doc(hidden)]
pub struct ComponentEntry {
    type_id: TypeId,
    type_name: &'static str,
    make_column: fn() -> Box<dyn ComponentColumn>,
    component: Box<dyn ErasedComponent>,
}

impl ComponentEntry {
    fn new<T: Component>(component: T) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
            make_column: || Box::new(Column::<T>::default()),
            component: Box::new(StoredComponent(component)),
        }
    }
}

pub(super) trait ErasedComponent: Send + Sync {
    fn into_any(self: Box<Self>) -> Box<dyn Any + Send + Sync>;
}

struct StoredComponent<T: Component>(T);

impl<T: Component> ErasedComponent for StoredComponent<T> {
    fn into_any(self: Box<Self>) -> Box<dyn Any + Send + Sync> {
        Box::new(self.0)
    }
}

pub(super) trait ComponentColumn: Send + Sync {
    fn push_box(
        &mut self,
        component: Box<dyn ErasedComponent>,
        changed_tick: u64,
    ) -> Result<(), EngineError>;
    fn swap_remove(&mut self, row: usize);
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub(super) struct Column<T: Component> {
    values: Vec<T>,
    changed_ticks: Vec<u64>,
}

impl<T: Component> Column<T> {
    pub fn value(&self, row: usize) -> &T {
        &self.values[row]
    }

    pub fn value_mut(&mut self, row: usize) -> &mut T {
        &mut self.values[row]
    }

    pub fn changed_tick(&self, row: usize) -> u64 {
        self.changed_ticks[row]
    }

    pub fn set_changed_tick(&mut self, row: usize, tick: u64) {
        self.changed_ticks[row] = tick;
    }
}

impl<T: Component> Default for Column<T> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            changed_ticks: Vec::new(),
        }
    }
}

impl<T: Component> ComponentColumn for Column<T> {
    fn push_box(
        &mut self,
        component: Box<dyn ErasedComponent>,
        changed_tick: u64,
    ) -> Result<(), EngineError> {
        let component = component.into_any().downcast::<T>().map_err(|_| {
            EngineError::EcsComponentStorageMismatch {
                component: type_name::<T>(),
            }
        })?;
        self.values.push(*component);
        self.changed_ticks.push(changed_tick);
        Ok(())
    }

    fn swap_remove(&mut self, row: usize) {
        self.values.swap_remove(row);
        self.changed_ticks.swap_remove(row);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

pub(super) struct Archetype {
    signature: Vec<TypeId>,
    component_names: Vec<&'static str>,
    pub entities: Vec<Entity>,
    columns: Vec<Box<dyn ComponentColumn>>,
}

impl Archetype {
    pub fn new(entries: &[ComponentEntry]) -> Self {
        Self {
            signature: entries.iter().map(|entry| entry.type_id).collect(),
            component_names: entries.iter().map(|entry| entry.type_name).collect(),
            entities: Vec::new(),
            columns: entries.iter().map(|entry| (entry.make_column)()).collect(),
        }
    }

    pub fn matches(&self, entries: &[ComponentEntry]) -> bool {
        self.signature
            .iter()
            .copied()
            .eq(entries.iter().map(|entry| entry.type_id))
    }

    pub fn component_names(&self) -> &[&'static str] {
        &self.component_names
    }

    pub fn push(
        &mut self,
        entity: Entity,
        entries: Vec<ComponentEntry>,
        tick: u64,
    ) -> Result<usize, EngineError> {
        let row = self.entities.len();
        for (entry, column) in entries.into_iter().zip(&mut self.columns) {
            column.push_box(entry.component, tick)?;
        }
        self.entities.push(entity);
        Ok(row)
    }

    pub fn swap_remove(&mut self, row: usize) {
        for column in &mut self.columns {
            column.swap_remove(row);
        }
        self.entities.swap_remove(row);
    }

    pub fn column<T: Component>(&self) -> Option<&Column<T>> {
        let index = self.column_index(TypeId::of::<T>())?;
        self.columns[index].as_any().downcast_ref::<Column<T>>()
    }

    pub fn column_mut<T: Component>(&mut self) -> Option<&mut Column<T>> {
        let index = self.column_index(TypeId::of::<T>())?;
        self.columns[index].as_any_mut().downcast_mut::<Column<T>>()
    }

    /// Borrows the entity row slice together with one mutable component column.
    ///
    /// `entities` and `columns` are disjoint struct fields, so the borrow checker
    /// permits holding `&self.entities` alongside `&mut self.columns[index]`. This
    /// lets mutable queries iterate rows without cloning `entities` per call
    /// (issue #1524 item 1).
    pub fn entities_and_column_mut<T: Component>(&mut self) -> Option<(&[Entity], &mut Column<T>)> {
        let index = self.column_index(TypeId::of::<T>())?;
        let column = self.columns[index]
            .as_any_mut()
            .downcast_mut::<Column<T>>()?;
        Some((&self.entities, column))
    }

    /// Borrows the entity row slice together with two disjoint component columns
    /// (`A` mutable, `B` shared). Same rationale as [`Self::entities_and_column_mut`]:
    /// no per-call `entities` clone (issue #1524 item 1).
    pub fn entities_and_two_columns_mut<A: Component, B: Component>(
        &mut self,
    ) -> Option<(&[Entity], &mut Column<A>, &Column<B>)> {
        let a_index = self.column_index(TypeId::of::<A>())?;
        let b_index = self.column_index(TypeId::of::<B>())?;
        if a_index == b_index {
            return None;
        }
        let (a, b) = if a_index < b_index {
            let (left, right) = self.columns.split_at_mut(b_index);
            let a = left[a_index].as_any_mut().downcast_mut::<Column<A>>()?;
            let b = right[0].as_any().downcast_ref::<Column<B>>()?;
            (a, b)
        } else {
            let (left, right) = self.columns.split_at_mut(a_index);
            let b = left[b_index].as_any().downcast_ref::<Column<B>>()?;
            let a = right[0].as_any_mut().downcast_mut::<Column<A>>()?;
            (a, b)
        };
        Some((&self.entities, a, b))
    }

    fn column_index(&self, type_id: TypeId) -> Option<usize> {
        self.signature.iter().position(|id| *id == type_id)
    }
}

pub(super) fn validate_and_sort(entries: &mut [ComponentEntry]) -> Result<(), EngineError> {
    let mut seen = HashSet::with_capacity(entries.len());
    for entry in entries.iter() {
        if !seen.insert(entry.type_id) {
            return Err(EngineError::EcsDuplicateComponent {
                component: entry.type_name,
            });
        }
    }
    entries.sort_by(|left, right| left.type_name.cmp(right.type_name));
    Ok(())
}

impl ComponentBundle for () {
    fn into_entries(self) -> Vec<ComponentEntry> {
        Vec::new()
    }
}

impl<A: Component> ComponentBundle for (A,) {
    fn into_entries(self) -> Vec<ComponentEntry> {
        let (a,) = self;
        vec![ComponentEntry::new(a)]
    }
}

impl<A: Component, B: Component> ComponentBundle for (A, B) {
    fn into_entries(self) -> Vec<ComponentEntry> {
        let (a, b) = self;
        vec![ComponentEntry::new(a), ComponentEntry::new(b)]
    }
}

impl<A: Component, B: Component, C: Component> ComponentBundle for (A, B, C) {
    fn into_entries(self) -> Vec<ComponentEntry> {
        let (a, b, c) = self;
        vec![
            ComponentEntry::new(a),
            ComponentEntry::new(b),
            ComponentEntry::new(c),
        ]
    }
}

impl<A: Component, B: Component, C: Component, D: Component> ComponentBundle for (A, B, C, D) {
    fn into_entries(self) -> Vec<ComponentEntry> {
        let (a, b, c, d) = self;
        vec![
            ComponentEntry::new(a),
            ComponentEntry::new(b),
            ComponentEntry::new(c),
            ComponentEntry::new(d),
        ]
    }
}

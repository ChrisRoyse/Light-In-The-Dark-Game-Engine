use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Entity {
    index: u32,
    generation: u32,
}

impl Entity {
    pub(super) const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    pub const fn index(self) -> u32 {
        self.index
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}v{}", self.index, self.generation)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EntityLocation {
    pub archetype: usize,
    pub row: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct EntitySlot {
    pub generation: u32,
    pub location: Option<EntityLocation>,
}

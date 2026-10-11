use crate::EngineError;

use super::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleReport {
    pub run_index: u64,
    pub system_count: usize,
    pub system_runs: u64,
    pub world_tick: u64,
}

#[derive(Default)]
pub struct Schedule {
    systems: Vec<Box<dyn System>>,
    run_index: u64,
    system_runs: u64,
}

impl Schedule {
    pub fn add_system<F>(&mut self, name: &'static str, run: F)
    where
        F: FnMut(&mut World) -> Result<(), EngineError> + Send + 'static,
    {
        self.systems.push(Box::new(FnSystem { name, run }));
    }

    pub fn run(&mut self, world: &mut World) -> Result<ScheduleReport, EngineError> {
        self.run_index += 1;
        for system in &mut self.systems {
            system.run(world)?;
            self.system_runs += 1;
        }
        Ok(self.report(world))
    }

    pub fn report(&self, world: &World) -> ScheduleReport {
        ScheduleReport {
            run_index: self.run_index,
            system_count: self.systems.len(),
            system_runs: self.system_runs,
            world_tick: world.current_tick(),
        }
    }

    pub fn system_names(&self) -> Vec<&'static str> {
        self.systems.iter().map(|system| system.name()).collect()
    }
}

trait System: Send {
    fn name(&self) -> &'static str;
    fn run(&mut self, world: &mut World) -> Result<(), EngineError>;
}

struct FnSystem<F> {
    name: &'static str,
    run: F,
}

impl<F> System for FnSystem<F>
where
    F: FnMut(&mut World) -> Result<(), EngineError> + Send,
{
    fn name(&self) -> &'static str {
        self.name
    }

    fn run(&mut self, world: &mut World) -> Result<(), EngineError> {
        (self.run)(world)
    }
}

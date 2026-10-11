//! Scene graph: parent/child hierarchy and dirty-aware global transform propagation.

use std::collections::{HashMap, HashSet};

use crate::{
    EngineError,
    ecs::Entity,
    math::{Mat4, Transform, Vec3},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenePropagationStats {
    pub propagation_index: u64,
    pub root_count: usize,
    pub visited_nodes: usize,
    pub recomputed_nodes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneNodeInfo {
    pub entity: Entity,
    pub parent: Option<Entity>,
    pub child_count: usize,
    pub dirty: bool,
}

#[derive(Debug, Clone)]
struct SceneNode {
    parent: Option<Entity>,
    children: Vec<Entity>,
    local: Transform,
    global: Mat4,
    dirty: bool,
}

impl SceneNode {
    fn new(local: Transform) -> Self {
        Self {
            parent: None,
            children: Vec::new(),
            local,
            global: local.matrix(),
            dirty: true,
        }
    }
}

#[derive(Default)]
pub struct SceneGraph {
    nodes: HashMap<Entity, SceneNode>,
    roots: Vec<Entity>,
    propagation_index: u64,
}

impl SceneGraph {
    pub fn insert(&mut self, entity: Entity, local: Transform) -> Result<(), EngineError> {
        if self.nodes.contains_key(&entity) {
            return Err(EngineError::SceneDuplicateNode {
                entity: entity.to_string(),
            });
        }
        self.nodes.insert(entity, SceneNode::new(local));
        self.roots.push(entity);
        Ok(())
    }

    pub fn set_parent(&mut self, child: Entity, parent: Option<Entity>) -> Result<(), EngineError> {
        self.require_node(child)?;
        if let Some(parent) = parent {
            self.require_node(parent)?;
            self.reject_cycle(child, parent)?;
        }

        let old_parent = self.nodes[&child].parent;
        if old_parent == parent {
            return Ok(());
        }
        self.detach_from_current_parent(child, old_parent);
        if let Some(parent) = parent {
            self.nodes.get_mut(&parent).unwrap().children.push(child);
        } else if !self.roots.contains(&child) {
            self.roots.push(child);
        }
        self.nodes.get_mut(&child).unwrap().parent = parent;
        self.mark_subtree_dirty(child);
        Ok(())
    }

    pub fn set_local_transform(
        &mut self,
        entity: Entity,
        local: Transform,
    ) -> Result<(), EngineError> {
        self.require_node(entity)?;
        self.nodes.get_mut(&entity).unwrap().local = local;
        self.mark_subtree_dirty(entity);
        Ok(())
    }

    pub fn remove(&mut self, entity: Entity) -> Result<usize, EngineError> {
        self.require_node(entity)?;
        let old_parent = self.nodes[&entity].parent;
        self.detach_from_current_parent(entity, old_parent);
        Ok(self.remove_subtree(entity))
    }

    pub fn propagate(&mut self) -> Result<ScenePropagationStats, EngineError> {
        self.propagation_index = self.propagation_index.saturating_add(1);
        let mut stats = ScenePropagationStats {
            propagation_index: self.propagation_index,
            root_count: self.roots.len(),
            visited_nodes: 0,
            recomputed_nodes: 0,
        };
        let mut visited = HashSet::with_capacity(self.nodes.len());
        for root in self.roots.clone() {
            self.propagate_node(root, Mat4::IDENTITY, false, &mut visited, &mut stats)?;
        }
        Ok(stats)
    }

    pub fn local_transform(&self, entity: Entity) -> Result<Transform, EngineError> {
        self.nodes
            .get(&entity)
            .map(|node| node.local)
            .ok_or_else(|| missing_node(entity))
    }

    pub fn global_matrix(&self, entity: Entity) -> Result<Mat4, EngineError> {
        self.nodes
            .get(&entity)
            .map(|node| node.global)
            .ok_or_else(|| missing_node(entity))
    }

    pub fn global_translation(&self, entity: Entity) -> Result<Vec3, EngineError> {
        Ok(self.global_matrix(entity)?.transform_point3(Vec3::ZERO))
    }

    pub fn parent(&self, entity: Entity) -> Result<Option<Entity>, EngineError> {
        self.nodes
            .get(&entity)
            .map(|node| node.parent)
            .ok_or_else(|| missing_node(entity))
    }

    pub fn children(&self, entity: Entity) -> Result<&[Entity], EngineError> {
        self.nodes
            .get(&entity)
            .map(|node| node.children.as_slice())
            .ok_or_else(|| missing_node(entity))
    }

    pub fn node_infos(&self) -> Vec<SceneNodeInfo> {
        self.nodes
            .iter()
            .map(|(entity, node)| SceneNodeInfo {
                entity: *entity,
                parent: node.parent,
                child_count: node.children.len(),
                dirty: node.dirty,
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn root_count(&self) -> usize {
        self.roots.len()
    }

    pub fn dirty_count(&self) -> usize {
        self.nodes.values().filter(|node| node.dirty).count()
    }

    fn require_node(&self, entity: Entity) -> Result<(), EngineError> {
        if self.nodes.contains_key(&entity) {
            Ok(())
        } else {
            Err(missing_node(entity))
        }
    }

    fn reject_cycle(&self, child: Entity, parent: Entity) -> Result<(), EngineError> {
        let mut cursor = Some(parent);
        while let Some(entity) = cursor {
            if entity == child {
                return Err(EngineError::SceneParentCycle {
                    child: child.to_string(),
                    parent: parent.to_string(),
                });
            }
            cursor = self.nodes.get(&entity).and_then(|node| node.parent);
        }
        Ok(())
    }

    fn detach_from_current_parent(&mut self, child: Entity, old_parent: Option<Entity>) {
        if let Some(parent) = old_parent {
            if let Some(parent_node) = self.nodes.get_mut(&parent) {
                parent_node.children.retain(|entity| *entity != child);
            }
        } else {
            self.roots.retain(|entity| *entity != child);
        }
    }

    // Iterative traversal over an explicit heap worklist rather than native-stack
    // recursion: a deep parent chain (procedural nesting, long bone rigs) would
    // otherwise overflow the stack and panic instead of failing loud (#1484).
    fn remove_subtree(&mut self, entity: Entity) -> usize {
        let mut removed = 0;
        let mut stack = vec![entity];
        while let Some(current) = stack.pop() {
            let Some(node) = self.nodes.remove(&current) else {
                continue;
            };
            self.roots.retain(|root| *root != current);
            removed += 1;
            stack.extend(node.children);
        }
        removed
    }

    fn mark_subtree_dirty(&mut self, entity: Entity) {
        let mut stack = vec![entity];
        while let Some(current) = stack.pop() {
            let children = match self.nodes.get_mut(&current) {
                Some(node) => {
                    node.dirty = true;
                    node.children.clone()
                }
                None => continue,
            };
            stack.extend(children);
        }
    }

    fn propagate_node(
        &mut self,
        entity: Entity,
        parent_global: Mat4,
        parent_recomputed: bool,
        visited: &mut HashSet<Entity>,
        stats: &mut ScenePropagationStats,
    ) -> Result<(), EngineError> {
        if !visited.insert(entity) {
            return Err(EngineError::SceneParentCycle {
                child: entity.to_string(),
                parent: "propagation".to_string(),
            });
        }
        let (children, global, recomputed) = {
            let node = self
                .nodes
                .get_mut(&entity)
                .ok_or_else(|| missing_node(entity))?;
            let recomputed = node.dirty || parent_recomputed;
            if recomputed {
                node.global = parent_global * node.local.matrix();
                node.dirty = false;
                stats.recomputed_nodes += 1;
            }
            stats.visited_nodes += 1;
            (node.children.clone(), node.global, recomputed)
        };
        for child in children {
            self.propagate_node(child, global, recomputed, visited, stats)?;
        }
        Ok(())
    }
}

fn missing_node(entity: Entity) -> EngineError {
    EngineError::SceneNodeMissing {
        entity: entity.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngineError, ecs::World};

    fn entity(world: &mut World) -> Entity {
        world.spawn(()).unwrap()
    }

    #[test]
    fn hierarchy_propagates_global_transforms() {
        let mut world = World::default();
        let root = entity(&mut world);
        let child = entity(&mut world);
        let grandchild = entity(&mut world);
        let mut graph = SceneGraph::default();
        graph
            .insert(root, Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)))
            .unwrap();
        graph
            .insert(child, Transform::from_translation(Vec3::new(0.0, 5.0, 0.0)))
            .unwrap();
        graph
            .insert(
                grandchild,
                Transform::from_translation(Vec3::new(0.0, 0.0, 2.0)),
            )
            .unwrap();
        graph.set_parent(child, Some(root)).unwrap();
        graph.set_parent(grandchild, Some(child)).unwrap();

        let stats = graph.propagate().unwrap();

        assert_eq!(stats.recomputed_nodes, 3);
        assert_eq!(
            graph.global_translation(grandchild).unwrap(),
            Vec3::new(10.0, 5.0, 2.0)
        );
        assert_eq!(graph.dirty_count(), 0);
    }

    #[test]
    fn local_change_marks_only_the_subtree_dirty() {
        let mut world = World::default();
        let root = entity(&mut world);
        let child = entity(&mut world);
        let grandchild = entity(&mut world);
        let mut graph = SceneGraph::default();
        graph.insert(root, Transform::IDENTITY).unwrap();
        graph.insert(child, Transform::IDENTITY).unwrap();
        graph.insert(grandchild, Transform::IDENTITY).unwrap();
        graph.set_parent(child, Some(root)).unwrap();
        graph.set_parent(grandchild, Some(child)).unwrap();
        graph.propagate().unwrap();

        graph
            .set_local_transform(child, Transform::from_translation(Vec3::Y * 7.0))
            .unwrap();
        let stats = graph.propagate().unwrap();

        assert_eq!(stats.recomputed_nodes, 2);
        assert_eq!(
            graph.global_translation(grandchild).unwrap(),
            Vec3::new(0.0, 7.0, 0.0)
        );
    }

    #[test]
    fn cycle_parenting_fails_loud() {
        let mut world = World::default();
        let root = entity(&mut world);
        let child = entity(&mut world);
        let mut graph = SceneGraph::default();
        graph.insert(root, Transform::IDENTITY).unwrap();
        graph.insert(child, Transform::IDENTITY).unwrap();
        graph.set_parent(child, Some(root)).unwrap();

        match graph.set_parent(root, Some(child)) {
            Err(EngineError::SceneParentCycle { child, parent }) => {
                assert!(child.contains("#0v0"));
                assert!(parent.contains("#1v0"));
            }
            other => panic!("expected SceneParentCycle, got {other:?}"),
        }
    }

    #[test]
    fn removing_node_removes_descendants_and_detaches_parent() {
        let mut world = World::default();
        let root = entity(&mut world);
        let child = entity(&mut world);
        let grandchild = entity(&mut world);
        let mut graph = SceneGraph::default();
        graph.insert(root, Transform::IDENTITY).unwrap();
        graph.insert(child, Transform::IDENTITY).unwrap();
        graph.insert(grandchild, Transform::IDENTITY).unwrap();
        graph.set_parent(child, Some(root)).unwrap();
        graph.set_parent(grandchild, Some(child)).unwrap();

        assert_eq!(graph.remove(child).unwrap(), 2);

        assert_eq!(graph.len(), 1);
        assert_eq!(graph.children(root).unwrap(), &[]);
        assert!(matches!(
            graph.local_transform(child),
            Err(EngineError::SceneNodeMissing { .. })
        ));
    }

    #[test]
    fn deep_chain_subtree_ops_do_not_overflow_the_stack() {
        // Run on a deliberately small (512 KiB) stack so a chain this deep would
        // overflow under per-child recursion; the iterative traversal keeps its
        // worklist on the heap and survives (#1484). If anyone reverts to
        // recursion this test faults instead of silently regressing.
        std::thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(|| {
                let mut world = World::default();
                let mut graph = SceneGraph::default();
                const DEPTH: usize = 12_000;

                let mut chain = Vec::with_capacity(DEPTH);
                for depth in 0..DEPTH {
                    let node = entity(&mut world);
                    graph.insert(node, Transform::IDENTITY).unwrap();
                    if depth > 0 {
                        graph.set_parent(node, Some(chain[depth - 1])).unwrap();
                    }
                    chain.push(node);
                }

                // mark_subtree_dirty over the full depth (via set_local_transform).
                graph
                    .set_local_transform(chain[0], Transform::from_translation(Vec3::Y))
                    .unwrap();
                assert_eq!(graph.dirty_count(), DEPTH);

                // remove_subtree over the full depth returns the exact node count.
                assert_eq!(graph.remove(chain[0]).unwrap(), DEPTH);
                assert_eq!(graph.len(), 0);
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

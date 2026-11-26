use std::collections::HashMap;

use weave_heart::{ObjectHandle, ObjectWorld, ReparentError, SpawnError};

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    InvalidName,
    StaleParent,
    StaleObject,
    DuplicateName,
    Cycle,
}

#[derive(Clone)]
struct Node {
    id: u64,
    name: String,
    parent: Option<ObjectHandle>,
}

#[derive(Default)]
struct Model {
    nodes: HashMap<ObjectHandle, Node>,
    next_id: u64,
}

impl Model {
    fn expect_spawn(&self, parent: Option<ObjectHandle>, name: &str) -> Result<(), Kind> {
        if name.is_empty() || name.contains('/') {
            return Err(Kind::InvalidName);
        }
        if parent.is_some_and(|p| !self.nodes.contains_key(&p)) {
            return Err(Kind::StaleParent);
        }
        if self.has_child_named(parent, name) {
            return Err(Kind::DuplicateName);
        }
        Ok(())
    }

    fn expect_reparent(
        &self,
        object: ObjectHandle,
        target: Option<ObjectHandle>,
    ) -> Result<(), Kind> {
        let Some(node) = self.nodes.get(&object) else {
            return Err(Kind::StaleObject);
        };
        if let Some(t) = target {
            if !self.nodes.contains_key(&t) {
                return Err(Kind::StaleParent);
            }
            if self.is_at_or_below(t, object) {
                return Err(Kind::Cycle);
            }
        }
        if node.parent == target {
            return Ok(());
        }
        if self.has_child_named(target, &node.name) {
            return Err(Kind::DuplicateName);
        }
        Ok(())
    }

    fn has_child_named(&self, parent: Option<ObjectHandle>, name: &str) -> bool {
        self.nodes
            .values()
            .any(|node| node.parent == parent && node.name == name)
    }

    fn is_at_or_below(&self, candidate: ObjectHandle, ancestor: ObjectHandle) -> bool {
        let mut current = Some(candidate);
        while let Some(h) = current {
            if h == ancestor {
                return true;
            }
            current = self.nodes.get(&h).and_then(|n| n.parent);
        }
        false
    }

    fn path(&self, object: ObjectHandle) -> String {
        let mut segments = Vec::new();
        let mut current = Some(object);
        while let Some(h) = current {
            let node = &self.nodes[&h];
            segments.push(node.name.clone());
            current = node.parent;
        }
        segments.reverse();
        segments.join("/")
    }

    fn sorted_children(&self, parent: Option<ObjectHandle>) -> Vec<ObjectHandle> {
        let mut found: Vec<(u64, ObjectHandle)> = self
            .nodes
            .iter()
            .filter(|(_, node)| node.parent == parent)
            .map(|(h, node)| (node.id, *h))
            .collect();
        found.sort_unstable();
        found.into_iter().map(|(_, h)| h).collect()
    }

    fn sorted_objects(&self) -> Vec<ObjectHandle> {
        let mut found: Vec<(u64, ObjectHandle)> =
            self.nodes.iter().map(|(h, node)| (node.id, *h)).collect();
        found.sort_unstable();
        found.into_iter().map(|(_, h)| h).collect()
    }

    fn remove_subtree(&mut self, object: ObjectHandle) {
        let mut doomed = vec![object];
        let mut i = 0;
        while i < doomed.len() {
            let parent = doomed[i];
            let children: Vec<ObjectHandle> = self
                .nodes
                .iter()
                .filter(|(_, node)| node.parent == Some(parent))
                .map(|(h, _)| *h)
                .collect();
            doomed.extend(children);
            i += 1;
        }
        for h in doomed {
            self.nodes.remove(&h);
        }
    }
}

fn spawn_kind(error: &SpawnError) -> Kind {
    match error {
        SpawnError::InvalidName(_) => Kind::InvalidName,
        SpawnError::StaleParent(_) => Kind::StaleParent,
        SpawnError::DuplicateName { .. } => Kind::DuplicateName,
        other => panic!("unexpected spawn error {other:?}"),
    }
}

fn reparent_kind(error: &ReparentError) -> Kind {
    match error {
        ReparentError::StaleObject(_) => Kind::StaleObject,
        ReparentError::StaleParent(_) => Kind::StaleParent,
        ReparentError::Cycle => Kind::Cycle,
        ReparentError::DuplicateName { .. } => Kind::DuplicateName,
    }
}

fn check(world: &ObjectWorld, model: &Model, dead: &[ObjectHandle]) {
    assert_eq!(world.len(), model.nodes.len(), "object count");
    assert_eq!(
        world.objects().collect::<Vec<_>>(),
        model.sorted_objects(),
        "objects in id order"
    );
    assert_eq!(
        world.roots().collect::<Vec<_>>(),
        model.sorted_children(None),
        "roots in id order"
    );

    for (&object, node) in &model.nodes {
        assert!(world.contains(object));
        assert_eq!(world.id(object).map(|id| id.get()), Some(node.id));
        assert_eq!(world.name(object).as_deref(), Some(node.name.as_str()));
        assert_eq!(world.parent_of(object), node.parent);
        assert_eq!(world.is_root(object), Some(node.parent.is_none()));
        let path = model.path(object);
        assert_eq!(world.path(object), Some(path.clone()));
        assert_eq!(world.lookup(&path), Some(object), "lookup by path");
        assert_eq!(
            world.children(object).unwrap().collect::<Vec<_>>(),
            model.sorted_children(Some(object)),
            "children in id order"
        );
    }

    for &stale in dead {
        assert!(!world.contains(stale));
        assert_eq!(world.id(stale), None);
        assert_eq!(world.name(stale), None);
        assert_eq!(world.path(stale), None);
        assert!(world.children(stale).is_none());
        assert_eq!(world.parent_of(stale), None);
    }
}

fn random_name(rng: &mut Rng) -> String {
    match rng.below(20) {
        0 => String::new(),
        1 => "bad/name".to_owned(),
        n => ["a", "b", "c", "d"][n % 4].to_owned(),
    }
}

fn pick_handle(rng: &mut Rng, live: &[ObjectHandle], dead: &[ObjectHandle]) -> ObjectHandle {
    if !dead.is_empty() && rng.chance(10) {
        dead[rng.below(dead.len())]
    } else {
        live[rng.below(live.len())]
    }
}

#[derive(Default)]
struct Coverage {
    spawned: usize,
    rejected_spawns: usize,
    despawned: usize,
    reparented: usize,
    rejected_reparents: usize,
    cycles: usize,
    duplicate_names: usize,
    stale_targets: usize,
}

fn run(seed: u64, steps: usize, coverage: &mut Coverage) {
    let mut rng = Rng::new(seed);
    let mut world = ObjectWorld::new();
    let mut model = Model::default();
    let mut dead: Vec<ObjectHandle> = Vec::new();

    for step in 0..steps {
        let live = model.sorted_objects();
        match rng.below(100) {
            0..=39 => {
                let parent = if live.is_empty() || rng.chance(30) {
                    None
                } else if !dead.is_empty() && rng.chance(5) {
                    Some(dead[rng.below(dead.len())])
                } else {
                    Some(live[rng.below(live.len())])
                };
                let name = random_name(&mut rng);
                let expected = model.expect_spawn(parent, &name);
                match (expected, world.spawn(parent, &name)) {
                    (Ok(()), Ok(object)) => {
                        coverage.spawned += 1;
                        let id = model.next_id + 1;
                        model.next_id = id;
                        model.nodes.insert(object, Node { id, name, parent });
                    }
                    (Err(kind), Err(error)) => {
                        coverage.rejected_spawns += 1;
                        assert_eq!(
                            spawn_kind(&error),
                            kind,
                            "seed {seed} step {step}: spawn error kind"
                        );
                    }
                    (expected, actual) => panic!(
                        "seed {seed} step {step}: spawn expected {expected:?}, got {actual:?}"
                    ),
                }
            }
            40..=49 => {
                if live.is_empty() {
                    continue;
                }
                let object = pick_handle(&mut rng, &live, &dead);
                let was_live = model.nodes.contains_key(&object);
                let removed = world.despawn(object);
                assert_eq!(removed, was_live, "seed {seed} step {step}: despawn result");
                if was_live {
                    coverage.despawned += 1;
                    model.remove_subtree(object);
                    dead.push(object);
                }
            }
            _ => {
                if live.is_empty() {
                    continue;
                }
                let object = pick_handle(&mut rng, &live, &dead);
                let target = if rng.chance(20) {
                    None
                } else {
                    Some(pick_handle(&mut rng, &live, &dead))
                };
                let expected = model.expect_reparent(object, target);
                match (expected, world.reparent(object, target)) {
                    (Ok(()), Ok(())) => {
                        coverage.reparented += 1;
                        if let Some(node) = model.nodes.get_mut(&object) {
                            node.parent = target;
                        }
                    }
                    (Err(kind), Err(error)) => {
                        coverage.rejected_reparents += 1;
                        match kind {
                            Kind::Cycle => coverage.cycles += 1,
                            Kind::DuplicateName => coverage.duplicate_names += 1,
                            Kind::StaleParent | Kind::StaleObject => coverage.stale_targets += 1,
                            Kind::InvalidName => {}
                        }
                        assert_eq!(
                            reparent_kind(&error),
                            kind,
                            "seed {seed} step {step}: reparent error kind"
                        );
                    }
                    (expected, actual) => panic!(
                        "seed {seed} step {step}: reparent expected {expected:?}, got {actual:?}"
                    ),
                }
            }
        }
        check(&world, &model, &dead);
    }
}

#[test]
fn random_sequences_of_spawn_despawn_and_reparent_match_the_model() {
    let mut coverage = Coverage::default();
    for seed in 1..=40 {
        run(seed, 300, &mut coverage);
    }
    assert!(coverage.spawned > 500, "spawns: {}", coverage.spawned);
    assert!(
        coverage.rejected_spawns > 50,
        "rejected spawns: {}",
        coverage.rejected_spawns
    );
    assert!(coverage.despawned > 100, "despawns: {}", coverage.despawned);
    assert!(
        coverage.reparented > 200,
        "reparents: {}",
        coverage.reparented
    );
    assert!(
        coverage.cycles > 10,
        "cycle rejections: {}",
        coverage.cycles
    );
    assert!(
        coverage.duplicate_names > 10,
        "name clashes: {}",
        coverage.duplicate_names
    );
    assert!(
        coverage.stale_targets > 10,
        "stale targets: {}",
        coverage.stale_targets
    );
    assert!(coverage.rejected_reparents > 50);
}

#![feature(test)]

extern crate test;

use terrarium::{BasePart, Instance, InstanceId, Part, Workspace};
use test::{Bencher, black_box};

const FLAT_CHILDREN: usize = 4_096;
const TREE_ROOTS: usize = 32;
const TREE_DEPTH: usize = 5;
const TREE_BRANCHING: usize = 4;
const SUBTREE_ROOTS: usize = 256;
const SUBTREE_CHILDREN: usize = 16;
const CHURN_CHILDREN: usize = 4_096;
const CHURN_ROUNDS: usize = 8;

#[bench]
fn workspace_add_direct_children(bencher: &mut Bencher) {
    bencher.iter(|| {
        let mut workspace = Workspace::new();
        for _ in 0..FLAT_CHILDREN {
            black_box(workspace.add_child(Part::new()));
        }
        black_box(workspace.instances().count());
    });
}

#[bench]
fn workspace_add_nested_tree(bencher: &mut Bencher) {
    bencher.iter(|| {
        let mut workspace = Workspace::new();
        populate_tree(&mut workspace, TREE_ROOTS, TREE_DEPTH, TREE_BRANCHING);
        black_box(workspace.instances().count());
    });
}

#[bench]
fn workspace_traverse_flat_tree(bencher: &mut Bencher) {
    let (workspace, _) = build_flat_workspace(FLAT_CHILDREN);

    bencher.iter(|| {
        black_box(workspace.instances().fold(0usize, |total, instance| {
            total.wrapping_add(instance.class_name().len())
        }));
    });
}

#[bench]
fn workspace_traverse_nested_tree(bencher: &mut Bencher) {
    let mut workspace = Workspace::new();
    populate_tree(&mut workspace, TREE_ROOTS, TREE_DEPTH, TREE_BRANCHING);

    bencher.iter(|| {
        black_box(workspace.instances().fold(0usize, |total, instance| {
            total.wrapping_add(instance.class_name().len())
        }));
    });
}

#[bench]
fn workspace_indexed_lookup(bencher: &mut Bencher) {
    let (workspace, ids) = build_flat_workspace(FLAT_CHILDREN);

    bencher.iter(|| {
        let found = ids
            .iter()
            .filter(|&&id| workspace.instance(id).is_some())
            .count();
        black_box(found);
    });
}

#[bench]
fn workspace_remove_direct_children(bencher: &mut Bencher) {
    bencher.iter(|| {
        let (mut workspace, ids) = build_flat_workspace(FLAT_CHILDREN);
        let mut removed = 0;
        for id in ids {
            if workspace.remove_child(id) {
                removed += 1;
            }
        }
        black_box(removed);
    });
}

#[bench]
fn workspace_destroy_nested_subtrees(bencher: &mut Bencher) {
    bencher.iter(|| {
        let (mut workspace, roots) = build_subtree_workspace(SUBTREE_ROOTS, SUBTREE_CHILDREN);
        let mut destroyed = 0;
        for id in roots {
            if workspace.instance_mut(id).is_some_and(Instance::destroy) {
                destroyed += 1;
            }
        }
        black_box(destroyed);
    });
}

#[bench]
fn workspace_mixed_add_delete_churn(bencher: &mut Bencher) {
    bencher.iter(|| {
        let (mut workspace, mut active) = build_flat_workspace(CHURN_CHILDREN);
        let mut removed = 0;
        for _ in 0..CHURN_ROUNDS {
            let delete_count = active.len() / 4;
            for id in active.drain(..delete_count) {
                if workspace.remove_child(id) {
                    removed += 1;
                }
            }
            for _ in 0..delete_count {
                active.push(workspace.add_child(Part::new()));
            }
        }
        black_box((removed, workspace.instances().count()));
    });
}

fn build_flat_workspace(count: usize) -> (Workspace, Vec<InstanceId>) {
    let mut workspace = Workspace::new();
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        ids.push(workspace.add_child(Part::new()));
    }
    (workspace, ids)
}

fn populate_tree(workspace: &mut Workspace, roots: usize, depth: usize, branching: usize) {
    let mut frontier = (0..roots)
        .map(|_| workspace.add_child(BasePart::new()))
        .collect::<Vec<_>>();

    for _ in 1..depth {
        let mut next_frontier = Vec::with_capacity(frontier.len() * branching);
        for parent_id in frontier {
            let parent = workspace
                .get_mut::<BasePart>(parent_id)
                .expect("tree frontier should contain base parts");
            for _ in 0..branching {
                next_frontier.push(parent.add_child(BasePart::new()));
            }
        }
        frontier = next_frontier;
    }
}

fn build_subtree_workspace(
    root_count: usize,
    children_per_root: usize,
) -> (Workspace, Vec<InstanceId>) {
    let mut workspace = Workspace::new();
    let mut roots = Vec::with_capacity(root_count);
    for _ in 0..root_count {
        let root_id = workspace.add_child(BasePart::new());
        roots.push(root_id);
        let root = workspace
            .get_mut::<BasePart>(root_id)
            .expect("new subtree root should be a base part");
        for _ in 0..children_per_root {
            root.add_child(Part::new());
        }
    }
    (workspace, roots)
}

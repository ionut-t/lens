use std::collections::HashSet;
use std::path::PathBuf;

use super::result::TestResult;
use super::status::TestStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum NodeKind {
    Workspace,
    Project,
    File,
    Suite,
    Test,
}

#[derive(Debug, Clone)]
pub struct TestNode {
    pub id: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub kind: NodeKind,
    pub name: String,
    pub path: Option<PathBuf>,
    pub status: TestStatus,
    pub result: Option<TestResult>,
    pub expanded: bool,
    pub console_output: Vec<String>,
    /// Source location (line, column) for this test, if known.
    pub location: Option<(u32, u32)>,
    /// Tombstoned nodes are children removed when a file re-ran with fewer tests.
    pub deleted: bool,
    /// Marked at FileStarted; cleared when the test reports a result. Still-stale nodes
    /// are purged at FileFinished, meaning they no longer exist in the new run.
    pub stale: bool,
}

#[derive(Debug, Default)]
pub struct TestTree {
    nodes: Vec<TestNode>,
    root_ids: Vec<usize>,
}

impl TestTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a root-level node (workspace or project). Returns the node id.
    pub fn add_root(&mut self, kind: NodeKind, name: String, path: Option<PathBuf>) -> usize {
        let id = self.add_node(kind, name, path, None);
        self.root_ids.push(id);
        id
    }

    /// Add a child node under a parent. Returns the node id.
    pub fn add_child(
        &mut self,
        parent_id: usize,
        kind: NodeKind,
        name: String,
        path: Option<PathBuf>,
    ) -> usize {
        let id = self.add_node(kind, name, path, Some(parent_id));
        self.nodes[parent_id].children.push(id);
        id
    }

    fn add_node(
        &mut self,
        kind: NodeKind,
        name: String,
        path: Option<PathBuf>,
        parent: Option<usize>,
    ) -> usize {
        let id = self.nodes.len();
        let expanded = !matches!(kind, NodeKind::Test);
        self.nodes.push(TestNode {
            id,
            parent,
            children: Vec::new(),
            kind,
            name,
            path,
            status: TestStatus::Pending,
            result: None,
            expanded,
            console_output: Vec::new(),
            location: None,
            deleted: false,
            stale: false,
        });
        id
    }

    pub fn root_ids(&self) -> &[usize] {
        &self.root_ids
    }

    pub fn get(&self, id: usize) -> Option<&TestNode> {
        self.nodes.get(id)
    }

    pub fn get_mut(&mut self, id: usize) -> Option<&mut TestNode> {
        self.nodes.get_mut(id)
    }

    /// Find a child of `parent` with the given name, or None.
    pub fn find_child_by_name(&self, parent: usize, name: &str) -> Option<usize> {
        self.nodes
            .get(parent)?
            .children
            .iter()
            .copied()
            .find(|&id| self.nodes.get(id).is_some_and(|n| n.name == name))
    }

    /// Find a root node with the given name, or None.
    pub fn find_root_by_name(&self, name: &str) -> Option<usize> {
        self.root_ids
            .iter()
            .copied()
            .find(|&id| self.nodes.get(id).is_some_and(|n| n.name == name))
    }

    /// Find any non-deleted File node whose name equals `filename` (the basename, not full path).
    pub fn find_file_by_filename(&self, filename: &str) -> Option<usize> {
        self.nodes
            .iter()
            .find(|n| !n.deleted && n.kind == NodeKind::File && n.name == filename)
            .map(|n| n.id)
    }

    /// Returns a flat list of visible node ids (respecting expanded/collapsed state),
    /// paired with their depth for indentation.
    pub fn visible_nodes(&self) -> Vec<(usize, usize)> {
        let mut result = Vec::new();
        for &root_id in &self.root_ids {
            self.collect_visible(root_id, 0, &mut result);
        }
        result
    }

    fn collect_visible(&self, id: usize, depth: usize, result: &mut Vec<(usize, usize)>) {
        result.push((id, depth));
        let node = &self.nodes[id];
        if node.expanded {
            for &child_id in &node.children {
                self.collect_visible(child_id, depth + 1, result);
            }
        }
    }

    /// Returns visible nodes filtered by a case-insensitive substring match on File/Project names.
    /// Matching nodes show all descendants; ancestor nodes of matches are also shown.
    pub fn visible_nodes_filtered(&self, query: &str) -> Vec<(usize, usize)> {
        let query_lower = query.to_lowercase();

        // Nodes whose own name matches (Files and Projects only — tests/suites aren't filtered)
        let direct_matches: HashSet<usize> = self
            .nodes
            .iter()
            .filter(|n| {
                !n.deleted
                    && matches!(n.kind, NodeKind::File | NodeKind::Project)
                    && n.name.to_lowercase().contains(&query_lower)
            })
            .map(|n| n.id)
            .collect();

        if direct_matches.is_empty() {
            return vec![];
        }

        // Ancestor IDs of every direct match (so the path to matches stays visible)
        let mut ancestor_ids: HashSet<usize> = HashSet::new();
        for &id in &direct_matches {
            let mut cur = self.nodes[id].parent;
            while let Some(p) = cur {
                ancestor_ids.insert(p);
                cur = self.nodes[p].parent;
            }
        }

        let mut result = Vec::new();
        for &root_id in &self.root_ids {
            if direct_matches.contains(&root_id) || ancestor_ids.contains(&root_id) {
                self.collect_filtered(root_id, 0, &direct_matches, &ancestor_ids, &mut result);
            }
        }
        result
    }

    fn collect_filtered(
        &self,
        id: usize,
        depth: usize,
        direct_matches: &HashSet<usize>,
        ancestor_ids: &HashSet<usize>,
        result: &mut Vec<(usize, usize)>,
    ) {
        result.push((id, depth));
        let node = &self.nodes[id];
        if !node.expanded {
            return;
        }
        if direct_matches.contains(&id) {
            // Show everything under a directly matching node
            for &child_id in &node.children {
                self.collect_visible(child_id, depth + 1, result);
            }
        } else {
            // Only descend into children that lead to a match
            for &child_id in &node.children {
                if direct_matches.contains(&child_id) || ancestor_ids.contains(&child_id) {
                    self.collect_filtered(
                        child_id,
                        depth + 1,
                        direct_matches,
                        ancestor_ids,
                        result,
                    );
                }
            }
        }
    }

    /// Toggle the expanded state of a node. Returns the new state.
    pub fn toggle_expanded(&mut self, id: usize) -> bool {
        if let Some(node) = self.nodes.get_mut(id) {
            node.expanded = !node.expanded;
            node.expanded
        } else {
            false
        }
    }

    pub fn expand_all(&mut self) {
        for node in &mut self.nodes {
            if !node.children.is_empty() {
                node.expanded = true;
            }
        }
    }

    pub fn collapse_all(&mut self) {
        for node in &mut self.nodes {
            node.expanded = false;
        }
    }

    /// Update a test node's result and propagate status up to ancestors.
    pub fn update_result(&mut self, id: usize, result: TestResult) {
        let status = result.status;
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = status;
            node.result = Some(result);
        }
        self.propagate_status(id);
    }

    /// Recalculate a node's parent status based on children.
    /// Failed > Running > Pending > Passed > Skipped
    fn propagate_status(&mut self, id: usize) {
        let parent_id = match self.nodes.get(id).and_then(|n| n.parent) {
            Some(pid) => pid,
            None => return,
        };

        let aggregate = self.nodes[parent_id]
            .children
            .iter()
            .map(|&cid| self.nodes[cid].status)
            .fold(None, |acc: Option<TestStatus>, s| {
                Some(match acc {
                    None => s,
                    Some(prev) => Self::higher_priority(prev, s),
                })
            });

        if let Some(status) = aggregate {
            self.nodes[parent_id].status = status;
        }

        self.propagate_status(parent_id);
    }

    fn higher_priority(a: TestStatus, b: TestStatus) -> TestStatus {
        if b.priority() > a.priority() { b } else { a }
    }

    /// Count nodes by kind.
    pub fn count_kind(&self, kind: NodeKind) -> usize {
        self.nodes
            .iter()
            .filter(|n| !n.deleted && n.kind == kind)
            .count()
    }

    /// Count test nodes by terminal status. Returns (passed, failed, skipped).
    pub fn count_tests_by_status(&self) -> (usize, usize, usize) {
        let (mut passed, mut failed, mut skipped) = (0, 0, 0);
        for node in self.nodes.iter().filter(|n| !n.deleted) {
            if node.kind == NodeKind::Test {
                match node.status {
                    TestStatus::Passed => passed += 1,
                    TestStatus::Failed => failed += 1,
                    TestStatus::Skipped => skipped += 1,
                    _ => {}
                }
            }
        }
        (passed, failed, skipped)
    }

    /// Collect all node ids with Failed status.
    pub fn failed_nodes(&self) -> Vec<usize> {
        self.nodes
            .iter()
            .filter(|n| !n.deleted && n.kind == NodeKind::Test && n.status == TestStatus::Failed)
            .map(|n| n.id)
            .collect()
    }

    /// Mark all descendants of a file node as stale at the start of a file run.
    /// Stale nodes remain visible with their current status until `purge_stale_children` is called.
    pub fn mark_children_stale(&mut self, id: usize) {
        let children = self.nodes[id].children.clone();
        for child_id in children {
            self.mark_stale_recursive(child_id);
        }
    }

    fn mark_stale_recursive(&mut self, id: usize) {
        self.nodes[id].stale = true;
        let children = self.nodes[id].children.clone();
        for child_id in children {
            self.mark_stale_recursive(child_id);
        }
    }

    /// Remove any descendants still marked stale after a file run completes.
    /// These are tests/suites that no longer exist in the file.
    pub fn purge_stale_children(&mut self, id: usize) {
        let children = self.nodes[id].children.clone();
        let (keep, purge): (Vec<usize>, Vec<usize>) =
            children.into_iter().partition(|&c| !self.nodes[c].stale);
        self.nodes[id].children = keep;
        for child_id in purge {
            self.delete_subtree(child_id);
        }
        // Recurse into kept children to clean up stale grandchildren
        let kept = self.nodes[id].children.clone();
        for child_id in kept {
            self.purge_stale_children(child_id);
        }
    }

    /// Mark a node and its entire subtree as deleted.
    fn delete_subtree(&mut self, id: usize) {
        self.nodes[id].deleted = true;
        let children = self.nodes[id].children.clone();
        for child_id in children {
            self.delete_subtree(child_id);
        }
    }

    /// Count (passed, failed, total) test nodes in the subtree rooted at `id`.
    pub fn subtree_test_counts(&self, id: usize) -> (usize, usize, usize) {
        let mut passed = 0;
        let mut failed = 0;
        let mut total = 0;
        self.count_tests_recursive(id, &mut passed, &mut failed, &mut total);
        (passed, failed, total)
    }

    fn count_tests_recursive(
        &self,
        id: usize,
        passed: &mut usize,
        failed: &mut usize,
        total: &mut usize,
    ) {
        let node = &self.nodes[id];
        if node.deleted {
            return;
        }
        if node.kind == NodeKind::Test {
            *total += 1;
            match node.status {
                TestStatus::Passed => *passed += 1,
                TestStatus::Failed => *failed += 1,
                _ => {}
            }
        }
        for &child_id in &node.children {
            self.count_tests_recursive(child_id, passed, failed, total);
        }
    }

    /// Count File-kind nodes in the subtree rooted at `id`.
    pub fn subtree_file_count(&self, id: usize) -> usize {
        let node = &self.nodes[id];
        if node.deleted {
            return 0;
        }
        let self_count = usize::from(node.kind == NodeKind::File);
        node.children
            .iter()
            .map(|&c| self.subtree_file_count(c))
            .sum::<usize>()
            + self_count
    }

    /// Reset all nodes to Pending (for re-run).
    pub fn reset(&mut self) {
        for node in &mut self.nodes {
            node.status = TestStatus::Pending;
            node.result = None;
            node.console_output.clear();
        }
    }
}

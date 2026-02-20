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
        });
        id
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

    /// Returns visible nodes filtered by a case-insensitive substring match on file names.
    /// Only root (file) nodes are matched against the query; matching files show all children.
    pub fn visible_nodes_filtered(&self, query: &str) -> Vec<(usize, usize)> {
        let query_lower = query.to_lowercase();
        let mut result = Vec::new();
        for &root_id in &self.root_ids {
            let node = &self.nodes[root_id];
            if node.name.to_lowercase().contains(&query_lower) {
                self.collect_visible(root_id, 0, &mut result);
            }
        }
        result
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

    /// Collect all node ids with Failed status.
    pub fn failed_nodes(&self) -> Vec<usize> {
        self.nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Test && n.status == TestStatus::Failed)
            .map(|n| n.id)
            .collect()
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

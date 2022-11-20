use std::{collections::HashMap, path::Path};

#[derive(Clone, Copy, Debug)]
pub(crate) struct NodeId(pub(crate) usize);

pub(crate) struct Node {
    pub(crate) children: HashMap<String, NodeId>,
    pub(crate) paths: Vec<String>,
}

impl Node {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            paths: Vec::new(),
        }
    }
}

pub struct PathTree {
    nodes: Vec<Node>,
}

impl PathTree {
    pub fn new() -> Self {
        Self {
            nodes: vec![Node::new()],
        }
    }

    pub(crate) fn root_id() -> NodeId {
        NodeId(0)
    }

    pub(crate) fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }

    pub fn insert(&mut self, path: impl AsRef<Path>) {
        let mut current_node = Self::root_id();
        for segment in path.as_ref().components() {
            let segment = segment.as_os_str().to_string_lossy().to_string();
            let child = self.nodes[current_node.0].children.get(&segment);
            if let Some(&node_id) = child {
                current_node = node_id;
            } else {
                let node_id = NodeId(self.nodes.len());
                self.nodes.push(Node::new());
                self.nodes[current_node.0].children.insert(segment, node_id);
                current_node = node_id;
            }
        }
        self.nodes[current_node.0]
            .paths
            .push(path.as_ref().to_string_lossy().to_string());
    }
}

impl Default for PathTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: add some actual tests
    #[test]
    fn debug_tree() {
        let mut tree = PathTree::new();
        tree.insert("foo/bar");
        tree.insert("foo/bar/baz");
        tree.insert("foo/qux");
        tree.insert("a/b/c/d");
        tree.insert("a/b/whatever");

        print_tree(&tree, PathTree::root_id(), 0);
    }

    fn print_tree(tree: &PathTree, node_id: NodeId, indent: usize) {
        for (segment, &child_id) in tree.node(node_id).children.iter() {
            println!("{}{}", " ".repeat(indent), segment);
            print_tree(tree, child_id, indent + 2);
        }
    }
}

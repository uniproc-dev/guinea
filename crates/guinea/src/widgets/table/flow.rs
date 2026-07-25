use std::collections::HashSet;
use std::hash::Hash;

pub struct TableNode<VM, GID = String> {
    pub vm: VM,
    pub group_id: Option<GID>,
    pub has_children: bool,
    pub is_expanded: bool,
    pub level: u8,
    pub children: Vec<TableNode<VM, GID>>,
}

#[derive(Clone)]
pub struct SortState<SID> {
    pub field_id: Option<SID>,
    pub descending: bool,
}

pub trait TableDataBuilder<T, VM, GID = String> {
    fn build_tree(&mut self, items: &[T], expanded: &HashSet<GID>, out: &mut Vec<TableNode<VM, GID>>);
}

pub struct TableFlowState<T, VM, ID, GID, SID> {
    last_items: Vec<T>,
    expanded_groups: HashSet<GID>,
    selected_id: Option<ID>,
    frozen_index: Option<usize>,
    last_known_vm: Option<VM>,
    pub sort: SortState<SID>,

    tree_buffer: Vec<TableNode<VM, GID>>,
}

impl<T, VM, ID, GID, SID> TableFlowState<T, VM, ID, GID, SID>
where
    VM: Clone,
    ID: PartialEq + Clone,
    GID: Eq + Hash + Clone,
    SID: Clone,
{
    pub fn new(initial_sort: SortState<SID>) -> Self {
        Self {
            last_items: Vec::new(),
            expanded_groups: HashSet::new(),
            selected_id: None,
            frozen_index: None,
            last_known_vm: None,
            sort: initial_sort,
            tree_buffer: Vec::with_capacity(256),
        }
    }

    pub fn items(&self) -> &[T] {
        &self.last_items
    }

    pub fn set_items(&mut self, items: Vec<T>) {
        self.last_items = items;
    }

    pub fn clear_selection(&mut self) {
        self.selected_id = None;
        self.frozen_index = None;
        self.last_known_vm = None;
    }

    pub fn toggle_expand(&mut self, gid: GID) {
        if !self.expanded_groups.remove(&gid) {
            self.expanded_groups.insert(gid);
        }
    }

    pub fn find(&self, f: impl Fn(&T) -> bool) -> Option<&T> {
        self.last_items.iter().find(|&item| f(item))
    }

    pub fn select(&mut self, id: ID, idx: usize) {
        self.selected_id = Some(id);
        self.frozen_index = Some(idx);
    }

    pub fn update_view_model(
        &mut self,
        target: &mut Vec<VM>,
        builder: &mut impl TableDataBuilder<T, VM, GID>,
        sorter: impl Fn(&mut [TableNode<VM, GID>], &SortState<SID>),
        get_id: impl Fn(&VM) -> ID,
        mark_dead: impl Fn(&mut VM),
    ) {
        self.tree_buffer.clear();
        builder.build_tree(&self.last_items, &self.expanded_groups, &mut self.tree_buffer);
        sorter(&mut self.tree_buffer, &self.sort);
        self.apply_selection_stability(get_id, mark_dead);

        target.clear();
        flatten_to_target(&mut self.tree_buffer, target);
    }

    fn apply_selection_stability(&mut self, get_id: impl Fn(&VM) -> ID, mark_dead: impl Fn(&mut VM)) {
        let Some(sel_id) = &self.selected_id else {
            return;
        };

        let current_pos = self.tree_buffer.iter().position(|n| get_id(&n.vm) == *sel_id);

        if let Some(pos) = current_pos {
            self.last_known_vm = Some(self.tree_buffer[pos].vm.clone());

            if let Some(target_idx) = self.frozen_index {
                let target_idx = target_idx.min(self.tree_buffer.len().saturating_sub(1));
                if pos != target_idx {
                    let node = self.tree_buffer.remove(pos);
                    self.tree_buffer.insert(target_idx, node);
                }
            }
        } else if let Some(ghost_vm) = &self.last_known_vm {
            let mut dead_vm = ghost_vm.clone();
            mark_dead(&mut dead_vm);
            let idx = self.frozen_index.unwrap_or(0).min(self.tree_buffer.len());
            self.tree_buffer.insert(
                idx,
                TableNode {
                    vm: dead_vm,
                    group_id: None,
                    has_children: false,
                    is_expanded: false,
                    level: 0,
                    children: Vec::new(),
                },
            );
        }
    }
}

fn flatten_to_target<VM: Clone, GID>(nodes: &mut Vec<TableNode<VM, GID>>, target: &mut Vec<VM>) {
    for node in nodes.drain(..) {
        target.push(node.vm);
        if !node.children.is_empty() {
            flatten_recursive(node.children, target);
        }
    }
}

fn flatten_recursive<VM: Clone, GID>(nodes: Vec<TableNode<VM, GID>>, target: &mut Vec<VM>) {
    for node in nodes {
        target.push(node.vm);
        if !node.children.is_empty() {
            flatten_recursive(node.children, target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, PartialEq, Debug)]
    struct Row {
        id: u32,
        name: String,
        dead: bool,
    }

    struct FlatBuilder;
    impl TableDataBuilder<Row, Row, String> for FlatBuilder {
        fn build_tree(&mut self, items: &[Row], _expanded: &HashSet<String>, out: &mut Vec<TableNode<Row, String>>) {
            out.extend(items.iter().cloned().map(|vm| TableNode {
                vm,
                group_id: None,
                has_children: false,
                is_expanded: false,
                level: 0,
                children: Vec::new(),
            }));
        }
    }

    fn no_sort(_nodes: &mut [TableNode<Row, String>], _sort: &SortState<String>) {}

    fn state() -> TableFlowState<Row, Row, u32, String, String> {
        TableFlowState::new(SortState { field_id: None, descending: false })
    }

    #[test]
    fn flattens_in_build_order_with_no_selection() {
        let mut flow = state();
        flow.set_items(vec![
            Row { id: 1, name: "a".into(), dead: false },
            Row { id: 2, name: "b".into(), dead: false },
        ]);

        let mut target = Vec::new();
        flow.update_view_model(&mut target, &mut FlatBuilder, no_sort, |r| r.id, |r| r.dead = true);

        assert_eq!(target.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn selection_freezes_position_across_reorder() {
        let mut flow = state();
        flow.set_items(vec![
            Row { id: 1, name: "a".into(), dead: false },
            Row { id: 2, name: "b".into(), dead: false },
            Row { id: 3, name: "c".into(), dead: false },
        ]);

        let mut target = Vec::new();
        flow.update_view_model(&mut target, &mut FlatBuilder, no_sort, |r| r.id, |r| r.dead = true);
        flow.select(3, 2); // id 3 is currently at index 2

        // Items reordered upstream - id 3 now comes first.
        flow.set_items(vec![
            Row { id: 3, name: "c".into(), dead: false },
            Row { id: 1, name: "a".into(), dead: false },
            Row { id: 2, name: "b".into(), dead: false },
        ]);

        let mut target = Vec::new();
        flow.update_view_model(&mut target, &mut FlatBuilder, no_sort, |r| r.id, |r| r.dead = true);

        assert_eq!(target[2].id, 3, "selected row spliced back to its frozen index");
    }

    #[test]
    fn selection_of_removed_row_becomes_a_ghost_at_its_frozen_index() {
        let mut flow = state();
        flow.set_items(vec![
            Row { id: 1, name: "a".into(), dead: false },
            Row { id: 2, name: "b".into(), dead: false },
        ]);

        let mut target = Vec::new();
        flow.update_view_model(&mut target, &mut FlatBuilder, no_sort, |r| r.id, |r| r.dead = true);
        flow.select(2, 1);

        // A refresh while id 2 is still present and selected - this is what
        // captures `last_known_vm`, the data the ghost row will later reuse.
        let mut target = Vec::new();
        flow.update_view_model(&mut target, &mut FlatBuilder, no_sort, |r| r.id, |r| r.dead = true);

        // id 2 disappears upstream.
        flow.set_items(vec![Row { id: 1, name: "a".into(), dead: false }]);

        let mut target = Vec::new();
        flow.update_view_model(&mut target, &mut FlatBuilder, no_sort, |r| r.id, |r| r.dead = true);

        assert_eq!(target.len(), 2, "a ghost row stands in for the removed selection");
        assert!(target[1].dead, "ghost row is marked dead, not a live row");
        assert_eq!(target[1].id, 2, "ghost keeps the last-known row's data");
    }
}

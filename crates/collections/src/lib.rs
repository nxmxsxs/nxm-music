// use orx_tree::{Dary, DaryTree, Node, NodeIdx, NodeRef as _, Side};
// use std::cell::Cell;
// use std::collections::VecDeque;
//
// const BITS: usize = 6;
// const WIDTH: usize = 1 << BITS; // 64
// const MASK: usize = WIDTH - 1;
//
// #[derive(Debug)]
// pub enum NodeData<T> {
//     Internal, // Zero metadata.
//     Leaf {
//         values: VecDeque<T>,
//         // Stable pointers to neighbors to allow O(1) horizontal rippling
//         next: Cell<Option<NodeIdx<Dary<{ WIDTH }, NodeData<T>>>>>,
//     },
// }
//
// impl<T: Clone> Clone for NodeData<T> {
//     fn clone(&self) -> Self {
//         match self {
//             Self::Internal => Self::Internal,
//             Self::Leaf { values, next } => Self::Leaf {
//                 values: values.clone(),
//                 next: Cell::new(None),
//             },
//         }
//     }
// }
//
// #[derive(Clone)]
// pub struct RadixLinkTree<T> {
//     tree: DaryTree<{ WIDTH }, NodeData<T>>,
//     root_shift: u32,
//     len: usize,
//     last_leaf: Option<NodeIdx<Dary<{ WIDTH }, NodeData<T>>>>,
// }
//
// impl<T> RadixLinkTree<T> {
//     pub fn new() -> Self {
//         Self {
//             tree: DaryTree::new(NodeData::Leaf {
//                 values: VecDeque::with_capacity(WIDTH),
//                 next: Cell::new(None),
//             }),
//             root_shift: 0,
//             len: 0,
//             last_leaf: None,
//         }
//     }
//
//     // --- THE 5ns PATH (Pure Bit-Addressing) ---
//     #[inline(always)]
//     pub fn get(&self, index: usize) -> Option<&T> {
//         if index >= self.len {
//             return None;
//         }
//
//         let mut curr = self.tree.root();
//         let mut shift = self.root_shift;
//
//         // Addressing: No comparisons, no prefixes.
//         while shift > 0 {
//             curr = curr.child((index >> shift) & MASK);
//             shift -= BITS as u32;
//         }
//
//         if let NodeData::Leaf { values, .. } = curr.data() {
//             return values.get(index & MASK);
//         }
//         None
//     }
//
//     // --- THE HORIZONTAL RIPPLE (O(N/64)) ---
//     pub fn insert(&mut self, index: usize, value: T) {
//         if index == self.len {
//             self.push(value);
//             return;
//         }
//
//         // 1. Find the target leaf using bit-math (One-time cost)
//         let mut curr_idx = self.tree.root().idx();
//         let mut shift = self.root_shift;
//         while shift > 0 {
//             println!("{:#?}", curr_idx);
//             curr_idx = self
//                 .tree
//                 .node(curr_idx)
//                 .child((index >> shift) & MASK)
//                 .idx();
//             shift -= BITS as u32;
//         }
//
//         // 2. Ripple horizontally using next pointers
//         let mut moving_val = value;
//         let mut is_first = true;
//         let mut cursor = curr_idx;
//
//         loop {
//             let overflow = {
//                 let mut cursornode_mut = self.tree.node_mut(cursor);
//                 let NodeData::Leaf { values, .. } = cursornode_mut.data_mut() else {
//                     unreachable!()
//                 };
//                 if is_first {
//                     values.insert(index & MASK, moving_val);
//                     is_first = false;
//                 } else {
//                     values.push_front(moving_val);
//                 }
//
//                 if values.len() > WIDTH {
//                     values.pop_back()
//                 } else {
//                     None
//                 }
//             };
//
//             if let Some(val) = overflow {
//                 moving_val = val;
//                 // Follow the linked list, NOT the tree
//                 let NodeData::Leaf { values: _, next } = self.get_leaf_data(cursor) else {
//                     panic!("at the disco")
//                 };
//                 let next_idx = next.get().expect("Tree integrity error");
//                 cursor = next_idx;
//             } else {
//                 break;
//             }
//         }
//         self.len += 1;
//     }
//
//     pub fn push(&mut self, value: T) {
//         let capacity = 1 << (self.root_shift + BITS as u32);
//         if self.len == capacity {
//             self.grow_height();
//         }
//
//         let mut curr_idx = self.tree.root().idx();
//         let mut shift = self.root_shift;
//         while shift > 0 {
//             let target_child = (self.len >> shift) & MASK;
//             if target_child >= self.tree.node(curr_idx).num_children() {
//                 let is_next_level_leaf = shift == BITS as u32;
//                 let new_node = if is_next_level_leaf {
//                     NodeData::Leaf {
//                         values: VecDeque::with_capacity(WIDTH),
//                         next: Cell::new(None),
//                     }
//                 } else {
//                     NodeData::Internal
//                 };
//
//                 let new_idx = self.tree.node_mut(curr_idx).push_child(new_node);
//
//                 // If it's a leaf, link it into the horizontal chain
//                 if is_next_level_leaf {
//                     if let Some(prev_idx) = self.last_leaf {
//                         let NodeData::Leaf { values: _, next } = self.get_leaf_data(prev_idx)
//                         else {
//                             unreachable!()
//                         };
//                         next.set(Some(new_idx));
//                     }
//                     self.last_leaf = Some(new_idx);
//                 }
//             }
//             curr_idx = self.tree.node(curr_idx).child(target_child).idx();
//             shift -= BITS as u32;
//         }
//
//         if let NodeData::Leaf { values, .. } = self.tree.node_mut(curr_idx).data_mut() {
//             if self.last_leaf.is_none() {
//                 self.last_leaf = Some(curr_idx);
//             }
//             values.push_back(value);
//         }
//         self.len += 1;
//     }
//
//     // --- HELPERS ---
//
//     fn get_leaf_data(&self, idx: NodeIdx<Dary<{ WIDTH }, NodeData<T>>>) -> &NodeData<T> {
//         self.tree.node(idx).data()
//     }
//
//     fn grow_height(&mut self) {
//         self.tree.root_mut().push_parent(NodeData::Internal);
//         self.root_shift += BITS as u32;
//     }
// }
//

pub type FxIndexMap<K, V> = indexmap::IndexMap<K, V, rustc_hash::FxBuildHasher>;
pub type FxIndexSet<K> = indexmap::IndexSet<K, rustc_hash::FxBuildHasher>;

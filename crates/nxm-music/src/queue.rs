use std::{collections::HashMap, sync::Arc};

use collections::FxIndexMap;
use orx_linked_list::{
    DoublyEnds as _, DoublyEndsMut as _, DoublyIdx, DoublyIterable as _, DoublyListLazy,
};
use rand::Rng as _;
use uuid::Uuid;

use crate::library::Track;

pub struct Queue {
    pub list: DoublyListLazy<Uuid>, // main queue traversal/mutation. O(1)
    pub curr: Option<DoublyIdx<Uuid>>, //
    pub tracks: HashMap<Uuid, Vec<DoublyIdx<Uuid>>>, // Reverse Map. Vec<...> for tracking duplicates in a queue
    pub order: imbl::Vector<DoublyIdx<Uuid>>,        // indices order view
}

impl Queue {
    #[allow(clippy::new_without_default)]
    pub fn new(library: &FxIndexMap<Uuid, Arc<Track>>) -> Self {
        let mut list = DoublyListLazy::new();
        let mut tracks = HashMap::new();
        let mut order = imbl::Vector::new();

        // println!("library = {:#?}", library);

        for track_uuid in library.keys().copied() {
            let idx = list.push_back(track_uuid);
            tracks
                .entry(track_uuid)
                .and_modify(|idxs: &mut Vec<_>| idxs.push(idx))
                .or_insert_with(|| vec![idx]);
            order.push_back(idx);
        }

        let curr = list.indices().next();

        Self {
            list,
            curr,
            tracks,
            order,
        }
    }

    pub fn curr(&mut self) -> Option<Uuid> {
        if let Some(curr) = self.curr {
            return self.list.get(curr).copied();
        }

        if self.list.is_empty() {
            return None;
        }

        self.curr = self.list.indices().next();

        Some(self.list[self.curr.expect("Track Cursor shouldn't be None")])
    }

    pub fn shuffle(&mut self) {
        let mut rng = rand::rng();
        // queue.shuffle(&mut rng);

        for i in (0..self.order.len()).rev() {
            let j = rng.random_range(0..=i);

            if i != j {
                self.order.swap(i, j);
                self.list.swap(self.order[i], self.order[j]);
            }
        }
    }

    pub fn track_at(&self, index: usize) -> Option<Uuid> {
        let &idx = self.order.get(index)?;

        self.list.get(idx).copied()
    }

    pub fn remove_by_uuid(&mut self, uuid: &Uuid) -> Option<Vec<DoublyIdx<Uuid>>> {
        self.tracks.remove(uuid)
    }

    pub fn next(&mut self) -> Option<Uuid> {
        if self.list.is_empty() {
            return None;
        }

        if let Some(curr) = self.curr
            && let Some(next) = self.list.next_idx_of(curr)
        {
            self.curr = Some(next);
            Some(self.list[next])
        } else {
            self.curr = self.list.indices().next();
            Some(self.list[self.curr.expect("no no")])
        }
    }

    pub fn peek_next(&self) -> Option<Uuid> {
        if let Some(curr) = self.curr
            && let Some(next_idx) = self.list.next_idx_of(curr)
        {
            Some(self.list[next_idx])
        } else {
            None
        }
    }
}

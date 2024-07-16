use std::io::Cursor;

use indradb::{util, Edge};
use sled::{Batch, Iter as DbIterator, Tree};
use uuid::Uuid;

use crate::datastore::SledHolder;
use crate::errors::map_err;

pub struct EdgeRangeManager<'tree> {
    pub tree: &'tree Tree,
}

impl<'tree> EdgeRangeManager<'tree> {
    pub fn new<'db: 'tree>(ds: &'db SledHolder) -> Self {
        EdgeRangeManager { tree: &ds.edge_ranges }
    }

    pub fn new_reversed<'db: 'tree>(ds: &'db SledHolder) -> Self {
        EdgeRangeManager {
            tree: &ds.reversed_edge_ranges,
        }
    }

    fn key(&self, edge: &Edge) -> Vec<u8> {
        util::build(&[
            util::Component::Uuid(edge.outbound_id),
            util::Component::Identifier(edge.t),
            util::Component::Uuid(edge.inbound_id),
        ])
    }

    pub(crate) fn contains(&self, edge: &Edge) -> indradb::Result<bool> {
        let key = self.key(edge);
        map_err(self.tree.contains_key(key))
    }

    fn sled_to_edge(iter: DbIterator) -> impl Iterator<Item = indradb::Result<Edge>> {
        iter.map(move |item| {
            let (k, _) = map_err(item)?;
            let mut cursor = Cursor::new(k);
            let outbound_id = util::read_uuid(&mut cursor);
            let t = util::read_identifier(&mut cursor);
            let inbound_id = util::read_uuid(&mut cursor);
            Ok(Edge {
                outbound_id,
                t,
                inbound_id,
            })
        })
    }

    pub fn iterate_for_range<'iter, 'trans: 'iter>(
        &'trans self,
        edge: &Edge,
    ) -> impl Iterator<Item = indradb::Result<Edge>> {
        let offset = self.key(edge);
        let iterator = self.tree.range(offset..);
        Self::sled_to_edge(iterator)
    }

    pub fn iterate_for_all(&self) -> impl Iterator<Item = indradb::Result<Edge>> {
        let iterator = self.tree.iter();
        Self::sled_to_edge(iterator)
    }

    pub fn iterate_for_owner<'iter, 'trans: 'iter>(
        &'trans self,
        id: Uuid,
    ) -> impl Iterator<Item = indradb::Result<Edge>> + 'iter {
        let prefix: Vec<u8> = util::build(&[util::Component::Uuid(id)]);
        let iterator = self.tree.scan_prefix(prefix);
        Self::sled_to_edge(iterator)
    }

    pub fn set(&self, edge: &Edge) -> indradb::Result<()> {
        let key = self.key(edge);
        map_err(self.tree.insert(key, &[]))?;
        Ok(())
    }

    pub fn set_batch(&self, edge: &Edge, batch: &mut Batch) -> indradb::Result<()> {
        let key = self.key(edge);
        batch.insert(key, &[]);
        Ok(())
    }

    pub fn delete(&self, edge: &Edge) -> indradb::Result<()> {
        map_err(self.tree.remove(self.key(edge)))?;
        Ok(())
    }
}

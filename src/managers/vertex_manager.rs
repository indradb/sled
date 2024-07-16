use std::io::Cursor;
use std::ops::Deref;

use indradb::{util, Identifier, Vertex};
use sled::{Batch, Iter as DbIterator, Tree};
use uuid::Uuid;

use crate::datastore::SledHolder;
use crate::errors::map_err;
use crate::managers::edge_manager::EdgeManager;
use crate::managers::edge_range_manager::EdgeRangeManager;
use crate::managers::vertex_property_manager::VertexPropertyManager;

pub type VertexItem = (Uuid, Identifier);

pub struct VertexManager<'db: 'tree, 'tree> {
    pub holder: &'db SledHolder,
    pub tree: &'tree Tree,
}

impl<'db: 'tree, 'tree> VertexManager<'db, 'tree> {
    pub fn new(ds: &'db SledHolder) -> Self {
        VertexManager {
            holder: ds,
            tree: ds.db.deref(),
        }
    }

    pub fn count(&self) -> u64 {
        self.tree.iter().count() as u64
    }

    fn key(&self, id: Uuid) -> Vec<u8> {
        util::build(&[util::Component::Uuid(id)])
    }

    pub fn exists(&self, id: Uuid) -> indradb::Result<bool> {
        Ok(map_err(self.tree.get(self.key(id)))?.is_some())
    }

    pub fn get(&self, id: Uuid) -> indradb::Result<Option<Identifier>> {
        match map_err(self.tree.get(self.key(id)))? {
            Some(value_bytes) => {
                let mut cursor = Cursor::new(value_bytes.deref());
                Ok(Some(util::read_identifier(&mut cursor)))
            }
            None => Ok(None),
        }
    }

    fn iterate(&self, iterator: DbIterator) -> impl Iterator<Item = indradb::Result<VertexItem>> + '_ {
        iterator.map(move |item| -> indradb::Result<VertexItem> {
            let (k, v) = map_err(item)?;

            let id = {
                debug_assert_eq!(k.len(), 16);
                let mut cursor = Cursor::new(k);
                util::read_uuid(&mut cursor)
            };

            let mut cursor = Cursor::new(v);
            let t = util::read_identifier(&mut cursor);
            Ok((id, t))
        })
    }

    pub fn iterate_for_range(&self, id: Uuid) -> impl Iterator<Item = indradb::Result<VertexItem>> + '_ {
        let low_key = util::build(&[util::Component::Uuid(id)]);
        let low_key_bytes: &[u8] = low_key.as_ref();
        let iter = self.tree.range(low_key_bytes..);
        self.iterate(iter)
    }

    pub fn create(&self, vertex: &Vertex) -> indradb::Result<bool> {
        let key = self.key(vertex.id);
        if map_err(self.tree.contains_key(&key))? {
            return Ok(false);
        }
        map_err(
            self.tree
                .insert(&key, util::build(&[util::Component::Identifier(vertex.t)])),
        )?;
        Ok(true)
    }

    pub fn create_batch(&self, vertex: &Vertex, batch: &mut Batch) -> indradb::Result<()> {
        let key = self.key(vertex.id);
        batch.insert(key.clone(), util::build(&[util::Component::Identifier(vertex.t)]));
        Ok(())
    }

    pub fn delete(&self, id: Uuid) -> indradb::Result<()> {
        map_err(self.tree.remove(self.key(id)))?;

        let vertex_property_manager =
            VertexPropertyManager::new(&self.holder.vertex_properties, &self.holder.vertex_property_values);
        for item in vertex_property_manager.iterate_for_owner(id)? {
            let ((vertex_property_owner_id, vertex_property_name), _) = item?;
            vertex_property_manager.delete(vertex_property_owner_id, vertex_property_name)?;
        }

        let edge_manager = EdgeManager::new(self.holder);

        {
            let edge_range_manager = EdgeRangeManager::new(self.holder);
            for item in edge_range_manager.iterate_for_owner(id) {
                let edge = item?;
                debug_assert_eq!(edge.outbound_id, id);
                edge_manager.delete(&edge)?;
            }
        }

        Ok(())
    }
}

use std::io::Cursor;
use std::ops::Deref;
use std::u8;

use super::errors::map_err;
use crate::datastore::SledHolder;

use chrono::offset::Utc;
use chrono::DateTime;
use indradb::{util, Result, Type, Vertex};
use serde_json::Value as JsonValue;
use sled::Result as SledResult;
use sled::{IVec, Iter as DbIterator, Tree};
use uuid::Uuid;

pub type OwnedPropertyItem = ((Uuid, String), JsonValue);
pub type VertexItem = (Uuid, Type);
pub type EdgeRangeItem = (Uuid, Type, DateTime<Utc>, Uuid);
pub type EdgePropertyItem = ((Uuid, Type, Uuid, String), JsonValue);

fn take_while_prefixed(iterator: DbIterator, prefix: Vec<u8>) -> impl Iterator<Item = SledResult<(IVec, IVec)>> {
    iterator.take_while(move |item| -> bool {
        match item {
            Ok((k, _)) => k.starts_with(&prefix),
            Err(_) => false,
        }
    })
}

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

    fn key(&self, id: Uuid) -> Vec<u8> {
        util::build(&[util::Component::Uuid(id)])
    }

    pub fn exists(&self, id: Uuid) -> Result<bool> {
        Ok(map_err(self.tree.get(&self.key(id)))?.is_some())
    }

    pub fn get(&self, id: Uuid) -> Result<Option<Type>> {
        match map_err(self.tree.get(&self.key(id)))? {
            Some(value_bytes) => {
                let mut cursor = Cursor::new(value_bytes.deref());
                Ok(Some(util::read_type(&mut cursor)))
            }
            None => Ok(None),
        }
    }

    fn iterate(&self, iterator: DbIterator) -> impl Iterator<Item = Result<VertexItem>> + '_ {
        iterator.map(move |item| -> Result<VertexItem> {
            let (k, v) = map_err(item)?;

            let id = {
                debug_assert_eq!(k.len(), 16);
                let mut cursor = Cursor::new(k);
                util::read_uuid(&mut cursor)
            };

            let mut cursor = Cursor::new(v);
            let t = util::read_type(&mut cursor);
            Ok((id, t))
        })
    }

    pub fn iterate_for_range(&self, id: Uuid) -> impl Iterator<Item = Result<VertexItem>> + '_ {
        let low_key = util::build(&[util::Component::Uuid(id)]);
        let low_key_bytes: &[u8] = low_key.as_ref();
        let iter = self.tree.range(low_key_bytes..);
        self.iterate(iter)
    }

    pub fn create(&self, vertex: &Vertex) -> Result<()> {
        let key = self.key(vertex.id);
        map_err(self.tree.insert(&key, util::build(&[util::Component::Type(&vertex.t)])))?;
        Ok(())
    }

    pub fn delete(&self, id: Uuid) -> Result<()> {
        map_err(self.tree.remove(&self.key(id)))?;

        let vertex_property_manager = VertexPropertyManager::new(&self.holder.vertex_properties);
        for item in vertex_property_manager.iterate_for_owner(id)? {
            let ((vertex_property_owner_id, vertex_property_name), _) = item?;
            vertex_property_manager.delete(vertex_property_owner_id, &vertex_property_name[..])?;
        }

        let edge_manager = EdgeManager::new(self.holder);

        {
            let edge_range_manager = EdgeRangeManager::new(self.holder);
            for item in edge_range_manager.iterate_for_owner(id) {
                let (edge_range_outbound_id, edge_range_t, edge_range_update_datetime, edge_range_inbound_id) = item?;
                debug_assert_eq!(edge_range_outbound_id, id);
                edge_manager.delete(
                    edge_range_outbound_id,
                    &edge_range_t,
                    edge_range_inbound_id,
                    edge_range_update_datetime,
                )?;
            }
        }

        {
            let reversed_edge_range_manager = EdgeRangeManager::new_reversed(self.holder);
            for item in reversed_edge_range_manager.iterate_for_owner(id) {
                let (
                    reversed_edge_range_inbound_id,
                    reversed_edge_range_t,
                    reversed_edge_range_update_datetime,
                    reversed_edge_range_outbound_id,
                ) = item?;
                debug_assert_eq!(reversed_edge_range_inbound_id, id);
                edge_manager.delete(
                    reversed_edge_range_outbound_id,
                    &reversed_edge_range_t,
                    reversed_edge_range_inbound_id,
                    reversed_edge_range_update_datetime,
                )?;
            }
        }
        Ok(())
    }
}

pub struct EdgeManager<'db: 'tree, 'tree> {
    pub holder: &'db SledHolder,
    pub tree: &'tree Tree,
}

impl<'db, 'tree> EdgeManager<'db, 'tree> {
    pub fn new(ds: &'db SledHolder) -> Self {
        EdgeManager {
            holder: ds,
            tree: &ds.edges,
        }
    }

    fn key(&self, outbound_id: Uuid, t: &Type, inbound_id: Uuid) -> Vec<u8> {
        util::build(&[
            util::Component::Uuid(outbound_id),
            util::Component::Type(t),
            util::Component::Uuid(inbound_id),
        ])
    }

    pub fn get(&self, outbound_id: Uuid, t: &Type, inbound_id: Uuid) -> Result<Option<DateTime<Utc>>> {
        match map_err(self.tree.get(self.key(outbound_id, t, inbound_id)))? {
            Some(value_bytes) => {
                let mut cursor = Cursor::new(value_bytes.deref());
                Ok(Some(util::read_datetime(&mut cursor)))
            }
            None => Ok(None),
        }
    }

    pub fn set(&self, outbound_id: Uuid, t: &Type, inbound_id: Uuid, new_update_datetime: DateTime<Utc>) -> Result<()> {
        let edge_range_manager = EdgeRangeManager::new(self.holder);
        let reversed_edge_range_manager = EdgeRangeManager::new_reversed(self.holder);

        if let Some(update_datetime) = self.get(outbound_id, t, inbound_id)? {
            edge_range_manager.delete(outbound_id, t, update_datetime, inbound_id)?;
            reversed_edge_range_manager.delete(inbound_id, t, update_datetime, outbound_id)?;
        }

        let key = self.key(outbound_id, t, inbound_id);
        map_err(
            self.tree
                .insert(key, util::build(&[util::Component::DateTime(new_update_datetime)])),
        )?;
        edge_range_manager.set(outbound_id, t, new_update_datetime, inbound_id)?;
        reversed_edge_range_manager.set(inbound_id, t, new_update_datetime, outbound_id)?;
        Ok(())
    }

    pub fn delete(&self, outbound_id: Uuid, t: &Type, inbound_id: Uuid, update_datetime: DateTime<Utc>) -> Result<()> {
        map_err(self.tree.remove(&self.key(outbound_id, t, inbound_id)))?;

        let edge_range_manager = EdgeRangeManager::new(self.holder);
        edge_range_manager.delete(outbound_id, t, update_datetime, inbound_id)?;

        let reversed_edge_range_manager = EdgeRangeManager::new_reversed(self.holder);
        reversed_edge_range_manager.delete(inbound_id, t, update_datetime, outbound_id)?;

        let edge_property_manager = EdgePropertyManager::new(&self.holder.edge_properties);
        for item in edge_property_manager.iterate_for_owner(outbound_id, t, inbound_id)? {
            let ((edge_property_outbound_id, edge_property_t, edge_property_inbound_id, edge_property_name), _) = item?;
            edge_property_manager.delete(
                edge_property_outbound_id,
                &edge_property_t,
                edge_property_inbound_id,
                &edge_property_name[..],
            )?;
        }
        Ok(())
    }
}

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

    fn key(&self, first_id: Uuid, t: &Type, update_datetime: DateTime<Utc>, second_id: Uuid) -> Vec<u8> {
        util::build(&[
            util::Component::Uuid(first_id),
            util::Component::Type(t),
            util::Component::DateTime(update_datetime),
            util::Component::Uuid(second_id),
        ])
    }

    fn iterate<'it>(&self, iterator: DbIterator, prefix: Vec<u8>) -> impl Iterator<Item = Result<EdgeRangeItem>> + 'it {
        let filtered = take_while_prefixed(iterator, prefix);
        filtered.map(move |item| -> Result<EdgeRangeItem> {
            let (k, _) = map_err(item)?;
            let mut cursor = Cursor::new(k);
            let first_id = util::read_uuid(&mut cursor);
            let t = util::read_type(&mut cursor);
            let update_datetime = util::read_datetime(&mut cursor);
            let second_id = util::read_uuid(&mut cursor);
            Ok((first_id, t, update_datetime, second_id))
        })
    }

    pub fn iterate_for_range<'iter, 'trans: 'iter>(
        &'trans self,
        id: Uuid,
        t: Option<&Type>,
        high: Option<DateTime<Utc>>,
    ) -> Result<Box<dyn Iterator<Item = Result<EdgeRangeItem>> + 'iter>> {
        match t {
            Some(t) => {
                let high = high.unwrap_or_else(|| *util::MAX_DATETIME);
                let prefix = util::build(&[util::Component::Uuid(id), util::Component::Type(t)]);
                let low_key = util::build(&[
                    util::Component::Uuid(id),
                    util::Component::Type(t),
                    util::Component::DateTime(high),
                ]);
                let low_key_bytes: &[u8] = low_key.as_ref();
                let iterator = self.tree.range(low_key_bytes..);
                Ok(Box::new(self.iterate(iterator, prefix)))
            }
            None => {
                let prefix = util::build(&[util::Component::Uuid(id)]);
                let prefix_bytes: &[u8] = prefix.as_ref();
                let iterator = self.tree.range(prefix_bytes..);
                let mapped = self.iterate(iterator, prefix);

                if let Some(high) = high {
                    // We can filter out `update_datetime`s greater than
                    // `high` via key prefix filtering, so instead we handle
                    // it here - after the key has been deserialized.
                    let filtered = mapped.filter(move |item| {
                        if let Ok((_, _, update_datetime, _)) = *item {
                            update_datetime <= high
                        } else {
                            true
                        }
                    });

                    Ok(Box::new(filtered))
                } else {
                    Ok(Box::new(mapped))
                }
            }
        }
    }

    pub fn iterate_for_owner<'iter, 'trans: 'iter>(
        &'trans self,
        id: Uuid,
    ) -> impl Iterator<Item = Result<EdgeRangeItem>> + 'iter {
        let prefix: Vec<u8> = util::build(&[util::Component::Uuid(id)]);
        let iterator = self.tree.scan_prefix(&prefix);
        self.iterate(iterator, prefix)
    }

    pub fn set(&self, first_id: Uuid, t: &Type, update_datetime: DateTime<Utc>, second_id: Uuid) -> Result<()> {
        let key = self.key(first_id, t, update_datetime, second_id);
        map_err(self.tree.insert(&key, &[]))?;
        Ok(())
    }

    pub fn delete(&self, first_id: Uuid, t: &Type, update_datetime: DateTime<Utc>, second_id: Uuid) -> Result<()> {
        map_err(self.tree.remove(&self.key(first_id, t, update_datetime, second_id)))?;
        Ok(())
    }
}

pub struct VertexPropertyManager<'tree> {
    pub tree: &'tree Tree,
}

impl<'tree> VertexPropertyManager<'tree> {
    pub fn new(tree: &'tree Tree) -> Self {
        VertexPropertyManager { tree }
    }

    fn key(&self, vertex_id: Uuid, name: &str) -> Vec<u8> {
        util::build(&[
            util::Component::Uuid(vertex_id),
            util::Component::FixedLengthString(name),
        ])
    }

    pub fn iterate_for_owner(&self, vertex_id: Uuid) -> Result<impl Iterator<Item = Result<OwnedPropertyItem>> + '_> {
        let prefix = util::build(&[util::Component::Uuid(vertex_id)]);
        let iterator = self.tree.scan_prefix(&prefix);

        Ok(iterator.map(move |item| -> Result<OwnedPropertyItem> {
            let (k, v) = map_err(item)?;
            let mut cursor = Cursor::new(k);
            let owner_id = util::read_uuid(&mut cursor);
            debug_assert_eq!(vertex_id, owner_id);
            let name = util::read_fixed_length_string(&mut cursor);
            let value = serde_json::from_slice(&v)?;
            Ok(((owner_id, name), value))
        }))
    }

    pub fn get(&self, vertex_id: Uuid, name: &str) -> Result<Option<JsonValue>> {
        let key = self.key(vertex_id, name);

        match map_err(self.tree.get(&key))? {
            Some(value_bytes) => Ok(Some(serde_json::from_slice(&value_bytes)?)),
            None => Ok(None),
        }
    }

    pub fn set(&self, vertex_id: Uuid, name: &str, value: &JsonValue) -> Result<()> {
        let key = self.key(vertex_id, name);
        let value_json = serde_json::to_vec(value)?;
        map_err(self.tree.insert(key.as_slice(), value_json.as_slice()))?;
        Ok(())
    }

    pub fn delete(&self, vertex_id: Uuid, name: &str) -> Result<()> {
        map_err(self.tree.remove(&self.key(vertex_id, name)))?;
        Ok(())
    }
}

pub struct EdgePropertyManager<'tree> {
    pub tree: &'tree Tree,
}

impl<'tree> EdgePropertyManager<'tree> {
    pub fn new(tree: &'tree Tree) -> Self {
        EdgePropertyManager { tree }
    }

    fn key(&self, outbound_id: Uuid, t: &Type, inbound_id: Uuid, name: &str) -> Vec<u8> {
        util::build(&[
            util::Component::Uuid(outbound_id),
            util::Component::Type(t),
            util::Component::Uuid(inbound_id),
            util::Component::FixedLengthString(name),
        ])
    }

    pub fn iterate_for_owner<'a>(
        &'a self,
        outbound_id: Uuid,
        t: &'a Type,
        inbound_id: Uuid,
    ) -> Result<Box<dyn Iterator<Item = Result<EdgePropertyItem>> + 'a>> {
        let prefix = util::build(&[
            util::Component::Uuid(outbound_id),
            util::Component::Type(t),
            util::Component::Uuid(inbound_id),
        ]);

        let iterator = self.tree.scan_prefix(&prefix);

        let mapped = iterator.map(move |item| -> Result<EdgePropertyItem> {
            let (k, v) = map_err(item)?;
            let mut cursor = Cursor::new(k);

            let edge_property_outbound_id = util::read_uuid(&mut cursor);
            debug_assert_eq!(edge_property_outbound_id, outbound_id);

            let edge_property_t = util::read_type(&mut cursor);
            debug_assert_eq!(&edge_property_t, t);

            let edge_property_inbound_id = util::read_uuid(&mut cursor);
            debug_assert_eq!(edge_property_inbound_id, inbound_id);

            let edge_property_name = util::read_fixed_length_string(&mut cursor);

            let value = serde_json::from_slice(&v)?;
            Ok((
                (
                    edge_property_outbound_id,
                    edge_property_t,
                    edge_property_inbound_id,
                    edge_property_name,
                ),
                value,
            ))
        });

        Ok(Box::new(mapped))
    }

    pub fn get(&self, outbound_id: Uuid, t: &Type, inbound_id: Uuid, name: &str) -> Result<Option<JsonValue>> {
        let key = self.key(outbound_id, t, inbound_id, name);

        match map_err(self.tree.get(&key))? {
            Some(ref value_bytes) => Ok(Some(serde_json::from_slice(value_bytes)?)),
            None => Ok(None),
        }
    }

    pub fn set(&self, outbound_id: Uuid, t: &Type, inbound_id: Uuid, name: &str, value: &JsonValue) -> Result<()> {
        let key = self.key(outbound_id, t, inbound_id, name);
        let value_json = serde_json::to_vec(value)?;
        map_err(self.tree.insert(key.as_slice(), value_json.as_slice()))?;
        Ok(())
    }

    pub fn delete(&self, outbound_id: Uuid, t: &Type, inbound_id: Uuid, name: &str) -> Result<()> {
        map_err(self.tree.remove(&self.key(outbound_id, t, inbound_id, name)))?;
        Ok(())
    }
}

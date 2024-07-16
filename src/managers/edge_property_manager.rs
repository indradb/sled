use std::collections::HashMap;
use std::io::Cursor;

use indradb::{util, Edge, Identifier, Json};
use serde_json::Value as JsonValue;
use sled::{IVec, Tree};

use crate::errors::map_err;

pub type EdgePropertyItem = ((Edge, Identifier), JsonValue);

pub struct EdgePropertyManager<'tree> {
    pub tree: &'tree Tree,
    pub value_index_tree: &'tree Tree,
}

impl<'tree> EdgePropertyManager<'tree> {
    pub fn new(tree: &'tree Tree, value_index_tree: &'tree Tree) -> Self {
        EdgePropertyManager { tree, value_index_tree }
    }

    fn key(&self, edge: &Edge, name: Identifier) -> Vec<u8> {
        util::build(&[
            util::Component::Uuid(edge.outbound_id),
            util::Component::Identifier(edge.t),
            util::Component::Uuid(edge.inbound_id),
            util::Component::Identifier(name),
        ])
    }

    fn read_key(buf: IVec) -> (Edge, Identifier) {
        let mut cursor = Cursor::new(buf.as_ref());
        let edge_property_outbound_id = util::read_uuid(&mut cursor);
        let edge_property_t = util::read_identifier(&mut cursor);
        let edge_property_inbound_id = util::read_uuid(&mut cursor);
        let edge_property_name = util::read_identifier(&mut cursor);
        let edge = Edge {
            outbound_id: edge_property_outbound_id,
            t: edge_property_t,
            inbound_id: edge_property_inbound_id,
        };
        (edge, edge_property_name)
    }

    pub fn iterate_for_property_name(
        &self,
        name: Identifier,
    ) -> indradb::Result<impl Iterator<Item = indradb::Result<Edge>> + '_> {
        let prefix = util::build(&[util::Component::Identifier(name)]);
        let iterator = self.value_index_tree.scan_prefix(prefix);

        Ok(iterator.map(move |item| -> indradb::Result<Edge> {
            let (k, _v) = map_err(item)?;
            let (_p, _, edge) = Self::read_key_value_index(k);
            Ok(edge)
        }))
    }

    pub fn iterate_for_property_name_and_value(
        &'tree self,
        name: Identifier,
        value: &JsonValue,
    ) -> indradb::Result<impl Iterator<Item = indradb::Result<Edge>> + 'tree> {
        let value = value.clone();
        let prefix = util::build(&[
            util::Component::Identifier(name),
            util::Component::Json(&Json::new(value)),
        ]);
        let iterator = self.value_index_tree.scan_prefix(prefix);

        Ok(iterator.map(move |item| -> indradb::Result<Edge> {
            let (k, _) = map_err(item)?;
            let (_p, _, edge) = Self::read_key_value_index(k);
            Ok(edge)
        }))
    }

    pub fn iterate_for_owner<'a>(
        &'a self,
        edge: &Edge,
    ) -> indradb::Result<Box<dyn Iterator<Item = indradb::Result<EdgePropertyItem>> + 'a>> {
        let prefix = util::build(&[
            util::Component::Uuid(edge.outbound_id),
            util::Component::Identifier(edge.t),
            util::Component::Uuid(edge.inbound_id),
        ]);

        let iterator = self.tree.scan_prefix(prefix);
        let mapped = iterator.map(move |item| -> indradb::Result<EdgePropertyItem> {
            let (k, v) = map_err(item)?;
            let (edge, p_name) = Self::read_key(k);
            let value = serde_json::from_slice(&v)?;
            Ok(((edge, p_name), value))
        });

        Ok(Box::new(mapped))
    }

    pub fn get(&self, edge: &Edge, name: Identifier) -> indradb::Result<Option<JsonValue>> {
        let key = self.key(edge, name);

        match map_err(self.tree.get(key))? {
            Some(ref value_bytes) => Ok(Some(serde_json::from_slice(value_bytes)?)),
            None => Ok(None),
        }
    }

    fn key_value_index(edge: &Edge, value: &JsonValue, property_name: Identifier) -> Vec<u8> {
        util::build(&[
            util::Component::Identifier(property_name),
            util::Component::Json(&Json::new(value.clone())),
            util::Component::Uuid(edge.outbound_id),
            util::Component::Identifier(edge.t),
            util::Component::Uuid(edge.inbound_id),
        ])
    }

    fn read_key_value_index(buf: IVec) -> (Identifier, u64, Edge) {
        let mut cursor = Cursor::new(buf.as_ref());
        let name = util::read_identifier(&mut cursor);
        let value = util::read_u64(&mut cursor);
        let outbound_id = util::read_uuid(&mut cursor);
        let t = util::read_identifier(&mut cursor);
        let inbound_id = util::read_uuid(&mut cursor);
        (
            name,
            value,
            Edge {
                outbound_id,
                t,
                inbound_id,
            },
        )
    }

    pub fn set_batch(
        &self,
        edge: &Edge,
        batch: &mut sled::Batch,
        batch_value: &mut sled::Batch,
        property_creation_set: &mut HashMap<(Edge, Identifier), Vec<u8>>,
        name: Identifier,
        value: &JsonValue,
    ) -> indradb::Result<()> {
        let key = self.key(edge, name);
        let value_json = serde_json::to_vec(value)?;
        batch.insert(key.clone(), value_json);
        let old_value = map_err(self.tree.get(key.clone()))?;
        if let Some(old_value) = old_value {
            let old_value: Json = serde_json::from_slice(&old_value)?;
            let value_key = Self::key_value_index(edge, &old_value, name);
            batch_value.remove(value_key.as_slice());
        }
        let value_key = Self::key_value_index(edge, value, name);
        property_creation_set.insert((edge.clone(), name), value_key);
        Ok(())
    }

    pub fn set(&self, edge: &Edge, name: Identifier, value: &JsonValue) -> indradb::Result<()> {
        let key = self.key(edge, name);
        let value_json = serde_json::to_vec(value)?;

        let old_value = map_err(self.tree.get(key.clone()))?;
        if let Some(old_value) = old_value {
            let old_value: Json = serde_json::from_slice(&old_value)?;
            let value_key = Self::key_value_index(edge, &old_value, name);
            map_err(self.value_index_tree.remove(value_key.as_slice()))?;
        }

        map_err(self.tree.insert(key.as_slice(), value_json.as_slice()))?;
        let value_key = Self::key_value_index(edge, value, name);

        map_err(
            self.value_index_tree
                .insert(value_key.as_slice(), value_json.as_slice()),
        )?;
        Ok(())
    }

    pub fn delete(&self, edge: &Edge, name: Identifier) -> indradb::Result<()> {
        let old_value = map_err(self.tree.get(self.key(edge, name)))?;
        map_err(self.tree.remove(self.key(edge, name)))?;
        if let Some(old_value) = old_value {
            let old_value: Json = serde_json::from_slice(&old_value)?;
            let value_key = Self::key_value_index(edge, &old_value, name);
            map_err(self.value_index_tree.remove(value_key.as_slice()))?;
        }

        Ok(())
    }
}

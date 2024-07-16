use std::collections::HashSet;
use std::io::Cursor;
use std::sync::{Arc, RwLock};

use indradb::{util, Identifier};
use sled::Tree;

use crate::errors::{map_err, DSError};

const INDEXED_PROPERTIES: &str = "IndexedProperties";

pub struct MetaDataManager<'tree> {
    pub tree: &'tree Tree,
    indexed_properties: Arc<RwLock<HashSet<String>>>,
    index_key: Identifier,
}

impl<'tree> MetaDataManager<'tree> {
    pub fn new(tree: &'tree Tree) -> indradb::Result<Self> {
        let manager = MetaDataManager {
            tree,
            indexed_properties: Arc::new(RwLock::new(HashSet::new())),
            index_key: Identifier::new(INDEXED_PROPERTIES)?,
        };
        manager.load()?;
        Ok(manager)
    }

    pub fn is_indexed(&self, prop: &Identifier) -> indradb::Result<bool> {
        let indexed_properties = self.indexed_properties.read().map_err(DSError::from)?;

        let is_indexed = indexed_properties.contains(prop.as_str());
        Ok(is_indexed)
    }

    pub fn add_index(&self, prop: &Identifier) -> indradb::Result<()> {
        {
            let mut indexed_properties = self.indexed_properties.write().map_err(DSError::from)?;
            if indexed_properties.contains(prop.as_str()) {
                return Ok(());
            }
            indexed_properties.insert(prop.to_string());
        }
        self.sync()?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn remove_index(&self, prop: &Identifier) -> indradb::Result<()> {
        {
            let mut indexed_properties = self.indexed_properties.write().map_err(DSError::from)?;
            if !indexed_properties.contains(prop.as_str()) {
                return Ok(());
            }

            indexed_properties.remove(prop.as_str());
        }
        self.sync()?;
        Ok(())
    }

    fn load(&self) -> indradb::Result<()> {
        let mut indexed_properties = self.indexed_properties.write().map_err(DSError::from)?;
        let all_indexed_prefix = util::build(&[util::Component::Identifier(self.index_key)]);
        for index in self.tree.scan_prefix(all_indexed_prefix) {
            let (k, _) = map_err(index)?;
            let mut cursor = Cursor::new(k);
            let _ = util::read_identifier(&mut cursor);
            let prop = util::read_identifier(&mut cursor);

            indexed_properties.insert(prop.to_string());
        }
        Ok(())
    }

    pub(crate) fn sync(&self) -> indradb::Result<()> {
        let all_indexed_prefix = util::build(&[util::Component::Identifier(self.index_key)]);
        for index in self.tree.scan_prefix(all_indexed_prefix) {
            let (key, _) = map_err(index)?;
            map_err(self.tree.remove(key))?;
        }
        for index in self.indexed_properties.read().map_err(DSError::from)?.iter() {
            let key = util::build(&[
                util::Component::Identifier(self.index_key),
                util::Component::Identifier(Identifier::new(index)?),
            ]);
            map_err(self.tree.insert(key, &[]))?;
        }
        Ok(())
    }
}

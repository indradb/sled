use std::path::Path;

use indradb::{Datastore, Result};
use sled::{Config, Db, Tree};

use crate::managers::edge_manager::EdgeManager;
use crate::managers::edge_property_manager::EdgePropertyManager;
use crate::managers::edge_range_manager::EdgeRangeManager;
use crate::managers::metadata::MetaDataManager;
use crate::managers::vertex_manager::VertexManager;
use crate::managers::vertex_property_manager::VertexPropertyManager;
use crate::transaction::SledTransaction;

use super::errors::map_err;

#[derive(Copy, Clone, Default, Debug)]
pub struct SledConfig {
    use_compression: bool,
    compression_factor: Option<i32>,
}

impl SledConfig {
    /// Creates a new sled config with zstd compression enabled.
    ///
    /// # Arguments
    /// * `factor`: The zstd compression factor to use. If unspecified, this
    ///   will default to 5.
    pub fn with_compression(factor: Option<i32>) -> SledConfig {
        SledConfig {
            use_compression: true,
            compression_factor: factor,
        }
    }

    /// Creates a new sled datastore.
    pub fn open<P: AsRef<Path>>(self, path: P) -> Result<SledDatastore> {
        Ok(SledDatastore {
            holder: SledHolder::new(path, self)?,
        })
    }
}

/// The meat of a Sled datastore
pub struct SledHolder {
    pub(crate) db: Db, // Derefs to Tree, holds the vertices
    pub(crate) edges: Tree,
    pub(crate) edge_ranges: Tree,
    pub(crate) reversed_edge_ranges: Tree,
    pub(crate) vertex_properties: Tree,
    pub(crate) edge_properties: Tree,
    // for prop-name -> value -> ID prefix-indexed lookup
    pub(crate) edge_property_values: Tree,
    // for prop-name -> value -> UUID prefix-indexed lookup
    pub(crate) vertex_property_values: Tree,
    pub(crate) metadata: Tree,
}

impl SledHolder {
    /// The meat of a Sled datastore.
    ///
    /// # Arguments
    /// * `path`: The file path to the Sled database.
    /// * `opts`: Sled options to pass in.
    pub fn new<P: AsRef<Path>>(path: P, opts: SledConfig) -> Result<SledHolder> {
        let mut config = Config::default().path(path);

        if opts.use_compression {
            config = config.use_compression(true);
        }

        if let Some(compression_factor) = opts.compression_factor {
            config = config.compression_factor(compression_factor);
        }

        let db = map_err(config.open())?;

        Ok(SledHolder {
            edges: map_err(db.open_tree("edges"))?,
            edge_ranges: map_err(db.open_tree("edge_ranges"))?,
            reversed_edge_ranges: map_err(db.open_tree("reversed_edge_ranges"))?,
            vertex_properties: map_err(db.open_tree("vertex_properties"))?,
            edge_properties: map_err(db.open_tree("edge_properties"))?,
            vertex_property_values: map_err(db.open_tree("vertex_property_values"))?,
            edge_property_values: map_err(db.open_tree("edge_property_values"))?,
            metadata: map_err(db.open_tree("metadata"))?,
            db,
        })
    }
}

/// A datastore that is backed by Sled.
pub struct SledDatastore {
    pub(crate) holder: SledHolder,
}

impl SledDatastore {
    /// Creates a new Sled datastore.
    ///
    /// # Arguments
    /// * `path`: The file path to the Sled database.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<SledDatastore> {
        Ok(SledDatastore {
            holder: SledHolder::new(path, SledConfig::default())?,
        })
    }
}

impl Datastore for SledDatastore {
    type Transaction<'a> = SledTransaction<'a>
    where
        Self: 'a;

    fn transaction(&self) -> Self::Transaction<'_> {
        SledTransaction {
            holder: &self.holder,
            vertex_manager: VertexManager::new(&self.holder),
            edge_manager: EdgeManager::new(&self.holder),
            edge_range_manager: EdgeRangeManager::new(&self.holder),
            edge_range_manager_rev: EdgeRangeManager::new_reversed(&self.holder),
            edge_property_manager: EdgePropertyManager::new(
                &self.holder.edge_properties,
                &self.holder.edge_property_values,
            ),
            vertex_property_manager: VertexPropertyManager::new(
                &self.holder.vertex_properties,
                &self.holder.vertex_property_values,
            ),
            meta_data_manager: MetaDataManager::new(&self.holder.metadata).unwrap(),
        }
    }
}

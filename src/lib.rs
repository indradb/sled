//! The Sled datastore implementation.

#![cfg_attr(feature = "bench-suite", feature(test))]

extern crate chrono;
extern crate ecow;
#[cfg(any(feature = "bench-suite", feature = "test-suite"))]
#[macro_use]
extern crate indradb;
#[cfg(not(any(feature = "bench-suite", feature = "test-suite")))]
extern crate indradb;
extern crate serde_json;
extern crate sled;
#[cfg(any(feature = "bench-suite", feature = "test-suite"))]
extern crate tempfile;
extern crate thiserror;
extern crate uuid;

use indradb::Edge;

pub use self::datastore::{SledConfig, SledDatastore};

mod datastore;
mod errors;
mod managers;
mod transaction;

mod normal_config {

    #[cfg(feature = "bench-suite")]
    full_bench_impl!({
        use super::SledDatastore;
        use indradb::Database;
        use tempfile::tempdir;
        let path = tempdir().unwrap().into_path();
        Database::new(SledDatastore::new(path).unwrap())
    });

    #[cfg(feature = "test-suite")]
    full_test_impl!({
        use super::SledDatastore;
        use indradb::Database;
        use tempfile::tempdir;
        let path = tempdir().unwrap().into_path();
        Database::new(SledDatastore::new(path).unwrap())
    });
}

mod compression_config {

    #[cfg(feature = "bench-suite")]
    full_bench_impl!({
        use super::SledConfig;
        use indradb::Database;
        use tempfile::tempdir;
        let path = tempdir().unwrap().into_path();
        Database::new(SledConfig::with_compression(None).open(path).unwrap())
    });

    #[cfg(feature = "test-suite")]
    full_test_impl!({
        use super::SledConfig;
        use indradb::Database;
        use tempfile::tempdir;
        let path = tempdir().unwrap().into_path();
        Database::new(SledConfig::with_compression(None).open(path).unwrap())
    });
}

fn reverse_edge(edge: &Edge) -> Edge {
    Edge {
        outbound_id: edge.inbound_id,
        t: edge.t,
        inbound_id: edge.outbound_id,
    }
}

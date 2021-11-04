//! The Sled datastore implementation.

extern crate chrono;
extern crate indradb;
extern crate serde_json;
extern crate sled;
extern crate uuid;

mod datastore;
mod errors;
mod managers;

pub use self::datastore::{SledConfig, SledDatastore, SledTransaction};

mod normal_config {
    #[cfg(feature = "bench-suite")]
    full_bench_impl!({
        use super::SledDatastore;
        use tempfile::tempdir;
        let path = tempdir().unwrap().into_path();
        SledDatastore::new(path).unwrap()
    });

    #[cfg(feature = "test-suite")]
    full_test_impl!({
        use super::SledDatastore;
        use tempfile::tempdir;
        let path = tempdir().unwrap().into_path();
        SledDatastore::new(path).unwrap()
    });
}

mod compression_config {
    #[cfg(feature = "bench-suite")]
    full_bench_impl!({
        use super::SledConfig;
        use tempfile::tempdir;
        let path = tempdir().unwrap().into_path();
        SledConfig::with_compression(None).open(path).unwrap()
    });

    #[cfg(feature = "test-suite")]
    full_test_impl!({
        use super::SledConfig;
        use tempfile::tempdir;
        let path = tempdir().unwrap().into_path();
        SledConfig::with_compression(None).open(path).unwrap()
    });
}

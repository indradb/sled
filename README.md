# IndraDB Sled Implementation [![Docs](https://docs.rs/indradb-sled/badge.svg)](https://docs.rs/indradb-sled)

This is an implementation of the IndraDB datastore for sled.

The sled datastore is not production-ready yet. sled itself is pre-1.0, and makes no guarantees about on-disk format stability. Upgrading IndraDB may require you to [manually migrate the sled datastore.](https://docs.rs/sled/0.34.6/sled/struct.Db.html#method.export) Additionally, there is a standing issue that prevents the sled datastore from having the same level of safety as the RocksDB datastore.

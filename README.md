# IndraDB Sled Implementation [![Docs](https://docs.rs/indradb-sled/badge.svg)](https://docs.rs/indradb-sled)

This is an implementation of the IndraDB datastore for sled.

## General hints

The sled datastore is not production-ready yet. sled itself is pre-1.0, and makes no guarantees about on-disk format
stability.
Upgrading IndraDB may require you
to [manually migrate the sled datastore.](https://docs.rs/sled/0.34.6/sled/struct.Db.html#method.export)
Additionally, there is a standing issue that prevents the sled datastore from having the same level of safety as the
RocksDB datastore.
Performance of sled in two-hop-queries is better than rocksdb though. We measured about 40% faster query times.

## TODO / Notices:

- This version does index all properties by default (and therefore duplicates the data).
  This is adding the benefit of faster queries but also increases the storage requirements.
- Batch operations are not optimized yet, 

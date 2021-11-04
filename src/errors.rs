use indradb::Error as IndraError;
use sled::Error as SledError;

pub(crate) fn map_err<T>(result: Result<T, SledError>) -> Result<T, IndraError> {
    result.map_err(|err| IndraError::Datastore { inner: Box::new(err) })
}

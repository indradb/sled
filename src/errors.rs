use std::sync::PoisonError;

use indradb::Error as IndraError;
use sled::Error as SledError;

pub(crate) fn map_err<T>(result: Result<T, SledError>) -> Result<T, IndraError> {
    result.map_err(|err| IndraError::Datastore(Box::new(err)))
}

#[derive(Debug, thiserror::Error)]
pub enum DSError {
    #[error("Error in locking a RwLock: {0}")]
    PoisonError(String),
}

impl<T> From<PoisonError<T>> for DSError {
    fn from(value: PoisonError<T>) -> Self {
        DSError::PoisonError(value.to_string())
    }
}

impl From<DSError> for IndraError {
    fn from(err: DSError) -> Self {
        IndraError::Datastore(Box::new(err))
    }
}

use bevy::ecs::entity::EntityValidButNotSpawnedError;
use thiserror::Error;

use super::inner_storage::Storage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ValueStorageError {
    #[error("The stored value has been dropped. {0}")]
    ValueDropped(EntityValidButNotSpawnedError),
    #[error("Type mismatch: expected {requested_type}, found {value_type}")]
    TypeMismatch {
        value_type: &'static str,
        requested_type: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ValueReadError<S: Storage<Value = T>, T> {
    #[error("Failed to read value: {0}")]
    StorageError(S::ReadError),
    #[error(transparent)]
    ValueStorageError(#[from] ValueStorageError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ValueWriteError<S: Storage<Value = T>, T> {
    #[error("Failed to write value: {0}")]
    StorageError(S::WriteError),
    #[error(transparent)]
    ValueStorageError(#[from] ValueStorageError),
}

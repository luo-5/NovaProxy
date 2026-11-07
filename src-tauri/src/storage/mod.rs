//! SQLite and configuration storage
//! Implemented in Phase P1.

pub mod db;

pub use db::{Database, StorageError, StorageResult};

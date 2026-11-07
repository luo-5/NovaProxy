//! SQLite and configuration storage
//! Implemented in Phase P1 & P6.

pub mod atomic;
pub mod db;

pub use atomic::atomic_write_file;
pub use db::{Database, StorageError, StorageResult};

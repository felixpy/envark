pub mod app_release;
pub mod config;
pub mod engine;
pub mod error;
pub mod filesystem;
pub(crate) mod git;
pub mod model;
pub mod operations;
pub mod persistence;
pub mod process;
pub mod providers;
pub mod scan_cache;
pub mod scanner;
mod worktree_removal;
#[cfg(test)]
mod worktree_removal_tests;

pub use error::{Error, Result};
pub mod artifact_policy;

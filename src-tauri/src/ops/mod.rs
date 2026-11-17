//! Applying a reviewed plan to disk, and undoing it. The only code that
//! changes the filesystem.

pub mod apply;
pub mod log;
pub mod preview;
pub mod trash;

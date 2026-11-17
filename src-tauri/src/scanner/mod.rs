//! Local, offline scanning of a folder and aggregation for the heatmap.
//! Nothing in this module calls the network or modifies the filesystem.

pub mod category;
pub mod duplicates;
pub mod heatmap;
pub mod sensitive;
pub mod types;
pub mod walk;

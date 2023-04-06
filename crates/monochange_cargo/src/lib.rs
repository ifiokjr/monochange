#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

doc_comment::doctest!("../readme.md");

pub use cargo_package::*;
pub use get_dependents_graph::*;
pub use get_packages::*;

mod cargo_package;
mod get_dependents_graph;
mod get_packages;

#[cfg(test)]
mod __tests;

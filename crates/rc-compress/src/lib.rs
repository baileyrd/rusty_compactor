//! Deterministic, caveman-inspired text compression: shrinks prose while
//! leaving code, commands, and error output byte-for-byte untouched.

pub mod engine;
pub mod level;
pub mod rules;
pub mod segment;

pub use engine::{compress, compress_prose, CompressResult};
pub use level::Level;

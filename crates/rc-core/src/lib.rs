pub mod command;
pub mod config;
pub mod result;
pub mod stats;

pub use command::{split_compound, ChainLink, CommandOutput, ParsedCommand};
pub use config::Config;
pub use result::CompactedOutput;

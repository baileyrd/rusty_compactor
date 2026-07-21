use std::fmt;
use std::str::FromStr;

/// Compression aggressiveness, mirroring caveman's `/caveman [level]` modes.
/// Each level is a strict superset of the transforms in the level before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Level {
    /// Strip filler/hedge phrases and collapse whitespace. No grammar changes.
    Lite,
    /// Lite + contractions + drop articles (a/an/the). The default level.
    #[default]
    Full,
    /// Full + drop sentence-initial discourse markers + dev-jargon abbreviations.
    Ultra,
    /// Ultra + collapse wordy connectors ("for example" -> "e.g.") and
    /// coordinate clauses. The most aggressive level.
    Wenyan,
}

/// Error returned by [`Level::from_str`] for an unrecognized level name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseLevelError(String);

impl fmt::Display for ParseLevelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParseLevelError {}

impl FromStr for Level {
    type Err = ParseLevelError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "lite" => Ok(Level::Lite),
            "full" => Ok(Level::Full),
            "ultra" => Ok(Level::Ultra),
            "wenyan" => Ok(Level::Wenyan),
            other => Err(ParseLevelError(format!(
                "unknown compression level {other:?}; expected lite, full, ultra, or wenyan"
            ))),
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Level::Lite => "lite",
            Level::Full => "full",
            Level::Ultra => "ultra",
            Level::Wenyan => "wenyan",
        };
        write!(f, "{s}")
    }
}

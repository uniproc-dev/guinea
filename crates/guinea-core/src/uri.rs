use std::fmt;
use std::ops::Deref;

pub use http::uri::{InvalidUri, PathAndQuery};


#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppUri(PathAndQuery);

impl AppUri {
    pub fn parse(s: impl AsRef<str>) -> Result<Self, InvalidUri> {
        Ok(Self(s.as_ref().parse()?))
    }

    /// Non-empty path segments, in order - `/ubuntu/processes` ->
    /// `["ubuntu", "processes"]`.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.path().split('/').filter(|s| !s.is_empty())
    }

    /// The `n`th non-empty path segment (0-indexed).
    pub fn segment(&self, n: usize) -> Option<&str> {
        self.segments().nth(n)
    }
}

impl Deref for AppUri {
    type Target = PathAndQuery;
    fn deref(&self) -> &PathAndQuery {
        &self.0
    }
}

impl fmt::Display for AppUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

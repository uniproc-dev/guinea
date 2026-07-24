use std::fmt;
use std::ops::Deref;

pub use http::uri::{InvalidUri, PathAndQuery};

/// An in-process route address - a bare path (+ optional query string), no
/// scheme or host (this never crosses the network; it only identifies
/// "which segment, with which params" within the running app). Wraps
/// `http::uri::PathAndQuery` rather than a hand-rolled struct, so query
/// strings (deep-link params) are real, correctly-parsed URI syntax instead
/// of a bespoke reimplementation - `Deref`s to it for `.path()`/`.query()`;
/// `segments`/`segment` are this app's own convenience on top, for reading
/// out positional path segments (e.g. this app's `:context` convention).
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

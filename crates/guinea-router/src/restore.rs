//! Writing a route down, and reading it back after a restart.
//!
//! The third tier of reachability, and the only one that outlives the process.
//! `link` says an address exists; `restorable` says the route can be
//! reconstructed from nothing but text - which is a stronger claim, and the
//! compiler is made to prove it: every field of a restorable route has to
//! survive the round trip, and a field that cannot fails to build at the field
//! rather than at run time on the one machine that had a saved session.
//!
//! What is *not* here is where the text goes. The router has no opinion about
//! storage, the same way it has none about drawing: an application hands the
//! string to whatever it already keeps state in.
//!
//! ```ignore
//! // On the way out.
//! if let Some(route) = router.current_route::<Route>() {
//!     store.set("route", route.save());
//! }
//!
//! // On the way back.
//! let route = store.get("route").and_then(|text| Route::restore(&text));
//! router.navigate(route.unwrap_or_else(Route::home))?;
//! ```
//!
//! The two halves below exist so that generated code never names `serde`. An
//! application depends on guinea; making it depend on a serialisation crate
//! because of a keyword in a macro would be the macro leaking its
//! implementation.

use std::collections::BTreeMap;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// A route being written down.
///
/// Fields land in a `BTreeMap` rather than in declaration order, so the same
/// route always produces the same text: a saved session that differs only in
/// key order would look like a change to anything comparing them.
///
/// This is where `restorable` stops being a keyword and becomes a proof. A
/// field that cannot make the round trip does not compile, and because
/// `routes!` writes the call with the field's own span, the error lands on the
/// declaration rather than inside an expansion:
///
/// ```compile_fail
/// use guinea_router::restore::Saving;
///
/// let channel = std::sync::mpsc::channel::<u8>().1;
///
/// let mut saving = Saving::new("Wizard");
/// saving.field("result", &channel);
/// ```
pub struct Saving {
    route: &'static str,
    fields: BTreeMap<&'static str, serde_json::Value>,
}

impl Saving {
    pub fn new(route: &'static str) -> Self {
        Self {
            route,
            fields: BTreeMap::new(),
        }
    }

    /// `None` when the value refused to serialise, which takes the whole route
    /// with it - half a route is worse than none, since restoring it would put
    /// the application somewhere the user never was.
    pub fn field<T: Serialize>(&mut self, name: &'static str, value: &T) -> Option<()> {
        self.fields.insert(name, serde_json::to_value(value).ok()?);
        Some(())
    }

    pub fn finish(self) -> Option<String> {
        let mut whole = serde_json::Map::new();
        whole.insert("route".to_string(), self.route.into());
        whole.insert(
            "fields".to_string(),
            serde_json::Value::Object(self.fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
        );
        serde_json::to_string(&serde_json::Value::Object(whole)).ok()
    }
}

/// A route being read back.
pub struct Restoring {
    fields: serde_json::Map<String, serde_json::Value>,
}

impl Restoring {
    /// The route's name and its fields, or `None` for anything that is not a
    /// route this wrote.
    ///
    /// Every failure here is `None` rather than an error: what is being read
    /// is a file from a previous run, possibly a previous *version*, and a
    /// route that no longer exists is an ordinary thing to find. The
    /// application falls back to wherever it starts.
    pub fn open(text: &str) -> Option<(String, Self)> {
        let serde_json::Value::Object(whole) = serde_json::from_str(text).ok()? else {
            return None;
        };

        let route = whole.get("route")?.as_str()?.to_string();
        let serde_json::Value::Object(fields) = whole.get("fields")?.clone() else {
            return None;
        };

        Some((route, Self { fields }))
    }

    pub fn field<T: DeserializeOwned>(&self, name: &str) -> Option<T> {
        serde_json::from_value(self.fields.get(name)?.clone()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_survives_the_round_trip() {
        let mut saving = Saving::new("Processes");
        saving.field("context", &"ubuntu".to_string()).expect("serialises");
        saving.field("tab", &3u8).expect("serialises");
        let text = saving.finish().expect("a whole route");

        let (route, fields) = Restoring::open(&text).expect("reads back");
        assert_eq!(route, "Processes");
        assert_eq!(fields.field::<String>("context").as_deref(), Some("ubuntu"));
        assert_eq!(fields.field::<u8>("tab"), Some(3));
    }

    #[test]
    fn the_same_route_always_writes_the_same_text() {
        let text = |first: bool| {
            let mut saving = Saving::new("Processes");
            let pairs: [(&'static str, u8); 2] = [("a", 1), ("b", 2)];
            for (name, value) in if first { pairs } else { [pairs[1], pairs[0]] } {
                saving.field(name, &value).expect("serialises");
            }
            saving.finish().expect("a whole route")
        };

        assert_eq!(text(true), text(false), "key order is not a change");
    }

    #[test]
    fn what_a_previous_version_wrote_is_read_as_nothing() {
        // A saved session outlives the build that wrote it, so a route that no
        // longer exists, or a field that changed type, is an ordinary thing to
        // find rather than an error to report.
        assert!(Restoring::open("not json").is_none());
        assert!(Restoring::open("{}").is_none());
        assert!(Restoring::open(r#"{"route":"Gone"}"#).is_none());

        let (_, fields) = Restoring::open(r#"{"route":"Here","fields":{"id":"seven"}}"#)
            .expect("the shape is still a route");
        assert_eq!(fields.field::<u32>("id"), None, "the field changed type");
        assert_eq!(fields.field::<u32>("missing"), None);
    }
}

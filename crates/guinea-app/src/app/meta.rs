/// Who this application is.
///
/// One source for the handful of strings that several plugins need and that
/// nothing else can supply: the store wants a directory to write in, an
/// updater wants a version to compare, a crash reporter wants a publisher.
/// Passing them plugin by plugin means the same string typed twice, and when
/// the copies drift the data quietly lands in two places.
///
/// The values come from the application's manifest through `guinea-meta`,
/// which generates them as constants at build time - see [`crate::app_meta`].
/// Nothing here is guessed from the Cargo package: an application's identity
/// outlives the crate that happens to build it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppMeta {
    /// Shown to a person: "Processes".
    pub name: &'static str,
    /// Written for a machine, and stable across renames: reverse-DNS, as in
    /// "dev.uniproc.processes". This is what decides where data lives.
    pub identifier: &'static str,
    pub version: &'static str,
    pub publisher: &'static str,
}

impl AppMeta {
    pub const fn new(
        name: &'static str,
        identifier: &'static str,
        version: &'static str,
        publisher: &'static str,
    ) -> Self {
        Self {
            name,
            identifier,
            version,
            publisher,
        }
    }
}

/// Builds [`AppMeta`] from the constants `guinea_meta::manifest!()` generated
/// in this crate.
///
/// ```ignore
/// guinea_meta::manifest!();          // once, at the crate root
///
/// GuineaApp::new()
///     .meta(guinea::app_meta!())
///     .plugin(StorePlugin::new())    // no application name to repeat
/// ```
///
/// A plugin that needs any of it asks for `AppMeta` like any other service.
#[macro_export]
macro_rules! app_meta {
    () => {
        $crate::app::AppMeta::new(
            crate::APP_NAME,
            crate::APP_IDENTIFIER,
            crate::APP_VERSION,
            crate::APP_PUBLISHER,
        )
    };
}

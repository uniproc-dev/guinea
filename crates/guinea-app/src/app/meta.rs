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

/// Declares this application's identity from its manifest, in one call.
///
/// ```ignore
/// guinea::app_meta!();               // once, at the crate root
///
/// windows_reactor::App::new()
///     .title(WINDOW_TITLE)           // the manifest's, not a literal
///     .render(root)
///
/// fn root(cx: &mut RenderCx) -> Element {
///     GuineaApp::new().meta(APP_META).bootstrap(cx);
///     ...
/// }
/// ```
///
/// Expands to the constants `guinea-meta` generated at build time - `APP_NAME`,
/// `APP_IDENTIFIER`, `APP_VERSION`, `APP_PUBLISHER`, `WINDOW_TITLE`,
/// `WINDOW_ICON` - plus `APP_META` built from them. Nothing else needs
/// including: the generated file is read from this crate's own `OUT_DIR`,
/// because the macro expands where it is called.
#[macro_export]
macro_rules! app_meta {
    () => {
        include!(concat!(env!("OUT_DIR"), "/guinea_meta.rs"));

        /// This application's identity, for `GuineaApp::meta`.
        #[allow(dead_code)]
        pub const APP_META: $crate::app::AppMeta = $crate::app::AppMeta::new(
            APP_NAME,
            APP_IDENTIFIER,
            APP_VERSION,
            APP_PUBLISHER,
        );
    };
}

//! What a shell can be asked about the window a root sits in.
//!
//! guinea has no windows of its own - it has [roots](super::roots), and a
//! shell puts each one somewhere: a window under WinUI or Slint, a webview
//! under Tauri, the terminal itself under ratatui. This is the contract for
//! talking to that somewhere, so that a plugin remembering where a window was
//! does not have to know which of them it is.
//!
//! Written the way winit writes it, because winit is the abstraction being
//! described here and it already learned the hard part: a capability can be
//! missing. `is_minimized()` returns an `Option`, `drag_window()` a `Result` -
//! not because those calls fail often, but because a platform may simply not
//! have the concept. Same here: a shell answers [`Unsupported`] rather than
//! forcing every contract down to what the poorest backend can do.

use std::sync::Arc;

use guinea_core::actor::traits::Message;

use super::roots::RootId;

/// Size in logical pixels - what the application asked for, before the
/// display's scale factor is applied.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// Position of the window's top-left corner, in logical pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

/// Where a window is and how it is shown.
///
/// Every part is optional because every part may be unknown: a shell that
/// cannot report the position leaves it `None` rather than inventing a zero,
/// and a plugin restoring geometry then simply does not restore what it never
/// learned.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Geometry {
    pub size: Option<Size>,
    pub position: Option<Position>,
    pub maximized: bool,
    pub fullscreen: bool,
    /// Minimised windows have no geometry worth keeping - Windows answers
    /// `-32000, -32000` for one, and restoring that puts the window off every
    /// screen. A shell reporting `minimized` leaves `size` and `position`
    /// `None`, so nothing downstream has to know that number.
    pub minimized: bool,
}

/// The shell does not have this concept, or cannot do it here.
///
/// Not an error in the usual sense - nothing went wrong, and a caller that
/// can carry on should. A terminal has no window to move.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Unsupported;

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the shell does not support this")
    }
}

impl std::error::Error for Unsupported {}

pub type Done = Result<(), Unsupported>;

/// A window moved, was resized, or changed how it is shown.
///
/// Published on the global bus by whoever owns the window, which is the only
/// party that can know. Carries the whole geometry rather than what changed:
/// what listens is code that stores it, and storing half of it is worse than
/// storing it whole.
#[derive(Clone, Copy, Debug)]
pub struct WindowChanged {
    pub root: RootId,
    pub geometry: Geometry,
}

impl Message for WindowChanged {}

/// Commands to the windows an application's roots live in.
///
/// A shell provides one; anything that needs it asks
/// `try_require::<WindowService>()` - `try`, because a backend without windows
/// provides nothing and that is a normal state, not a misconfiguration.
pub trait Windows: Send + Sync + 'static {
    /// Where the root's window is now, as far as the shell knows.
    fn geometry(&self, root: RootId) -> Option<Geometry>;

    /// Puts the window where `geometry` says. Parts left `None` are left
    /// alone, so restoring a size without a position is one call.
    fn apply(&self, root: RootId, geometry: Geometry) -> Done;

    fn set_visible(&self, root: RootId, visible: bool) -> Done;

    fn close(&self, root: RootId) -> Done;

    /// Starts dragging the window, for a title bar the application drew
    /// itself. Ends when the pointer is released - there is nothing to stop.
    fn start_drag(&self, root: RootId) -> Done;
}

/// Where a window should open, according to something that remembers.
///
/// The inverse of the usual direction: a plugin provides this, and the *shell*
/// asks - because only the shell knows the moment before a window is shown,
/// and restoring a size after that is a visible jump. Keyed by
/// [label](super::roots::label) rather than by [`RootId`], since ids last one
/// run and what this answers was learned in an earlier one.
pub trait RestoreGeometry: Send + Sync + 'static {
    fn for_label(&self, label: &str) -> Option<Geometry>;
}

/// The service key for [`RestoreGeometry`].
#[derive(Clone)]
pub struct SavedGeometry(Arc<dyn RestoreGeometry>);

impl SavedGeometry {
    pub fn new(source: impl RestoreGeometry) -> Self {
        Self(Arc::new(source))
    }

    pub fn from_arc(source: Arc<dyn RestoreGeometry>) -> Self {
        Self(source)
    }
}

impl std::ops::Deref for SavedGeometry {
    type Target = dyn RestoreGeometry;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

/// The service key: a contract, not an implementation.
///
/// Whoever provides it decides what a window is - the same trick
/// `guinea-plugin-store` gets from `amethystate::Store`, and the reason a
/// plugin written against this keeps working when the shell is replaced.
#[derive(Clone)]
pub struct WindowService(Arc<dyn Windows>);

impl WindowService {
    pub fn new(windows: impl Windows) -> Self {
        Self(Arc::new(windows))
    }

    pub fn from_arc(windows: Arc<dyn Windows>) -> Self {
        Self(windows)
    }
}

impl std::ops::Deref for WindowService {
    type Target = dyn Windows;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shell that knows where its one window is and refuses everything else,
    /// the way a poor backend would.
    struct Meagre;

    impl Windows for Meagre {
        fn geometry(&self, _root: RootId) -> Option<Geometry> {
            Some(Geometry {
                size: Some(Size {
                    width: 420.0,
                    height: 420.0,
                }),
                ..Geometry::default()
            })
        }

        fn apply(&self, _root: RootId, _geometry: Geometry) -> Done {
            Err(Unsupported)
        }

        fn set_visible(&self, _root: RootId, _visible: bool) -> Done {
            Err(Unsupported)
        }

        fn close(&self, _root: RootId) -> Done {
            Err(Unsupported)
        }

        fn start_drag(&self, _root: RootId) -> Done {
            Err(Unsupported)
        }
    }

    #[test]
    fn a_shell_answers_for_what_it_has_and_says_so_for_the_rest() {
        let service = WindowService::new(Meagre);
        let root = super::super::roots::Registration::open();

        let geometry = service.geometry(root.id()).expect("this one it knows");
        assert_eq!(geometry.size.unwrap().width, 420.0);
        assert_eq!(geometry.position, None, "unknown is not zero");

        // A caller that can carry on should: nothing went wrong, the shell
        // just has no such concept.
        assert_eq!(service.start_drag(root.id()), Err(Unsupported));
    }
}

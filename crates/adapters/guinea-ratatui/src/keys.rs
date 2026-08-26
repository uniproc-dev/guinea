//! One thing about keys that is worth not rediscovering.

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};

/// The key, if this event is a press.
///
/// A terminal reports releases and repeats as well, so matching on `KeyCode`
/// alone acts three times on one keystroke - which looks like a navigation
/// bug long before it looks like an input one.
pub fn pressed(event: &Event) -> Option<KeyCode> {
    match event {
        Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) => Some(*code),
        _ => None,
    }
}

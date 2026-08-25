//! Owning the terminal: install the application, draw, read input, repeat.
//!
//! Unlike the WinUI backend, which borrows the reactor's loop, here the loop
//! is ours. That makes this the place where everything meets: the frame, the
//! router, the tasks actors queued from other threads, and the keys.

use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::time::Duration;

use guinea_app::app::{GuineaApp, install_runtime, shutdown_current};
use guinea_core::actor::UiThreadToken;
use guinea_router::router::{NavigateHandle, RouteChain, RouteSink, Router, ToUri};
use ratatui::crossterm::event::{self, Event};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;

use crate::{Tui, dispatcher};

/// What the application wants after an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Exit,
}

/// How often the loop wakes with nothing to do. Sets the worst-case delay
/// before work an actor finished on another thread reaches the screen.
const TICK: Duration = Duration::from_millis(50);

/// Takes over the terminal and runs until `on_event` says to stop.
///
/// `on_event` is handed every input event and the navigator, so routing stays
/// the application's decision - the same split as the reactor backend, where
/// keys are the application's business and the router only obeys.
pub fn run<R, F>(app: GuineaApp, initial: R, mut on_event: F) -> anyhow::Result<()>
where
    R: RouteChain<Tui> + ToUri + Clone + PartialEq + 'static,
    F: FnMut(&Event, &NavigateHandle<Tui, R>) -> Flow,
{
    // Before any actor exists: the first thing a feature does during install
    // may already queue work back to this thread.
    dispatcher::install();

    // Genuinely this thread: it is the one that will draw, and nothing else
    // touches the router or the scopes.
    let token = UiThreadToken::dangerously_create_token_unchecked();
    install_runtime(app.install(token.clone())?);

    let router = Rc::new(Router::<Tui>::new(token));
    let route = Rc::new(RefCell::new(initial.clone()));
    let nav = NavigateHandle::new(router.clone(), {
        let route = route.clone();
        RouteSink::new(move |next: R| *route.borrow_mut() = next)
    });

    router.navigate(initial.clone(), &initial.to_uri())?;

    let mut terminal = enter()?;
    let outcome = pump(&mut terminal, &router, &nav, &mut on_event);
    // Restore the terminal before reporting anything: a failure that leaves
    // the screen in raw mode is unreadable, including its own error message.
    leave(&mut terminal)?;

    shutdown_current();
    outcome
}

fn pump<R, F>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    router: &Router<Tui>,
    nav: &NavigateHandle<Tui, R>,
    on_event: &mut F,
) -> anyhow::Result<()>
where
    R: RouteChain<Tui> + ToUri + Clone + PartialEq + 'static,
    F: FnMut(&Event, &NavigateHandle<Tui, R>) -> Flow,
{
    loop {
        terminal.draw(|frame| router.render().draw(frame, frame.area()))?;

        if event::poll(TICK)? {
            let event = event::read()?;
            if on_event(&event, nav) == Flow::Exit {
                return Ok(());
            }
        }

        // After the keys, so a navigation made above is already installed and
        // whatever it started can run in the same breath.
        dispatcher::drain();
    }
}

fn enter() -> anyhow::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn leave(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

#[cfg(feature = "crossterm")]
use std::time::Duration;

#[cfg(feature = "crossterm")]
use crossterm::event::{self, Event as CEvent, KeyEvent};

#[derive(Debug, Clone, Copy)]
pub enum Event {
    Tick,
    #[cfg(feature = "crossterm")]
    Key(KeyEvent),
}

#[cfg(feature = "crossterm")]
pub fn poll_event(tick_ms: u64) -> anyhow::Result<Event> {
    if event::poll(Duration::from_millis(tick_ms))? {
        if let CEvent::Key(key) = event::read()? {
            return Ok(Event::Key(key));
        }
    }
    Ok(Event::Tick)
}

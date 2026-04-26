#[cfg(feature = "crossterm")]
use std::io;

#[cfg(feature = "crossterm")]
use crossterm::event::KeyCode;
#[cfg(feature = "crossterm")]
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
#[cfg(feature = "crossterm")]
use crossterm::{event::KeyModifiers, execute};
#[cfg(feature = "crossterm")]
use ratatui::backend::CrosstermBackend;
#[cfg(feature = "crossterm")]
use ratatui::Terminal;

#[cfg(feature = "crossterm")]
use crate::app::{App, Tab};
#[cfg(feature = "crossterm")]
use crate::event::{poll_event, Event};
#[cfg(feature = "crossterm")]
use crate::model::types::UnitsMode;
#[cfg(feature = "crossterm")]
use crate::ui;

#[cfg(feature = "crossterm")]
pub fn run(app: &mut App, rx: std::sync::mpsc::Receiver<anyhow::Result<crate::model::types::Snapshot>>) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let out = run_loop(&mut terminal, app, rx);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    out
}

#[cfg(feature = "crossterm")]
fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    rx: std::sync::mpsc::Receiver<anyhow::Result<crate::model::types::Snapshot>>,
) -> anyhow::Result<()> {
    loop {
        while let Ok(s) = rx.try_recv() {
            match s {
                Ok(snap) => {
                    app.last_error = None;
                    app.ingest(snap);
                }
                Err(e) => {
                    app.last_error = Some(format!("{e:#}"));
                }
            }
        }

        terminal.draw(|f| ui::draw(f, app))?;

        match poll_event(80)? {
            Event::Tick => {}
            Event::Key(k) => {
                if k.modifiers.contains(KeyModifiers::CONTROL) && matches!(k.code, KeyCode::Char('c')) {
                    return Ok(());
                }
                match k.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Left | KeyCode::Char('h') => app.tab = app.tab.prev(),
                    KeyCode::Right | KeyCode::Char('l') => app.tab = app.tab.next(),
                    KeyCode::Down | KeyCode::Char('j') => {
                        if app.tab == Tab::Servers {
                            app.server_selected = app.server_selected.saturating_add(1);
                        } else {
                            app.selected = app.selected.saturating_add(1);
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if app.tab == Tab::Servers {
                            app.server_selected = app.server_selected.saturating_sub(1);
                        } else {
                            app.selected = app.selected.saturating_sub(1);
                        }
                    }
                    KeyCode::Char('?') => app.tab = Tab::Help,
                    KeyCode::Char(' ') => app.paused = !app.paused,
                    KeyCode::Char('r') => app.reset_baseline(),
                    KeyCode::Char('s') => app.sort = app.sort.next(),
                    KeyCode::Char('p') => app.percentile_mode = app.percentile_mode.next(),
                    KeyCode::Char('a') => app.units = UnitsMode::Auto,
                    KeyCode::Char('m') => app.units = UnitsMode::MiB,
                    KeyCode::Char('g') => app.units = UnitsMode::GiB,
                    KeyCode::Char('t') => app.units = UnitsMode::TiB,
                    KeyCode::Char('+') => app.increase_interval(),
                    KeyCode::Char('-') => app.decrease_interval(),
                    _ => {}
                }
            }
        }
    }
}

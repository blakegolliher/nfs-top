use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::ui::panel;

/// `/proc/self/mountstats` is tab-separated. ratatui puts each tab in one
/// buffer cell, but the terminal interprets the emitted `\t` byte as an
/// actual tab and advances the cursor to the next 8-column stop. That
/// desyncs ratatui's cell bookkeeping from the terminal's cursor, which
/// shows up as panel borders disappearing on long mountstats lines and
/// stale content from the previous tab leaking through. Replacing tabs
/// with spaces before they reach `Paragraph` keeps ratatui's "1 char =
/// 1 cell" invariant intact.
fn detab(s: &str) -> String {
    s.replace('\t', "    ")
}

pub fn draw(f: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(30), Constraint::Percentage(25)])
        .split(area);

    let raw_mount = app
        .selected_mount()
        .map(|m| detab(&m.counters.raw_block))
        .unwrap_or_else(|| "no mount".to_string());
    let raw_rpc = app
        .snapshot
        .as_ref()
        .map(|s| detab(&s.rpc.raw))
        .unwrap_or_else(|| "no /proc/net/rpc/nfs".to_string());
    let raw_tcp = app
        .snapshot
        .as_ref()
        .map(|s| detab(&s.raw_tcp_matches.iter().take(20).cloned().collect::<Vec<_>>().join("\n")))
        .unwrap_or_else(|| "no /proc/net/tcp matches".to_string());

    f.render_widget(Paragraph::new(raw_mount).block(panel("mountstats block")), parts[0]);
    f.render_widget(Paragraph::new(raw_rpc).block(panel("/proc/net/rpc/nfs")), parts[1]);
    f.render_widget(Paragraph::new(raw_tcp).block(panel("/proc/net/tcp* matches")), parts[2]);
}

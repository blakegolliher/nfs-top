use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::ui::panel;

pub fn draw(f: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(30), Constraint::Percentage(25)])
        .split(area);

    let raw_mount = app.selected_mount().map(|m| m.counters.raw_block.clone()).unwrap_or_else(|| "no mount".to_string());
    let raw_rpc = app
        .snapshot
        .as_ref()
        .map(|s| s.rpc.raw.clone())
        .unwrap_or_else(|| "no /proc/net/rpc/nfs".to_string());
    let raw_tcp = app
        .snapshot
        .as_ref()
        .map(|s| s.raw_tcp_matches.iter().take(20).cloned().collect::<Vec<_>>().join("\n"))
        .unwrap_or_else(|| "no /proc/net/tcp matches".to_string());

    f.render_widget(Paragraph::new(raw_mount).block(panel("mountstats block")), parts[0]);
    f.render_widget(Paragraph::new(raw_rpc).block(panel("/proc/net/rpc/nfs")), parts[1]);
    f.render_widget(Paragraph::new(raw_tcp).block(panel("/proc/net/tcp* matches")), parts[2]);
}

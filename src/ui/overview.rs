use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;
use crate::ui::{draw_line_card, panel, ACCENT_A, ACCENT_B};
use crate::util::format::{fmt_bytes, fmt_ms, fmt_rate};

pub fn draw(f: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(10)])
        .split(area);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 4); 4])
        .split(parts[0]);

    let mounts = app.visible_mounts();
    let tr: f64 = mounts.iter().map(|m| m.derived.read_bps).sum();
    let tw: f64 = mounts.iter().map(|m| m.derived.write_bps).sum();
    let ops: f64 = mounts.iter().map(|m| m.derived.ops_per_sec).sum();
    let rtt = mounts.iter().filter_map(|m| m.derived.avg_rtt_ms).sum::<f64>() / (mounts.len().max(1) as f64);
    let obs: f64 = mounts.iter().map(|m| m.derived.observed_conns as f64).sum();

    let read_label = format!("{} | {} total", fmt_rate(tr, app.units), fmt_bytes(app.cumulative_read_bytes));
    let write_label = format!("{} | {} total", fmt_rate(tw, app.units), fmt_bytes(app.cumulative_write_bytes));
    draw_line_card(f, top[0], "Read Throughput", &app.read_hist.iter().copied().collect::<Vec<_>>(), &read_label, ACCENT_A);
    draw_line_card(f, top[1], "Write Throughput", &app.write_hist.iter().copied().collect::<Vec<_>>(), &write_label, ACCENT_B);
    draw_line_card(f, top[2], "Ops/s", &app.ops_hist.iter().copied().collect::<Vec<_>>(), &format!("{ops:.1}"), ACCENT_A);
    draw_line_card(f, top[3], "Avg RTT / Obs", &app.rtt_hist.iter().copied().collect::<Vec<_>>(), &format!("{rtt:.2} ms | {:.0} conns", obs), ACCENT_B);

    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(parts[1]);

    let header = Row::new(vec!["Mount", "Server:Export", "Vers", "nconn", "Read", "Write", "Ops/s", "RTT", "EXE", "Obs"])
        .style(Style::default().fg(ACCENT_A).add_modifier(Modifier::BOLD));

    let rows = mounts
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let style = if i == app.selected {
                Style::default().fg(Color::Black).bg(ACCENT_B)
            } else {
                Style::default().fg(Color::White)
            };
            Row::new(vec![
                Cell::from(m.counters.mountpoint.clone()),
                Cell::from(m.counters.device.clone()),
                Cell::from(m.counters.vers.clone().unwrap_or_else(|| "-".to_string())),
                Cell::from(m.counters.nconnect.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string())),
                Cell::from(fmt_rate(m.derived.read_bps, app.units)),
                Cell::from(fmt_rate(m.derived.write_bps, app.units)),
                Cell::from(format!("{:.1}", m.derived.ops_per_sec)),
                Cell::from(fmt_ms(m.derived.avg_rtt_ms)),
                Cell::from(fmt_ms(m.derived.avg_exe_ms)),
                Cell::from(m.derived.observed_conns.to_string()),
            ])
            .style(style)
        })
        .collect::<Vec<_>>();

    let widths = [
        Constraint::Length(16),
        Constraint::Length(24),
        Constraint::Length(5),
        Constraint::Length(5),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(8),
        Constraint::Length(6),
        Constraint::Length(6),
        Constraint::Length(5),
    ];

    let table = Table::new(rows, widths).header(header).block(panel("Mounts"));
    f.render_widget(table, middle[0]);

    let details = if let Some(m) = app.selected_mount() {
        let ips = m
            .resolved_ips
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let obs = m
            .derived
            .observed_by_ip
            .iter()
            .map(|(ip, c)| format!("{ip} -> {c}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "device: {}\nmount: {}\nvers: {}\nproto: {}\nnconnect: {}\naddr: {}\nclientaddr: {}\nresolved: {}\n\nobserved:\n{}",
            m.counters.device,
            m.counters.mountpoint,
            m.counters.vers.clone().unwrap_or_else(|| "-".to_string()),
            m.counters.proto.clone().unwrap_or_else(|| "-".to_string()),
            m.counters.nconnect.map(|x| x.to_string()).unwrap_or_else(|| "-".to_string()),
            m.counters.addr.map(|x| x.to_string()).unwrap_or_else(|| "-".to_string()),
            m.counters.clientaddr.map(|x| x.to_string()).unwrap_or_else(|| "-".to_string()),
            if ips.is_empty() { "-".to_string() } else { ips },
            if obs.is_empty() { "-".to_string() } else { obs },
        )
    } else {
        "no mounts".to_string()
    };
    f.render_widget(Paragraph::new(details).block(panel("Selected")), middle[1]);
}

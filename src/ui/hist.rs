//! eBPF latency histogram tab.
//!
//! Renders per-op p50..p99.999 + a log-scale bucket sparkline, populated
//! from `Snapshot.bpf`. Visible whenever the binary is built with the
//! `ebpf` feature; on builds or hosts without working probes the tab
//! shows a single explanatory line — no panic, no empty chrome.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;
use crate::model::types::{BpfLatency, BpfOpLatency, LatencyDist};
use crate::ui::{panel, ACCENT_A};

pub fn draw(f: &mut Frame<'_>, area: Rect, app: &App) {
    let block = panel("Latency histogram (eBPF)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let bpf = match app.snapshot.as_ref().and_then(|s| s.bpf.as_ref()) {
        Some(b) if !b.per_op.is_empty() => b,
        _ => {
            let msg = empty_message(app);
            f.render_widget(Paragraph::new(msg).style(Style::default().fg(Color::Gray)), inner);
            return;
        }
    };

    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length((bpf.per_op.len() as u16) + 3), Constraint::Min(3)])
        .split(inner);

    f.render_widget(percentile_table(bpf), parts[0]);

    if let Some(top) = bpf.per_op.first() {
        f.render_widget(distribution_line(top), parts[1]);
    }
}

fn empty_message(_app: &App) -> Line<'static> {
    let cfg_hint = if cfg!(feature = "ebpf") {
        "eBPF backend not active — needs CAP_BPF and a kernel with NFS BTF (RHEL 9+). Run with sudo or `setcap cap_bpf,cap_sys_resource=ep`."
    } else {
        "Built without --features=ebpf. Rebuild with `cargo build --features=ebpf` to enable per-op latency histograms."
    };
    Line::from(vec![Span::styled(cfg_hint, Style::default().fg(Color::Gray))])
}

fn percentile_table(bpf: &BpfLatency) -> Table<'static> {
    let header_cells = ["op", "samples", "p50", "p90", "p99", "p99.9", "p99.99", "p99.999", "max"]
        .into_iter()
        .map(|h| Cell::from(h).style(Style::default().fg(ACCENT_A).add_modifier(Modifier::BOLD)));
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = bpf.per_op.iter().map(row_for_op).collect();

    let totals_cell = format!("{} samples this tick", fmt_count(bpf.total_samples));
    let footer = Row::new(vec![Cell::from(totals_cell)])
        .style(Style::default().fg(Color::DarkGray));

    let widths = [
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Length(9),
    ];

    Table::new(rows.into_iter().chain(std::iter::once(footer)), widths)
        .header(header)
        .column_spacing(1)
}

fn row_for_op(op: &BpfOpLatency) -> Row<'static> {
    let d = &op.dist;
    Row::new(vec![
        Cell::from(op.op.clone()),
        Cell::from(fmt_count(d.samples)),
        Cell::from(fmt_ns(d.p50_ns)),
        Cell::from(fmt_ns(d.p90_ns)),
        Cell::from(fmt_ns(d.p99_ns)),
        Cell::from(fmt_ns(d.p999_ns)),
        Cell::from(fmt_ns(d.p9999_ns)),
        Cell::from(fmt_ns(d.p99999_ns)),
        Cell::from(fmt_ns(d.max_ns)),
    ])
}

/// One-line "shape" of the highest-throughput op's distribution. Each
/// character is a power-of-two bucket whose height encodes log(count).
fn distribution_line(top: &BpfOpLatency) -> Paragraph<'static> {
    let (chars, low_edge_ns, high_edge_ns) = bucket_sparkline(&top.dist);
    let header = format!(
        "{} distribution  ({} → {})",
        top.op,
        fmt_ns(low_edge_ns),
        fmt_ns(high_edge_ns),
    );
    let lines = vec![
        Line::from(Span::styled(header, Style::default().fg(Color::Gray))),
        Line::from(Span::styled(chars, Style::default().fg(ACCENT_A))),
    ];
    Paragraph::new(lines)
}

/// We don't have raw bucket counts on `LatencyDist` (only percentiles).
/// Render a synthetic sparkline using the percentile shape: each character
/// is the lower-bound bucket value at p50/p90/p99/p99.9/p99.99/p99.999/max,
/// height encodes its rank.
fn bucket_sparkline(dist: &LatencyDist) -> (String, u64, u64) {
    const SPARK: [char; 8] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'];
    let stops = [
        dist.p50_ns,
        dist.p90_ns,
        dist.p99_ns,
        dist.p999_ns,
        dist.p9999_ns,
        dist.p99999_ns,
        dist.max_ns,
    ];
    let s: String = stops
        .iter()
        .enumerate()
        .map(|(i, &ns)| {
            if ns == 0 { return ' '; }
            let h = ((i + 1) * SPARK.len() / stops.len()).min(SPARK.len() - 1);
            SPARK[h]
        })
        .collect();
    let lo = stops.iter().copied().filter(|n| *n > 0).min().unwrap_or(0);
    let hi = dist.max_ns;
    (s, lo, hi)
}

fn fmt_count(n: u64) -> String {
    match n {
        0..=9_999 => format!("{n}"),
        10_000..=999_999 => format!("{:.1}K", (n as f64) / 1e3),
        1_000_000..=999_999_999 => format!("{:.1}M", (n as f64) / 1e6),
        _ => format!("{:.1}G", (n as f64) / 1e9),
    }
}

fn fmt_ns(ns: u64) -> String {
    if ns == 0 {
        return "-".to_string();
    }
    if ns >= 1_000_000_000 {
        format!("{:.1}s", (ns as f64) / 1e9)
    } else if ns >= 1_000_000 {
        format!("{:.1}ms", (ns as f64) / 1e6)
    } else if ns >= 1_000 {
        format!("{:.1}us", (ns as f64) / 1e3)
    } else {
        format!("{ns}ns")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_ns_human_units() {
        assert_eq!(fmt_ns(0), "-");
        assert_eq!(fmt_ns(500), "500ns");
        assert_eq!(fmt_ns(1_500), "1.5us");
        assert_eq!(fmt_ns(1_500_000), "1.5ms");
        assert_eq!(fmt_ns(2_500_000_000), "2.5s");
    }

    #[test]
    fn fmt_count_thousands_and_millions() {
        assert_eq!(fmt_count(42), "42");
        assert_eq!(fmt_count(12_345), "12.3K");
        assert_eq!(fmt_count(1_500_000), "1.5M");
    }
}

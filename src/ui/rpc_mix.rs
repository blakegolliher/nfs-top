use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::ui::panel;
use crate::util::format::{fmt_ms, fmt_rate};

pub fn draw(f: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let body = if let Some(m) = app.selected_mount() {
        let txt = m
            .derived
            .per_op
            .iter()
            .take(16)
            .map(|o| {
                format!(
                    "{:>10}  {:>6.1} ops/s  {:>6.1}%  {:>12}  rtt={}ms exe={}ms",
                    o.op,
                    o.ops_per_sec,
                    o.share_pct,
                    fmt_rate(o.bytes_per_sec, app.units),
                    fmt_ms(o.avg_rtt_ms),
                    fmt_ms(o.avg_exe_ms),
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        if txt.is_empty() {
            "no per-op rate data yet".to_string()
        } else {
            format!(
                "Mount: {}\nRPC mix shows interval op rates and share of calls for this mount.\n\n{}",
                m.counters.mountpoint, txt
            )
        }
    } else {
        "no mount selected".to_string()
    };

    f.render_widget(Paragraph::new(body).block(panel("RPC Mix (selected mount)")), area);
}

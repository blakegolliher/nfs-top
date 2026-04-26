use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::ui::panel;

pub fn draw(f: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let txt = if let Some(m) = app.selected_mount() {
        let configured = m.counters.nconnect.map(|x| x.to_string()).unwrap_or_else(|| "-".to_string());
        let total = m.derived.observed_conns;
        let nonzero_total = total.max(1);
        let unique_paths = m.derived.observed_by_ip.len();
        let balance = if total > 0 && unique_paths > 0 {
            let sum_sq = m
                .derived
                .observed_by_ip
                .iter()
                .map(|(_, c)| {
                    let p = (*c as f64) / (total as f64);
                    p * p
                })
                .sum::<f64>();
            let effective = if sum_sq > 0.0 { 1.0 / sum_sq } else { 0.0 };
            (effective / unique_paths as f64) * 100.0
        } else {
            0.0
        };

        let mut lines = vec![
            "What this tells you: whether the mount is opening expected TCP sessions and how evenly traffic paths are used.".to_string(),
            format!("configured nconnect: {configured}"),
            format!("observed matching TCP sessions: {total}"),
            format!("unique remote IPs: {unique_paths}"),
            format!("distribution balance: {balance:.1}% (100% = evenly spread)"),
            String::new(),
            "remote_ip -> count (%share)".to_string(),
        ];
        for (ip, c) in &m.derived.observed_by_ip {
            let pct = (*c as f64) * 100.0 / (nonzero_total as f64);
            lines.push(format!("{ip} -> {c} ({pct:.1}%)"));
        }
        if m.derived.observed_by_ip.is_empty() {
            lines.push("no active conns observed".to_string());
        }
        lines.join("\n")
    } else {
        "no mount selected".to_string()
    };
    f.render_widget(Paragraph::new(txt).block(panel("Connections")), area);
}

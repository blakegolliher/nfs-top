use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::ui::panel;

pub fn draw(f: &mut Frame<'_>, area: ratatui::layout::Rect, _app: &App) {
    let text = "q quit | h/l or Left/Right tabs | j/k or Up/Down mount select\nspace pause | r reset baseline | s cycle sort | p cycle trend mode | ? help\na auto units | m MiB | g GiB | t TiB\n+/- interval adjust\n\nTrends percentile lines are estimated from rolling sampled averages.\nObserved connections are inferred from /proc/net/tcp* established sockets to ports 2049/20049 and mapped via addr= or DNS.";
    f.render_widget(Paragraph::new(text).block(panel("Help")), area);
}

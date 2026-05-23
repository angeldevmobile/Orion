use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{
        Block, Borders, Gauge, List, ListItem, Paragraph,
        Sparkline, Tabs, Wrap,
    },
    Frame, Terminal,
};
use ratatui::widgets::{Cell, Row, Table};

use super::{TuiWidget, TuiStyle, with_state};

//    Punto de entrada                                                         

pub fn launch(
    title:        String,
    widgets:      Vec<TuiWidget>,
    key_handlers: Vec<(String, String)>,
) -> Result<(), String> {
    enable_raw_mode().map_err(|e| format!("tui: {e}"))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| format!("tui: {e}"))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| format!("tui: {e}"))?;

    let result = run_loop(&mut terminal, title, widgets, key_handlers);

    // Siempre restaurar la terminal aunque haya error
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    result
}

//    Event loop                                                               

fn run_loop(
    terminal:     &mut Terminal<CrosstermBackend<io::Stdout>>,
    title:        String,
    widgets:      Vec<TuiWidget>,
    key_handlers: Vec<(String, String)>,
) -> Result<(), String> {
    loop {
        let title_ref = title.as_str();
        terminal.draw(|f| {
            let area = f.area();
            // Marco exterior con el título de la app
            let outer = Block::default()
                .title(format!(" {} ", title_ref))
                .borders(Borders::ALL)
                .style(Style::default());
            let inner = outer.inner(area);
            f.render_widget(outer, area);
            render_list(f, inner, &widgets);
        }).map_err(|e| format!("tui render: {e}"))?;

        if event::poll(Duration::from_millis(100)).map_err(|e| format!("tui poll: {e}"))? {
            if let Event::Key(key) = event::read().map_err(|e| format!("tui read: {e}"))? {
                // q o Ctrl+C para salir
                if key.code == KeyCode::Char('q')
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                {
                    break;
                }
                let name = key_name(key.code);
                for (k, event_name) in &key_handlers {
                    if k == &name {
                        with_state(|s| s.last_key = event_name.clone());
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

fn key_name(code: KeyCode) -> String {
    match code {
        KeyCode::Char(c)   => c.to_string(),
        KeyCode::Up        => "up".into(),
        KeyCode::Down      => "down".into(),
        KeyCode::Left      => "left".into(),
        KeyCode::Right     => "right".into(),
        KeyCode::Enter     => "enter".into(),
        KeyCode::Esc       => "esc".into(),
        KeyCode::Tab       => "tab".into(),
        KeyCode::BackTab   => "shift+tab".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Delete    => "delete".into(),
        KeyCode::Home      => "home".into(),
        KeyCode::End       => "end".into(),
        KeyCode::PageUp    => "pageup".into(),
        KeyCode::PageDown  => "pagedown".into(),
        KeyCode::F(n)      => format!("f{n}"),
        _                  => String::new(),
    }
}

//    Rendering                                                                 

fn to_style(ts: &TuiStyle) -> Style {
    let mut s = Style::default();
    if let Some(fg) = ts.fg { s = s.fg(fg); }
    if let Some(bg) = ts.bg { s = s.bg(bg); }
    if ts.bold { s = s.add_modifier(Modifier::BOLD); }
    s
}

/// Altura estimada de un widget para el layout de constraints
fn widget_height(w: &TuiWidget) -> u16 {
    match w {
        TuiWidget::Text(_, _)    => 1,
        TuiWidget::Caption(_, _) => 1,
        TuiWidget::Divider       => 1,
        TuiWidget::Spacer        => 1,
        TuiWidget::Heading(_, _) => 3,
        TuiWidget::Gauge { .. }  => 3,
        TuiWidget::TuiTabs { .. }  => 3,
        TuiWidget::Spark { data } => if data.is_empty() { 3 } else { 5 },
        TuiWidget::Items { items, .. } => (items.len() as u16 + 2).min(20),
        TuiWidget::Grid { rows, .. }   => (rows.len() as u16 + 3).min(20),
        TuiWidget::Chart { data, .. }  => (data.len() as u16 * 3 + 2).min(30),
        TuiWidget::Row(children) => children.iter().map(widget_height).max().unwrap_or(3) + 1,
        TuiWidget::Col(children) => children.iter().map(widget_height).sum::<u16>() + 1,
    }
}

/// Renderiza una lista de widgets en vertical dentro de `area`
fn render_list(f: &mut Frame, area: Rect, widgets: &[TuiWidget]) {
    if widgets.is_empty() { return; }

    let constraints: Vec<Constraint> = widgets.iter()
        .map(|w| Constraint::Length(widget_height(w)))
        .collect();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (i, w) in widgets.iter().enumerate() {
        if i < chunks.len() {
            render_one(f, chunks[i], w);
        }
    }
}

fn render_one(f: &mut Frame, area: Rect, widget: &TuiWidget) {
    match widget {
        TuiWidget::Text(content, style) => {
            let p = Paragraph::new(content.as_str())
                .style(to_style(style))
                .wrap(Wrap { trim: false });
            f.render_widget(p, area);
        }

        TuiWidget::Heading(title, style) => {
            let block = Block::default()
                .title(title.as_str())
                .borders(Borders::ALL)
                .style(to_style(style));
            f.render_widget(block, area);
        }

        TuiWidget::Caption(content, style) => {
            let s = to_style(style).add_modifier(Modifier::DIM);
            f.render_widget(Paragraph::new(content.as_str()).style(s), area);
        }

        TuiWidget::Gauge { label, percent, style } => {
            // Si el dev no especifica color, usa el color de primer plano del terminal
            let mut gauge_style = Style::default();
            if let Some(fg) = style.fg { gauge_style = gauge_style.fg(fg); }
            if let Some(bg) = style.bg { gauge_style = gauge_style.bg(bg); }
            let gauge = Gauge::default()
                .block(Block::default().title(label.as_str()).borders(Borders::ALL))
                .gauge_style(gauge_style)
                .ratio(*percent as f64 / 100.0);
            f.render_widget(gauge, area);
        }

        TuiWidget::Items { items, style } => {
            let list_items: Vec<ListItem> = items.iter()
                .map(|i| ListItem::new(i.as_str()))
                .collect();
            let list = List::new(list_items)
                .block(Block::default().borders(Borders::ALL))
                .style(to_style(style));
            f.render_widget(list, area);
        }

        TuiWidget::Grid { headers, rows, style } => {
            let n = headers.len().max(1);
            let widths: Vec<Constraint> = (0..n)
                .map(|_| Constraint::Ratio(1, n as u32))
                .collect();

            let header_row = Row::new(
                headers.iter().map(|h| Cell::from(h.as_str())
                    .style(Style::default().add_modifier(Modifier::BOLD)))
            ).height(1);

            let body: Vec<Row> = rows.iter()
                .map(|r| Row::new(r.iter().map(|c| Cell::from(c.as_str())).collect::<Vec<_>>()))
                .collect();

            let table = Table::new(body, widths)
                .header(header_row)
                .block(Block::default().borders(Borders::ALL))
                .style(to_style(style));

            f.render_widget(table, area);
        }

        TuiWidget::Chart { label, data } => {
            // Renderiza como lista de gauges (uno por item) — sin depender de BarChart API
            let max = data.iter().map(|(_, v)| *v).max().unwrap_or(1).max(1);
            let block = Block::default()
                .title(label.as_str())
                .borders(Borders::ALL);
            let inner = block.inner(area);
            f.render_widget(block, area);

            if data.is_empty() { return; }
            let row_h = 3u16;
            let constraints: Vec<Constraint> = data.iter()
                .map(|_| Constraint::Length(row_h))
                .collect();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(inner);

            for (i, (lbl, val)) in data.iter().enumerate() {
                if i >= chunks.len() { break; }
                let ratio = (*val as f64) / (max as f64);
                let g = Gauge::default()
                    .block(Block::default().title(lbl.as_str()))
                    .gauge_style(Style::default())
                    .ratio(ratio)
                    .label(format!("{val}"));
                f.render_widget(g, chunks[i]);
            }
        }

        TuiWidget::Spark { data } => {
            let spark = Sparkline::default()
                .block(Block::default().borders(Borders::TOP))
                .data(data)
                .style(Style::default());
            f.render_widget(spark, area);
        }

        TuiWidget::TuiTabs { labels, selected } => {
            let titles: Vec<Line> = labels.iter().map(|t| Line::from(t.as_str())).collect();
            let tabs = Tabs::new(titles)
                .block(Block::default().borders(Borders::ALL))
                .select(*selected)
                .highlight_style(Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED));
            f.render_widget(tabs, area);
        }

        TuiWidget::Divider => {
            let line = " ".repeat(area.width as usize);
            f.render_widget(Paragraph::new(line.as_str()), area);
        }

        TuiWidget::Spacer => {}

        TuiWidget::Row(children) => {
            if children.is_empty() { return; }
            let n = children.len() as u32;
            let constraints: Vec<Constraint> = children.iter()
                .map(|_| Constraint::Ratio(1, n))
                .collect();
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(constraints)
                .split(area);
            for (i, child) in children.iter().enumerate() {
                if i < chunks.len() {
                    render_one(f, chunks[i], child);
                }
            }
        }

        TuiWidget::Col(children) => {
            render_list(f, area, children);
        }
    }
}

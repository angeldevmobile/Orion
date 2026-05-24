/// Estilo opcional por componente: color de fondo y/o texto
#[derive(Clone, Default)]
pub struct Style {
    pub bg: Option<[u8; 3]>,
    pub fg: Option<[u8; 3]>,
}

impl Style {
    pub fn bg_color(&self) -> Option<egui::Color32> {
        self.bg.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b))
    }
    pub fn fg_color(&self) -> Option<egui::Color32> {
        self.fg.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b))
    }
}

// ── Chart types ──────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub enum ChartKind { Bar, Line, Area, Scatter, Pie, Hist }

#[derive(Clone)]
pub struct ChartConfig {
    pub kind:    ChartKind,
    pub title:   Option<String>,
    pub height:  f32,
    /// Etiquetas del eje X (bar/line/area/pie)
    pub labels:  Vec<String>,
    /// Series nombradas: (nombre, valores_y)
    pub series:  Vec<(String, Vec<f64>)>,
    /// Valores X para scatter
    pub xs:      Vec<f64>,
    /// Número de bins para histograma
    pub bins:    usize,
    /// Paleta de colores RGB personalizada
    pub palette: Vec<[u8; 3]>,
}

/// Componentes nativos de Orion — nombres propios del lenguaje
#[derive(Clone)]
pub enum Component {
    //    Tipografía
    Heading(String, Style),
    Text(String, Style),
    Caption(String, Style),

    //    Inputs
    Field { id: String, placeholder: String, style: Style },
    Toggle { id: String, label: String },

    //    Acciones
    Press(String, Style),   // botón primario
    Ghost(String, Style),   // botón outline
    Tap(String),            // botón de texto / link

    //    Display
    Badge(String, Style),
    Banner { title: String, subtitle: Option<String>, style: Style },
    Avatar { text: String, size: f32, style: Style },
    Divider,
    Spacer(f32),

    //    Inputs avanzados
    Pick  { id: String, options: Vec<String>, style: Style },
    Slide { id: String, min: f32, max: f32, step: f32 },

    //    Layout (containers anidados, se cierran con gui.end())
    Card(Vec<Component>),
    Row(Vec<Component>),
    Col(Vec<Component>),
    Zone(Vec<Component>, Style),

    //    UI-5 — Animaciones
    FadeGroup { id: String, show: bool, children: Vec<Component> },
    SlideIn { id: String, children: Vec<Component> },

    //    Datos visuales
    Table { headers: Vec<String>, rows: Vec<Vec<String>>, height: f32 },
    Chart(Box<ChartConfig>),
}

//     Render

use std::collections::HashMap;
use eframe::egui;
use super::theme;

const ACCENT: egui::Color32 = egui::Color32::from_rgb(108, 99, 255);

const DEFAULT_PALETTE: &[[u8; 3]] = &[
    [108, 99,  255], [34,  197,  94], [59,  130, 246], [234, 179,   8],
    [249, 115,  22], [168,  85, 247], [236,  72, 153], [239,  68,  68],
    [ 20, 184, 166], [251, 191,  36], [ 99, 102, 241], [ 16, 185, 129],
];

fn palette_color(palette: &[[u8; 3]], i: usize) -> egui::Color32 {
    let rgb = palette.get(i)
        .copied()
        .unwrap_or_else(|| DEFAULT_PALETTE[i % DEFAULT_PALETTE.len()]);
    egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// Renderiza un componente. Si el usuario interactúa, escribe el evento en `event`.
pub fn render(
    ui: &mut egui::Ui,
    comp: &Component,
    fields: &mut HashMap<String, String>,
    event: &mut Option<String>,
) {
    match comp {
        Component::Heading(t, style) => {
            let rt = egui::RichText::new(t).size(26.0).strong();
            let rt = match style.fg_color() {
                Some(c) => rt.color(c),
                None    => rt,
            };
            ui.label(rt);
        }
        Component::Text(t, style) => {
            match style.fg_color() {
                Some(c) => { ui.colored_label(c, t); }
                None    => { ui.label(t); }
            }
        }
        Component::Caption(t, style) => {
            let rt = egui::RichText::new(t).small();
            let rt = match style.fg_color() {
                Some(c) => rt.color(c),
                None    => rt,
            };
            ui.label(rt);
        }
        Component::Field { id, placeholder, style } => {
            let val = fields.entry(id.clone()).or_default();
            ui.scope(|ui| {
                if let Some(c) = style.bg_color() {
                    ui.visuals_mut().extreme_bg_color = c;
                }
                if let Some(c) = style.fg_color() {
                    ui.visuals_mut().override_text_color = Some(c);
                }
                ui.add(
                    egui::TextEdit::singleline(val)
                        .hint_text(placeholder.as_str())
                        .desired_width(f32::INFINITY),
                );
            });
        }
        Component::Toggle { id, label } => {
            let val = fields.entry(id.clone()).or_insert_with(|| "false".into());
            let mut checked = val == "true";
            if ui.checkbox(&mut checked, label.as_str()).changed() {
                *val = if checked { "true".into() } else { "false".into() };
                if event.is_none() {
                    *event = Some(format!("{}:{}", id, val));
                }
            } else {
                *val = if checked { "true".into() } else { "false".into() };
            }
        }
        Component::Press(label, style) => {
            let fill = style.bg_color().unwrap_or(ACCENT);
            let rt = match style.fg_color() {
                Some(c) => egui::RichText::new(label).color(c),
                None    => egui::RichText::new(label),
            };
            if ui.add_sized([120.0, 36.0], egui::Button::new(rt).fill(fill)).clicked()
                && event.is_none()
            {
                *event = Some(label.clone());
            }
        }
        Component::Ghost(label, style) => {
            let color = style.fg_color().unwrap_or(ACCENT);
            if ui.add(
                egui::Button::new(label)
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(egui::Stroke::new(1.5, color)),
            ).clicked() && event.is_none() {
                *event = Some(label.clone());
            }
        }
        Component::Tap(label) => {
            if ui.link(label).clicked() && event.is_none() {
                *event = Some(label.clone());
            }
        }
        Component::Badge(text, style) => {
            let fill = style.bg_color().unwrap_or(ACCENT);
            egui::Frame::none()
                .fill(fill)
                .rounding(egui::Rounding::same(12.0))
                .inner_margin(egui::Margin::symmetric(10.0, 4.0))
                .show(ui, |ui| {
                    let text_color = style.fg_color().unwrap_or(egui::Color32::WHITE);
                    ui.colored_label(text_color, text);
                });
        }
        Component::Banner { title, subtitle, style } => {
            let fill = style.bg_color().unwrap_or(theme::SURFACE);
            egui::Frame::none()
                .fill(fill)
                .rounding(egui::Rounding::same(10.0))
                .inner_margin(egui::Margin::same(20.0))
                .show(ui, |ui| {
                    let color = style.fg_color().unwrap_or(egui::Color32::WHITE);
                    ui.label(egui::RichText::new(title).size(22.0).strong().color(color));
                    if let Some(sub) = subtitle {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(sub).size(13.0)
                            .color(egui::Color32::from_rgb(160, 160, 180)));
                    }
                });
        }
        Component::Avatar { text, size, style } => {
            let fill       = style.bg_color().unwrap_or(ACCENT);
            let text_color = style.fg_color().unwrap_or(egui::Color32::WHITE);
            let (resp, painter) = ui.allocate_painter(
                egui::Vec2::splat(*size),
                egui::Sense::hover(),
            );
            painter.circle_filled(resp.rect.center(), size / 2.0, fill);
            painter.text(
                resp.rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(size * 0.38),
                text_color,
            );
        }
        Component::Divider => {
            ui.separator();
        }
        Component::Spacer(h) => {
            ui.add_space(*h);
        }
        Component::Pick { id, options, style } => {
            let selected = fields.entry(id.clone())
                .or_insert_with(|| options.first().cloned().unwrap_or_default());
            let prev = selected.clone();
            ui.scope(|ui| {
                if let Some(c) = style.bg_color() {
                    ui.visuals_mut().extreme_bg_color = c;
                }
                egui::ComboBox::from_id_salt(id.as_str())
                    .selected_text(selected.as_str())
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for opt in options {
                            ui.selectable_value(selected, opt.clone(), opt.as_str());
                        }
                    });
            });
            if *selected != prev && event.is_none() {
                *event = Some(format!("{}:{}", id, selected));
            }
        }
        Component::Slide { id, min, max, step } => {
            let val_str = fields.entry(id.clone())
                .or_insert_with(|| min.to_string());
            let mut val: f32 = val_str.parse().unwrap_or(*min);
            if ui.add(egui::Slider::new(&mut val, *min..=*max).step_by(*step as f64))
                .changed() && event.is_none()
            {
                *event = Some(format!("{}:{}", id, val));
            }
            *val_str = val.to_string();
        }
        Component::Card(children) => {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(26, 26, 40))
                .rounding(egui::Rounding::same(10.0))
                .inner_margin(egui::Margin::same(16.0))
                .show(ui, |ui| {
                    for child in children {
                        render(ui, child, fields, event);
                        ui.add_space(6.0);
                    }
                });
        }
        Component::Row(children) => {
            ui.horizontal(|ui| {
                for child in children {
                    render(ui, child, fields, event);
                }
            });
        }
        Component::Col(children) => {
            ui.vertical(|ui| {
                for child in children {
                    render(ui, child, fields, event);
                    ui.add_space(6.0);
                }
            });
        }
        Component::Zone(children, style) => {
            let fill = style.bg_color().unwrap_or(theme::SURFACE);
            egui::Frame::none()
                .fill(fill)
                .rounding(egui::Rounding::same(6.0))
                .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                .show(ui, |ui| {
                    ui.set_max_width(f32::INFINITY);
                    for child in children {
                        render(ui, child, fields, event);
                        ui.add_space(4.0);
                    }
                });
        }

        // UI-5 — Animaciones

        Component::FadeGroup { id, show, children } => {
            let alpha = ui.ctx().animate_bool(egui::Id::new(id), *show);
            if alpha > 0.001 {
                ui.set_opacity(alpha);
                for child in children {
                    render(ui, child, fields, event);
                    ui.add_space(4.0);
                }
                ui.set_opacity(1.0);
                if alpha < 0.999 {
                    ui.ctx().request_repaint();
                }
            }
        }

        Component::SlideIn { id, children } => {
            let t = ui.ctx().animate_value_with_time(
                egui::Id::new(id),
                1.0,
                0.35,
            );
            let offset = (1.0 - t) * 24.0;
            ui.set_opacity(t);
            ui.add_space(offset);
            for child in children {
                render(ui, child, fields, event);
                ui.add_space(4.0);
            }
            ui.set_opacity(1.0);
            if t < 0.999 {
                ui.ctx().request_repaint();
            }
        }

        // ── Datos visuales ────────────────────────────────────────────────────

        Component::Table { headers, rows, height } => {
            render_table(ui, headers, rows, *height);
        }

        Component::Chart(cfg) => {
            render_chart(ui, cfg);
        }
    }
}

// ── Table ─────────────────────────────────────────────────────────────────────

fn render_table(ui: &mut egui::Ui, headers: &[String], rows: &[Vec<String>], height: f32) {
    let header_color  = egui::Color32::from_rgb(180, 170, 255);
    let stripe_a      = egui::Color32::from_rgb(20, 20, 34);
    let stripe_b      = egui::Color32::from_rgb(26, 26, 42);
    let border_color  = egui::Color32::from_rgb(50, 50, 70);

    egui::Frame::none()
        .fill(stripe_a)
        .rounding(egui::Rounding::same(8.0))
        .stroke(egui::Stroke::new(1.0, border_color))
        .show(ui, |ui| {
            egui::ScrollArea::both()
                .max_height(height)
                .id_salt("orion_table_scroll")
                .show(ui, |ui| {
                    egui::Grid::new("orion_table_grid")
                        .striped(false)
                        .num_columns(headers.len())
                        .min_col_width(90.0)
                        .spacing([0.0, 0.0])
                        .show(ui, |ui| {
                            // Cabecera
                            for h in headers {
                                egui::Frame::none()
                                    .fill(egui::Color32::from_rgb(30, 28, 52))
                                    .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                                    .show(ui, |ui| {
                                        ui.set_min_width(90.0);
                                        ui.label(
                                            egui::RichText::new(h)
                                                .strong()
                                                .size(13.0)
                                                .color(header_color),
                                        );
                                    });
                            }
                            ui.end_row();

                            // Filas de datos
                            for (ri, row) in rows.iter().enumerate() {
                                let fill = if ri % 2 == 0 { stripe_a } else { stripe_b };
                                for cell in row {
                                    egui::Frame::none()
                                        .fill(fill)
                                        .inner_margin(egui::Margin::symmetric(10.0, 5.0))
                                        .show(ui, |ui| {
                                            ui.set_min_width(90.0);
                                            ui.label(
                                                egui::RichText::new(cell).size(13.0)
                                                    .color(egui::Color32::from_rgb(210, 210, 225)),
                                            );
                                        });
                                }
                                ui.end_row();
                            }
                        });
                });
        });
}

// ── Chart ─────────────────────────────────────────────────────────────────────

fn render_chart(ui: &mut egui::Ui, cfg: &ChartConfig) {
    use egui_plot::{Bar, BarChart, Line, Plot, PlotPoints, Points};

    if let Some(title) = &cfg.title {
        ui.label(
            egui::RichText::new(title)
                .size(14.0)
                .strong()
                .color(egui::Color32::WHITE),
        );
        ui.add_space(4.0);
    }

    // ── Pie: custom painter ──────────────────────────────────────────────────
    if cfg.kind == ChartKind::Pie {
        let avail_w = ui.available_width();
        let size = egui::Vec2::new(avail_w, cfg.height);
        let (resp, painter) = ui.allocate_painter(size, egui::Sense::hover());
        let center = resp.rect.center();
        let radius = (cfg.height / 2.2).min(avail_w / 2.5) - 10.0;

        let values = cfg.series.first().map(|(_, v)| v.as_slice()).unwrap_or(&[]);
        let total: f64 = values.iter().sum();

        if total > 0.0 {
            let mut angle = -std::f32::consts::FRAC_PI_2;
            for (i, &val) in values.iter().enumerate() {
                let sweep = (val / total * std::f64::consts::TAU) as f32;
                let steps = ((sweep * radius / 2.0) as usize + 4).max(6);
                let mut pts = vec![center];
                for j in 0..=steps {
                    let a = angle + sweep * j as f32 / steps as f32;
                    pts.push(egui::pos2(
                        center.x + radius * a.cos(),
                        center.y + radius * a.sin(),
                    ));
                }
                let color = palette_color(&cfg.palette, i);
                painter.add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));

                // Etiqueta dentro del slice si es suficientemente grande
                if sweep > 0.18 {
                    let mid = angle + sweep * 0.5;
                    let lp = egui::pos2(
                        center.x + radius * 0.65 * mid.cos(),
                        center.y + radius * 0.65 * mid.sin(),
                    );
                    let label = cfg.labels.get(i).map(|s| s.as_str()).unwrap_or("");
                    let pct   = format!("{:.1}%", val / total * 100.0);
                    painter.text(lp, egui::Align2::CENTER_CENTER,
                        label, egui::FontId::proportional(10.5), egui::Color32::WHITE);
                    painter.text(
                        egui::pos2(lp.x, lp.y + 13.0),
                        egui::Align2::CENTER_CENTER,
                        &pct, egui::FontId::proportional(10.0),
                        egui::Color32::from_gray(220),
                    );
                }
                angle += sweep;
            }

            // Leyenda lateral
            let legend_x = center.x + radius + 16.0;
            let mut legend_y = center.y - (values.len() as f32 * 18.0) / 2.0;
            for (i, label) in cfg.labels.iter().enumerate() {
                let color = palette_color(&cfg.palette, i);
                let r = egui::Rect::from_min_size(
                    egui::pos2(legend_x, legend_y),
                    egui::Vec2::new(12.0, 12.0),
                );
                painter.rect_filled(r, egui::Rounding::same(2.0), color);
                painter.text(
                    egui::pos2(legend_x + 16.0, legend_y + 6.0),
                    egui::Align2::LEFT_CENTER,
                    label, egui::FontId::proportional(11.0),
                    egui::Color32::from_gray(210),
                );
                legend_y += 18.0;
            }
        }
        return;
    }

    // ── egui_plot: Bar / Line / Area / Scatter / Hist ────────────────────────
    let labels     = cfg.labels.clone();
    let n_labels   = labels.len();
    let chart_kind = cfg.kind.clone();

    let mut plot = Plot::new(
        format!("orion_chart_{}", cfg.title.as_deref().unwrap_or("0")),
    )
    .height(cfg.height)
    .set_margin_fraction(egui::Vec2::new(0.05, 0.08));

    // Formateador del eje X con etiquetas de categoría
    if !labels.is_empty() && chart_kind != ChartKind::Scatter && chart_kind != ChartKind::Hist {
        plot = plot.x_axis_formatter(move |mark, _range| {
            let i = mark.value.round() as isize;
            if i >= 0 && (i as usize) < n_labels {
                labels[i as usize].clone()
            } else {
                String::new()
            }
        });
    }

    match cfg.kind {
        ChartKind::Bar => {
            plot.show(ui, |pu| {
                let n = cfg.series.len().max(1);
                let bar_w = 0.8 / n as f64;
                for (si, (name, vals)) in cfg.series.iter().enumerate() {
                    let color  = palette_color(&cfg.palette, si);
                    let offset = si as f64 * bar_w - (n as f64 - 1.0) * bar_w / 2.0;
                    let bars: Vec<Bar> = vals.iter().enumerate().map(|(i, &v)| {
                        Bar::new(i as f64 + offset, v)
                            .width(bar_w * 0.9)
                            .fill(color)
                    }).collect();
                    pu.bar_chart(BarChart::new(bars).name(name));
                }
            });
        }
        ChartKind::Line => {
            plot.show(ui, |pu| {
                for (si, (name, vals)) in cfg.series.iter().enumerate() {
                    let color = palette_color(&cfg.palette, si);
                    let pts: PlotPoints = vals.iter().enumerate()
                        .map(|(i, &v)| [i as f64, v])
                        .collect();
                    pu.line(Line::new(pts).name(name).color(color).width(2.0));
                }
            });
        }
        ChartKind::Area => {
            plot.show(ui, |pu| {
                for (si, (name, vals)) in cfg.series.iter().enumerate() {
                    let color = palette_color(&cfg.palette, si);
                    let pts: PlotPoints = vals.iter().enumerate()
                        .map(|(i, &v)| [i as f64, v])
                        .collect();
                    pu.line(Line::new(pts).name(name).color(color).width(2.0).fill(0.0));
                }
            });
        }
        ChartKind::Scatter => {
            plot.show(ui, |pu| {
                for (si, (name, vals)) in cfg.series.iter().enumerate() {
                    let color = palette_color(&cfg.palette, si);
                    let pts: PlotPoints = cfg.xs.iter().zip(vals.iter())
                        .map(|(&x, &y)| [x, y])
                        .collect();
                    pu.points(Points::new(pts).name(name).color(color).radius(4.0));
                }
            });
        }
        ChartKind::Hist => {
            plot.show(ui, |pu| {
                if let Some((name, vals)) = cfg.series.first() {
                    if !vals.is_empty() {
                        let color = palette_color(&cfg.palette, 0);
                        let bins  = cfg.bins.max(2);
                        let min_v = vals.iter().cloned().fold(f64::INFINITY,     f64::min);
                        let max_v = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                        let step  = (max_v - min_v) / bins as f64;
                        let mut counts = vec![0usize; bins];
                        for &v in vals {
                            let idx = ((v - min_v) / step).floor() as usize;
                            counts[idx.min(bins - 1)] += 1;
                        }
                        let bars: Vec<Bar> = counts.iter().enumerate().map(|(i, &c)| {
                            let x = min_v + step * (i as f64 + 0.5);
                            Bar::new(x, c as f64).width(step * 0.9).fill(color)
                        }).collect();
                        pu.bar_chart(BarChart::new(bars).name(name));
                    }
                }
            });
        }
        ChartKind::Pie => unreachable!(),
    }
}

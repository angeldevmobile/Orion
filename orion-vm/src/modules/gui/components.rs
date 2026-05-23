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
    /// Fade in/out suave basado en un booleano. Usa egui animate_bool internamente.
    FadeGroup { id: String, show: bool, children: Vec<Component> },
    /// Slide in desde abajo con opacidad. El componente aparece animado la primera vez.
    SlideIn { id: String, children: Vec<Component> },
}

//     Render

use std::collections::HashMap;
use eframe::egui;
use super::theme;

const ACCENT: egui::Color32 = egui::Color32::from_rgb(108, 99, 255);

/// Renderiza un componente. Si el usuario interactúa, escribe el evento en `event`.
/// El evento es el nombre del botón pulsado, o "id:valor" para inputs.
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
            // ui.scope() aísla los cambios de visuals al widget — se restauran automáticamente
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
            // animate_bool devuelve 0.0 (oculto) → 1.0 (visible) suavemente
            let alpha = ui.ctx().animate_bool(egui::Id::new(id), *show);
            if alpha > 0.001 {
                ui.set_opacity(alpha);
                for child in children {
                    render(ui, child, fields, event);
                    ui.add_space(4.0);
                }
                ui.set_opacity(1.0);
                // Mientras anima, pide siguiente frame
                if alpha < 0.999 {
                    ui.ctx().request_repaint();
                }
            }
        }

        Component::SlideIn { id, children } => {
            // Anima desde 0.0 → 1.0 en 0.35s la primera vez que aparece
            let t = ui.ctx().animate_value_with_time(
                egui::Id::new(id),
                1.0,
                0.35,
            );
            // Offset vertical: empieza 24px abajo, llega a 0
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
    }
}

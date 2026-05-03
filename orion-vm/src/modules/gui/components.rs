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
    Divider,

    //    Layout
    Card(Vec<Component>),
    Row(Vec<Component>),
    Col(Vec<Component>),
}

//     Render

use std::collections::HashMap;
use eframe::egui;

const ACCENT: egui::Color32 = egui::Color32::from_rgb(108, 99, 255);

pub fn render(
    ui: &mut egui::Ui,
    comp: &Component,
    fields: &mut HashMap<String, String>,
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
            ui.checkbox(&mut checked, label.as_str());
            *val = if checked { "true".into() } else { "false".into() };
        }
        Component::Press(label, style) => {
            let fill = style.bg_color().unwrap_or(ACCENT);
            let rt = match style.fg_color() {
                Some(c) => egui::RichText::new(label).color(c),
                None    => egui::RichText::new(label),
            };
            ui.add_sized([120.0, 36.0], egui::Button::new(rt).fill(fill));
        }
        Component::Ghost(label, style) => {
            let color = style.fg_color().unwrap_or(ACCENT);
            ui.add(
                egui::Button::new(label)
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(egui::Stroke::new(1.5, color)),
            );
        }
        Component::Tap(label) => {
            ui.link(label);
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
        Component::Divider => {
            ui.separator();
        }
        Component::Card(children) => {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(26, 26, 40))
                .rounding(egui::Rounding::same(10.0))
                .inner_margin(egui::Margin::same(16.0))
                .show(ui, |ui| {
                    for child in children {
                        render(ui, child, fields);
                        ui.add_space(6.0);
                    }
                });
        }
        Component::Row(children) => {
            ui.horizontal(|ui| {
                for child in children {
                    render(ui, child, fields);
                }
            });
        }
        Component::Col(children) => {
            ui.vertical(|ui| {
                for child in children {
                    render(ui, child, fields);
                    ui.add_space(6.0);
                }
            });
        }
    }
}

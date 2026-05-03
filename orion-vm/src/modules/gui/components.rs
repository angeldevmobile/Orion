/// Componentes nativos de Orion — nombres propios del lenguaje
#[derive(Clone)]
pub enum Component {
    //    Tipografía                                               
    Heading(String),
    Text(String),
    Caption(String),

    //    Inputs                                                   
    Field { id: String, placeholder: String },
    Toggle { id: String, label: String },

    //    Acciones                                                 
    Press(String),   // botón primario
    Ghost(String),   // botón outline
    Tap(String),     // botón de texto / link

    //    Display                                                  
    Badge(String),
    Divider,

    //    Layout                                                   
    Card(Vec<Component>),
    Row(Vec<Component>),
    Col(Vec<Component>),
}

//     Render                                                                    

use std::collections::HashMap;
use eframe::egui;

pub fn render(
    ui: &mut egui::Ui,
    comp: &Component,
    fields: &mut HashMap<String, String>,
) {
    match comp {
        Component::Heading(t) => {
            ui.heading(t);
        }
        Component::Text(t) => {
            ui.label(t);
        }
        Component::Caption(t) => {
            ui.small(t);
        }
        Component::Field { id, placeholder } => {
            let val = fields.entry(id.clone()).or_default();
            ui.add(
                egui::TextEdit::singleline(val)
                    .hint_text(placeholder.as_str())
                    .desired_width(f32::INFINITY),
            );
        }
        Component::Toggle { id, label } => {
            let val = fields.entry(id.clone()).or_insert_with(|| "false".into());
            let mut checked = val == "true";
            ui.checkbox(&mut checked, label.as_str());
            *val = if checked { "true".into() } else { "false".into() };
        }
        Component::Press(label) => {
            ui.add_sized([120.0, 36.0], egui::Button::new(label));
        }
        Component::Ghost(label) => {
            ui.add(
                egui::Button::new(label).stroke(egui::Stroke::new(
                    1.5,
                    egui::Color32::from_rgb(108, 99, 255),
                )),
            );
        }
        Component::Tap(label) => {
            ui.link(label);
        }
        Component::Badge(text) => {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(108, 99, 255))
                .rounding(egui::Rounding::same(12.0))
                .inner_margin(egui::Margin::symmetric(10.0, 4.0))
                .show(ui, |ui| {
                    ui.colored_label(egui::Color32::WHITE, text);
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

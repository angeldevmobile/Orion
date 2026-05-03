use eframe::egui;
use std::collections::HashMap;
use super::components::{Component, render};
use super::theme;

pub fn launch(
    title:      String,
    width:      f32,
    height:     f32,
    components: Vec<Component>,
    field_vals: HashMap<String, String>,
) -> Result<(), String> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(&title)
            .with_inner_size([width, height])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        &title,
        opts,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(OrionApp { components, field_vals }))
        }),
    )
    .map_err(|e| format!("gui.run: {e}"))
}

//     App                                                                       

struct OrionApp {
    components: Vec<Component>,
    field_vals: HashMap<String, String>,
}

impl eframe::App for OrionApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(24.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_max_width(640.0);
                for comp in &self.components {
                    render(ui, comp, &mut self.field_vals);
                    ui.add_space(6.0);
                }
            });
        });
    }
}

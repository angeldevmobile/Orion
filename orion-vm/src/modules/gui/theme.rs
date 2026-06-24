use eframe::egui;

/// Paleta y estilo oficial de Orion UI
pub const ACCENT:    egui::Color32 = egui::Color32::from_rgb(108, 99,  255);
pub const ACCENT_H:  egui::Color32 = egui::Color32::from_rgb(90,  82,  210);
pub const BG:        egui::Color32 = egui::Color32::from_rgb(15,  15,   23);
pub const SURFACE:   egui::Color32 = egui::Color32::from_rgb(26,  26,   40);

/// Colores semánticos del design system — disponibles como nombres en componentes
#[allow(dead_code)] pub const SUCCESS: egui::Color32 = egui::Color32::from_rgb(34,  197,  94);
#[allow(dead_code)] pub const WARNING: egui::Color32 = egui::Color32::from_rgb(234, 179,   8);
#[allow(dead_code)] pub const ERROR:   egui::Color32 = egui::Color32::from_rgb(239,  68,  68);
#[allow(dead_code)] pub const INFO:    egui::Color32 = egui::Color32::from_rgb(59,  130, 246);

pub fn apply(ctx: &egui::Context) {
    let mut vis = egui::Visuals::dark();

    vis.window_fill = BG;
    vis.panel_fill  = BG;

    vis.widgets.active.bg_fill          = ACCENT;
    vis.widgets.active.rounding         = egui::Rounding::same(8.0);
    vis.widgets.hovered.bg_fill         = ACCENT_H;
    vis.widgets.hovered.rounding        = egui::Rounding::same(8.0);
    vis.widgets.inactive.bg_fill        = SURFACE;
    vis.widgets.inactive.rounding       = egui::Rounding::same(8.0);
    vis.widgets.noninteractive.bg_fill  = SURFACE;
    vis.widgets.noninteractive.rounding = egui::Rounding::same(8.0);
    vis.window_rounding                 = egui::Rounding::same(12.0);
    vis.selection.bg_fill               = ACCENT;

    ctx.set_visuals(vis);

    let mut style = (*ctx.style()).clone();
    use egui::{FontFamily::Proportional, FontId, TextStyle::*};
    style.text_styles = [
        (Heading,   FontId::new(26.0, Proportional)),
        (Body,      FontId::new(15.0, Proportional)),
        (Small,     FontId::new(12.0, Proportional)),
        (Button,    FontId::new(14.0, Proportional)),
        (Monospace, FontId::new(13.0, egui::FontFamily::Monospace)),
    ]
    .into();
    style.spacing.item_spacing   = egui::vec2(8.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 8.0);
    ctx.set_style(style);
}

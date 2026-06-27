use eframe::egui;
use std::cell::RefCell;
use super::state::{with_state, ThemeConfig};

/// Colores semánticos del design system — disponibles como nombres en componentes.
/// (No dependen del tema; son constantes de significado.)
#[allow(dead_code)] pub const SUCCESS: egui::Color32 = egui::Color32::from_rgb(34,  197,  94);
#[allow(dead_code)] pub const WARNING: egui::Color32 = egui::Color32::from_rgb(234, 179,   8);
#[allow(dead_code)] pub const ERROR:   egui::Color32 = egui::Color32::from_rgb(239,  68,  68);
#[allow(dead_code)] pub const INFO:    egui::Color32 = egui::Color32::from_rgb(59,  130, 246);

/// Tema RESUELTO (valores concretos). Se arma desde defaults + overrides que el
/// developer pasa con `gui.theme({...})`. NADA está fijado a fuego: accent, fondo,
/// superficie, texto, redondeo, tipografía y modo claro/oscuro son todos overridables.
#[derive(Clone)]
pub struct Theme {
    pub accent:   egui::Color32,
    pub accent_h: egui::Color32,
    pub bg:       egui::Color32,
    pub surface:  egui::Color32,
    pub text:     egui::Color32,
    pub rounding: f32,
    pub heading:  f32,
    pub body:     f32,
    pub spacing:  f32,
    pub dark:     bool,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            accent:   egui::Color32::from_rgb(108, 99,  255),
            accent_h: egui::Color32::from_rgb(90,  82,  210),
            bg:       egui::Color32::from_rgb(15,  15,  23),
            surface:  egui::Color32::from_rgb(26,  26,  40),
            text:     egui::Color32::from_rgb(235, 235, 245),
            rounding: 8.0,
            heading:  26.0,
            body:     15.0,
            spacing:  8.0,
            dark:     true,
        }
    }
}

fn rgb(c: [u8; 3]) -> egui::Color32 { egui::Color32::from_rgb(c[0], c[1], c[2]) }

impl Theme {
    /// Resuelve el tema final: defaults sobreescritos por lo que el developer fijó.
    pub fn from_config(cfg: &ThemeConfig) -> Theme {
        // Si pide modo claro y no dio colores, arrancar de una base clara legible.
        let mut t = if cfg.light == Some(true) {
            Theme {
                bg:      egui::Color32::from_rgb(247, 247, 250),
                surface: egui::Color32::from_rgb(255, 255, 255),
                text:    egui::Color32::from_rgb(20, 20, 30),
                dark:    false,
                ..Theme::default()
            }
        } else {
            Theme::default()
        };
        if let Some(c) = cfg.accent  { t.accent = rgb(c); t.accent_h = rgb(c); }
        if let Some(c) = cfg.bg      { t.bg = rgb(c); }
        if let Some(c) = cfg.surface { t.surface = rgb(c); }
        if let Some(c) = cfg.text    { t.text = rgb(c); }
        if let Some(v) = cfg.rounding { t.rounding = v; }
        if let Some(v) = cfg.heading  { t.heading = v; }
        if let Some(v) = cfg.body     { t.body = v; }
        if let Some(v) = cfg.spacing  { t.spacing = v; }
        t
    }

    /// Resuelve el tema desde el estado actual del GUI (thread-local).
    pub fn from_state() -> Theme {
        Theme::from_config(&with_state(|s| s.theme.clone()))
    }
}

thread_local! {
    static CURRENT: RefCell<Theme> = RefCell::new(Theme::default());
}

/// Tema activo — usado por los componentes (card surface, acento, etc.).
pub fn current() -> Theme { CURRENT.with(|t| t.borrow().clone()) }

/// Aplica el tema a egui y lo deja como `current()` para los componentes.
pub fn apply(ctx: &egui::Context, t: &Theme) {
    CURRENT.with(|c| *c.borrow_mut() = t.clone());

    let mut vis = if t.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    vis.window_fill = t.bg;
    vis.panel_fill  = t.bg;
    vis.override_text_color = Some(t.text);

    let r = egui::Rounding::same(t.rounding);
    vis.widgets.active.bg_fill          = t.accent;
    vis.widgets.active.rounding         = r;
    vis.widgets.hovered.bg_fill         = t.accent_h;
    vis.widgets.hovered.rounding        = r;
    vis.widgets.inactive.bg_fill        = t.surface;
    vis.widgets.inactive.rounding       = r;
    vis.widgets.noninteractive.bg_fill  = t.surface;
    vis.widgets.noninteractive.rounding = r;
    vis.window_rounding                 = egui::Rounding::same(t.rounding * 1.5);
    vis.selection.bg_fill               = t.accent;
    ctx.set_visuals(vis);

    let mut style = (*ctx.style()).clone();
    use egui::{FontFamily::Proportional, FontId, TextStyle::*};
    style.text_styles = [
        (Heading,   FontId::new(t.heading,     Proportional)),
        (Body,      FontId::new(t.body,        Proportional)),
        (Small,     FontId::new(t.body * 0.80, Proportional)),
        (Button,    FontId::new(t.body * 0.93, Proportional)),
        (Monospace, FontId::new(t.body * 0.87, egui::FontFamily::Monospace)),
    ]
    .into();
    style.spacing.item_spacing   = egui::vec2(t.spacing, t.spacing + 2.0);
    style.spacing.button_padding = egui::vec2(t.spacing * 2.0, t.spacing);
    ctx.set_style(style);
}

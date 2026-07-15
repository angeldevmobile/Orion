use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32};
use super::components::Component;
use crate::eval_value::EvalValue;

/// En true, gui.run() no lanza eframe — el watcher maneja la ventana.
pub static IS_WATCH_MODE:    AtomicBool = AtomicBool::new(false);
/// En true, el script está corriendo como re-evaluación reactiva (evento de UI).
pub static IS_REACTIVE_MODE: AtomicBool = AtomicBool::new(false);
/// Intervalo de animación en ms fijado por gui.tick(ms); 0 = sin animación.
/// El runner lo resetea antes de cada re-run: el script debe volver a llamar
/// gui.tick en cada ejecución para mantener viva la animación.
pub static TICK_MS: AtomicU32 = AtomicU32::new(0);

thread_local! {
    /// Ruta del script actual — necesaria para re-evaluación reactiva.
    static SCRIPT_PATH: RefCell<String> = RefCell::new(String::new());
}

pub fn set_script_path(path: &str) {
    SCRIPT_PATH.with(|p| *p.borrow_mut() = path.to_string());
}

pub fn get_script_path() -> String {
    SCRIPT_PATH.with(|p| p.borrow().clone())
}

thread_local! {
    pub static STATE: RefCell<GuiState> = RefCell::new(GuiState::default());
}

/// Overrides de tema fijados por el developer con `gui.theme({...})`. Todo es
/// opcional: lo que no se setea cae al default. Datos planos (sin egui) para que
/// el estado siga siendo clonable y libre de dependencias gráficas.
#[derive(Default, Clone)]
pub struct ThemeConfig {
    pub accent:   Option<[u8; 3]>,
    pub bg:       Option<[u8; 3]>,
    pub surface:  Option<[u8; 3]>,
    pub text:     Option<[u8; 3]>,
    pub rounding: Option<f32>,
    pub heading:  Option<f32>,
    pub body:     Option<f32>,
    pub spacing:  Option<f32>,
    pub light:    Option<bool>,
}

#[derive(Default, Clone)]
pub struct GuiState {
    pub title:           String,
    pub width:           f32,
    pub height:          f32,
    pub components:      Vec<Component>,
    pub field_vals:      HashMap<String, String>,
    pub container_stack: Vec<(String, usize)>,
    /// Tema fijado por el developer (overrides; vacío = defaults).
    pub theme:           ThemeConfig,

    // UI-3 — Estado reactivo
    /// Valores que persisten entre re-ejecuciones del script
    pub state_store:     HashMap<String, EvalValue>,
    /// Nombre del último evento disparado (botón pulsado, toggle, pick)
    pub last_event:      String,
    /// true cuando el script está corriendo como re-evaluación reactiva
    pub is_reactive:     bool,
}

pub fn with_state<F, R>(f: F) -> R
where
    F: FnOnce(&mut GuiState) -> R,
{
    STATE.with(|s| f(&mut s.borrow_mut()))
}

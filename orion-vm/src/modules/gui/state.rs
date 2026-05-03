use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use super::components::Component;

/// Cuando está en true, gui.run() no lanza eframe — el watcher maneja la ventana.
pub static IS_WATCH_MODE: AtomicBool = AtomicBool::new(false);

thread_local! {
    pub static STATE: RefCell<GuiState> = RefCell::new(GuiState::default());
}

#[derive(Default, Clone)]
pub struct GuiState {
    pub title:      String,
    pub width:      f32,
    pub height:     f32,
    pub components: Vec<Component>,
    pub field_vals: HashMap<String, String>,
}

pub fn with_state<F, R>(f: F) -> R
where
    F: FnOnce(&mut GuiState) -> R,
{
    STATE.with(|s| f(&mut s.borrow_mut()))
}

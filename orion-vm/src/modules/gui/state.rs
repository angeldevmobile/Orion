use std::cell::RefCell;
use std::collections::HashMap;
use super::components::Component;

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

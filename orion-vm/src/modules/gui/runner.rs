use eframe::egui;
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};
use super::components::{Component, render};
use super::theme;

//    Lanzador normal                                                            

pub fn launch(
    title:      String,
    width:      f32,
    height:     f32,
    components: Vec<Component>,
    field_vals: HashMap<String, String>,
) -> Result<(), String> {
    let opts = native_opts(&title, width, height);
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

//    Lanzador con hot-reload                                                    

pub fn launch_watch(
    path:       &str,
    title:      String,
    width:      f32,
    height:     f32,
    initial:    Vec<Component>,
    field_vals: HashMap<String, String>,
) -> Result<(), String> {
    // Canal entre el watcher thread y OrionAppWatch::update()
    let bus: Arc<Mutex<Option<Vec<Component>>>> = Arc::new(Mutex::new(None));

    let opts = native_opts(&title, width, height);
    let path_owned  = path.to_string();
    let bus_eframe  = bus.clone();

    eframe::run_native(
        &title,
        opts,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);

            // El watcher thread necesita el Context para despertar a eframe al recargar
            let ctx     = cc.egui_ctx.clone();
            let bus_w   = bus_eframe.clone();
            let path_w  = path_owned.clone();

            thread::spawn(move || {
                let mut last = mtime(&path_w);
                loop {
                    thread::sleep(Duration::from_millis(300));
                    let cur = mtime(&path_w);
                    if cur != last {
                        last = cur;
                        // Pausa breve para que el editor termine de escribir el archivo
                        thread::sleep(Duration::from_millis(80));
                        if let Some(comps) = reload_script(&path_w) {
                            *bus_w.lock().unwrap() = Some(comps);
                            ctx.request_repaint(); // despierta eframe inmediatamente
                            eprintln!("  [Orion] hot reload OK");
                        }
                    }
                }
            });

            Ok(Box::new(OrionAppWatch {
                components: initial,
                field_vals,
                bus: bus_eframe,
            }))
        }),
    )
    .map_err(|e| format!("gui.watch: {e}"))
}

//    Helpers internos                                                           

fn native_opts(title: &str, width: f32, height: f32) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([width, height])
            .with_resizable(true),
        ..Default::default()
    }
}

fn mtime(path: &str) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

/// Re-evalúa el script Orion en el hilo actual (thread-local GUI state fresco).
/// Devuelve los nuevos componentes, o None si hubo error (el error se imprime en stderr).
fn reload_script(path: &str) -> Option<Vec<Component>> {
    use crate::{lexer, parser, codegen, vm};

    let raw = fs::read_to_string(path).ok()?;
    let src = raw.strip_prefix('\u{FEFF}').unwrap_or(&raw);

    let tokens = match lexer::lex(src) {
        Ok(t) => t,
        Err(e) => { eprintln!("  [!] Léxico  {}:{} — {}", e.line, e.col, e.message); return None; }
    };
    let stmts = match parser::parse(tokens) {
        Ok(s) => s,
        Err(e) => { eprintln!("  [!] Parse   línea {} — {}", e.line, e.message); return None; }
    };
    let bc = match codegen::compile(stmts) {
        Ok(b) => b,
        Err(e) => { eprintln!("  [!] Codegen línea {} — {}", e.line, e.message); return None; }
    };

    // Limpiar el estado GUI de ESTE hilo antes de re-evaluar
    super::state::with_state(|s| { s.components.clear(); s.field_vals.clear(); });

    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    if let Err(e) = machine.run() {
        eprintln!("  [!] Runtime — {e}");
        return None;
    }

    let comps = super::state::with_state(|s| s.components.clone());
    if comps.is_empty() { return None; }
    Some(comps)
}

//    Apps                                                                       

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

struct OrionAppWatch {
    components: Vec<Component>,
    field_vals: HashMap<String, String>,
    bus: Arc<Mutex<Option<Vec<Component>>>>,
}

impl eframe::App for OrionAppWatch {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Leer nuevos componentes del watcher si llegaron
        if let Ok(mut lock) = self.bus.try_lock() {
            if let Some(new_comps) = lock.take() {
                self.components = new_comps;
                self.field_vals.clear();
            }
        }

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

        // Indicador sutil de watch mode en esquina superior derecha
        egui::Area::new(egui::Id::new("watch_badge"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 8.0))
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("● watch")
                        .small()
                        .color(egui::Color32::from_rgb(108, 99, 255)),
                );
            });

        // Polling activo: eframe verifica el bus cada 300ms sin depender
        // de que request_repaint() desde el watcher thread lo despierte.
        ctx.request_repaint_after(Duration::from_millis(300));
    }
}

use eframe::egui;
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant, SystemTime};
use super::components::{Component, render};
use super::state::{with_state, IS_REACTIVE_MODE, TICK_MS};
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
            theme::apply(&cc.egui_ctx, &theme::Theme::from_state());
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
            theme::apply(&cc.egui_ctx, &theme::Theme::from_state());

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
            .with_resizable(true)
            .with_icon(orion_icon()),
        ..Default::default()
    }
}

/// Ícono de la ventana generado en código (sin depender de un asset): disco en
/// el color de acento de Orion con un anillo blanco — la "O" de Orion. Reemplaza
/// el ícono genérico del sistema.
fn orion_icon() -> egui::IconData {
    const SIZE: i32 = 64;
    let accent = [108u8, 99, 255];     // acento Orion
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    let c = (SIZE as f32 - 1.0) / 2.0; // centro
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - c;
            let dy = y as f32 - c;
            let r = (dx * dx + dy * dy).sqrt();
            let (px, a) = if r <= 30.0 {
                // anillo blanco (la "O") sobre disco de acento
                if (14.0..=21.0).contains(&r) {
                    ([245u8, 245, 250], 255u8)
                } else {
                    (accent, 255u8)
                }
            } else if r <= 31.5 {
                (accent, 120u8) // borde suavizado (anti-alias simple)
            } else {
                ([0, 0, 0], 0u8) // fuera del disco: transparente
            };
            rgba.extend_from_slice(&[px[0], px[1], px[2], a]);
        }
    }
    egui::IconData { rgba, width: SIZE as u32, height: SIZE as u32 }
}

fn mtime(path: &str) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

/// Re-evalúa el script Orion preservando state_store y field_vals.
/// `event` se inyecta como last_event antes de correr el script.
fn rerun_script(
    path:       &str,
    event:      String,
    field_vals: HashMap<String, String>,
) -> Option<(Vec<Component>, HashMap<String, String>)> {
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

    // Preparar estado: preservar state_store, inyectar event, restaurar field_vals
    with_state(|s| {
        s.components.clear();
        s.container_stack.clear();
        s.last_event  = event;
        s.field_vals  = field_vals;
        s.is_reactive = true;
    });
    IS_REACTIVE_MODE.store(true, Ordering::Relaxed);
    // La animación no es pegajosa: si el script deja de llamar gui.tick en
    // esta re-ejecución, el reloj se apaga.
    TICK_MS.store(0, Ordering::Relaxed);

    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    if let Err(e) = machine.run() {
        eprintln!("  [!] Runtime — {e}");
    }

    IS_REACTIVE_MODE.store(false, Ordering::Relaxed);

    let (comps, fields) = with_state(|s| {
        s.last_event = String::new();
        (s.components.clone(), s.field_vals.clone())
    });
    if comps.is_empty() { return None; }
    Some((comps, fields))
}

/// Versión para watch-mode (no reactivo): limpia todo excepto state_store.
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

    with_state(|s| {
        s.components.clear();
        s.container_stack.clear();
        s.field_vals.clear();
    });

    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    if let Err(e) = machine.run() {
        eprintln!("  [!] Runtime — {e}");
        return None;
    }

    let comps = with_state(|s| s.components.clone());
    if comps.is_empty() { return None; }
    Some(comps)
}

//    Render raíz (común a las 3 apps)

/// Dibuja los componentes: si hay un `Sidebar` a nivel raíz lo pinta como
/// SidePanel izquierdo fijo, y el resto va en el panel central con scroll.
fn render_root(
    ctx: &egui::Context,
    components: &[Component],
    fields: &mut HashMap<String, String>,
    event: &mut Option<String>,
) {
    // Ancho del sidebar: lo decide el dev en gui.sidebar(ancho); aquí solo lo usamos.
    let sidebar_w = components.iter().find_map(|c| match c {
        Component::Sidebar(w, _) => Some(*w),
        _ => None,
    });
    if let Some(w) = sidebar_w {
        egui::SidePanel::left("orion_sidebar")
            .resizable(false)
            .exact_width(w)
            .frame(
                egui::Frame::none()
                    .fill(theme::current().surface)
                    .inner_margin(egui::Margin::same(16.0)),
            )
            .show(ctx, |ui| {
                for comp in components {
                    if let Component::Sidebar(_, children) = comp {
                        for child in children {
                            render(ui, child, fields, event);
                            ui.add_space(8.0);
                        }
                    }
                }
            });
    }

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(24.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            for comp in components {
                if matches!(comp, Component::Sidebar(..)) {
                    continue;
                }
                render(ui, comp, fields, event);
                ui.add_space(6.0);
            }
        });
    });
}

//    Apps

struct OrionApp {
    components: Vec<Component>,
    field_vals: HashMap<String, String>,
}

impl eframe::App for OrionApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut event: Option<String> = None;
        render_root(ctx, &self.components, &mut self.field_vals, &mut event);
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

        let mut event: Option<String> = None;
        render_root(ctx, &self.components, &mut self.field_vals, &mut event);

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

//    Reactive app — re-corre el script en cada evento de UI

/// Lanza la ventana en modo reactivo: cada click/toggle/pick re-corre el script.
/// El state_store persiste entre re-ejecuciones; field_vals se restaura también.
pub fn launch_reactive(
    title:      String,
    width:      f32,
    height:     f32,
    components: Vec<Component>,
    field_vals: HashMap<String, String>,
    path:       String,
) -> Result<(), String> {
    let opts = native_opts(&title, width, height);
    eframe::run_native(
        &title,
        opts,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx, &theme::Theme::from_state());
            Ok(Box::new(OrionAppReactive {
                path,
                components,
                field_vals,
                last_tick: Instant::now(),
            }))
        }),
    )
    .map_err(|e| format!("gui.run: {e}"))
}

struct OrionAppReactive {
    path:       String,
    components: Vec<Component>,
    field_vals: HashMap<String, String>,
    last_tick:  Instant,
}

impl eframe::App for OrionAppReactive {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut fired_event: Option<String> = None;
        render_root(ctx, &self.components, &mut self.field_vals, &mut fired_event);

        // Animación: si el script pidió gui.tick(ms) y no hubo evento de UI en
        // este frame, disparamos "tick" cuando toca. Los eventos del usuario
        // tienen prioridad: la animación nunca se traga un clic.
        let tick_ms = TICK_MS.load(Ordering::Relaxed);
        if tick_ms > 0 {
            if fired_event.is_none()
                && self.last_tick.elapsed().as_millis() as u32 >= tick_ms
            {
                fired_event = Some("tick".into());
                self.last_tick = Instant::now();
            }
            // Mantener vivo el loop de render sin girar la CPU: despertar como
            // muy tarde en el próximo tick (tope 16ms ≈ 60fps).
            ctx.request_repaint_after(Duration::from_millis(tick_ms.min(16) as u64));
        }

        // Si se disparó un evento, re-evaluar el script EN ESTE MISMO HILO de UI.
        // El state_store vive en el thread_local STATE de este hilo; correrlo en
        // un hilo aparte (thread::spawn) usaría un STATE nuevo y vacío, perdiendo
        // todo el estado (acc/op/cur) en cada clic. La re-evaluación es trivial
        // en coste, así que correrla síncronamente no afecta la fluidez.
        if let Some(ev) = fired_event {
            if let Some((new_comps, new_fields)) =
                rerun_script(&self.path, ev, self.field_vals.clone())
            {
                self.components = new_comps;
                self.field_vals = new_fields;
            }
            ctx.request_repaint();
        }
    }
}

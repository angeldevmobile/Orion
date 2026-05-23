pub mod components;
pub mod runner;
pub mod state;
pub mod theme;

use std::sync::atomic::Ordering;
use crate::eval_value::EvalValue;
use components::{Component, Style};
use state::{with_state, IS_WATCH_MODE, IS_REACTIVE_MODE, get_script_path};

//     Dispatcher principal — gui.función(args)

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        //    Configuración de panel
        "panel" => {
            let title = str_arg(&args, 0).unwrap_or_else(|| "Orion App".into());
            let w     = f32_arg(&args, 1).unwrap_or(900.0);
            let h     = f32_arg(&args, 2).unwrap_or(600.0);
            with_state(|s| { s.title = title; s.width = w; s.height = h; });
            Ok(EvalValue::Null)
        }

        //    Tipografía — gui.heading("texto", "colorTexto?")
        "heading" => push(Component::Heading(req_str(&args, 0, "heading")?, style_args(&args, 1, 2))),
        "text"    => push(Component::Text(req_str(&args, 0, "text")?, style_args(&args, 1, 2))),
        "caption" => push(Component::Caption(req_str(&args, 0, "caption")?, style_args(&args, 1, 2))),

        //    Inputs — gui.field("placeholder", "bgColor?", "textColor?")
        "field" => {
            let placeholder = str_arg(&args, 0).unwrap_or_default();
            let style = style_args(&args, 1, 2);
            let id = with_state(|s| format!("field_{}", s.components.len()));
            push(Component::Field { id, placeholder, style })
        }
        "toggle" => {
            let label = str_arg(&args, 0).unwrap_or_default();
            let id = with_state(|s| format!("toggle_{}", s.components.len()));
            push(Component::Toggle { id, label })
        }

        //    Acciones — gui.press("label", "bgColor?", "textColor?")
        "press" => push(Component::Press(
            str_arg(&args, 0).unwrap_or_else(|| "OK".into()),
            style_args(&args, 1, 2),
        )),
        "ghost" => push(Component::Ghost(req_str(&args, 0, "ghost")?, style_args(&args, 1, 2))),
        "tap"   => push(Component::Tap(req_str(&args, 0, "tap")?)),

        //    Display
        "badge"   => push(Component::Badge(req_str(&args, 0, "badge")?, style_args(&args, 1, 2))),
        "divider" => push(Component::Divider),
        "spacer"  => push(Component::Spacer(f32_arg(&args, 0).unwrap_or(12.0))),
        "banner"  => {
            let title    = req_str(&args, 0, "banner")?;
            let subtitle = str_arg(&args, 1);
            let style    = style_args(&args, 2, 3);
            push(Component::Banner { title, subtitle, style })
        }
        "avatar"  => {
            let text  = req_str(&args, 0, "avatar")?;
            let size  = f32_arg(&args, 1).unwrap_or(40.0);
            let style = style_args(&args, 2, 3);
            push(Component::Avatar { text, size, style })
        }

        //    Inputs avanzados
        "pick" => {
            let id = str_arg(&args, 0).unwrap_or_else(|| with_state(|s| format!("pick_{}", s.components.len())));
            let options: Vec<String> = match args.get(1) {
                Some(EvalValue::List(l)) => l.iter().map(|v| match v {
                    EvalValue::Str(s) => s.clone(),
                    other             => format!("{other:?}"),
                }).collect(),
                _ => vec![],
            };
            let style = style_args(&args, 2, 3);
            push(Component::Pick { id, options, style })
        }
        "slide" => {
            let id   = str_arg(&args, 0).unwrap_or_else(|| with_state(|s| format!("slide_{}", s.components.len())));
            let min  = f32_arg(&args, 1).unwrap_or(0.0);
            let max  = f32_arg(&args, 2).unwrap_or(100.0);
            let step = f32_arg(&args, 3).unwrap_or(1.0);
            push(Component::Slide { id, min, max, step })
        }

        //    Layout — containers anidados
        //    gui.card() / gui.row() / gui.col() / gui.zone() → abre el contenedor
        //    gui.end() → cierra el último contenedor abierto
        "card" => {
            with_state(|s| s.container_stack.push(("card".into(), s.components.len())));
            Ok(EvalValue::Null)
        }
        "row" => {
            with_state(|s| s.container_stack.push(("row".into(), s.components.len())));
            Ok(EvalValue::Null)
        }
        "col" => {
            with_state(|s| s.container_stack.push(("col".into(), s.components.len())));
            Ok(EvalValue::Null)
        }
        "zone" => {
            let style = style_args(&args, 0, 1);
            with_state(|s| {
                let idx = s.components.len();
                // guardamos el estilo codificado en el tipo de contenedor
                s.container_stack.push((format!("zone|{}|{}",
                    style.bg.map(|c| format!("{},{},{}", c[0], c[1], c[2])).unwrap_or_default(),
                    style.fg.map(|c| format!("{},{},{}", c[0], c[1], c[2])).unwrap_or_default(),
                ), idx));
            });
            Ok(EvalValue::Null)
        }
        "end" => {
            with_state(|s| {
                if let Some((kind, start)) = s.container_stack.pop() {
                    let children: Vec<Component> = s.components.drain(start..).collect();
                    let comp = if kind == "card" {
                        Component::Card(children)
                    } else if kind == "row" {
                        Component::Row(children)
                    } else if kind == "col" {
                        Component::Col(children)
                    } else if kind.starts_with("zone|") {
                        let parts: Vec<&str> = kind.splitn(3, '|').collect();
                        let bg = parse_rgb_tag(parts.get(1).copied().unwrap_or(""));
                        let fg = parse_rgb_tag(parts.get(2).copied().unwrap_or(""));
                        Component::Zone(children, Style { bg, fg })
                    } else if kind.starts_with("fade|") {
                        // "fade|id|show"
                        let parts: Vec<&str> = kind.splitn(3, '|').collect();
                        let id   = parts.get(1).copied().unwrap_or("").to_string();
                        let show = parts.get(2).copied().unwrap_or("false") == "true";
                        Component::FadeGroup { id, show, children }
                    } else if kind.starts_with("slide_in|") {
                        let id = kind.strip_prefix("slide_in|").unwrap_or("").to_string();
                        Component::SlideIn { id, children }
                    } else {
                        Component::Col(children)
                    };
                    s.components.push(comp);
                }
            });
            Ok(EvalValue::Null)
        }

        // UI-3 — Estado reactivo
        //
        // gui.val("key", default) → lee del state_store o devuelve default
        "val" => {
            let key = req_str(&args, 0, "val")?;
            let default = args.get(1).cloned().unwrap_or(EvalValue::Null);
            let v = with_state(|s| s.state_store.get(&key).cloned().unwrap_or(default));
            Ok(v)
        }

        // gui.set("key", value) → escribe en state_store
        "set" => {
            let key = req_str(&args, 0, "set")?;
            let val = args.get(1).cloned().unwrap_or(EvalValue::Null);
            with_state(|s| s.state_store.insert(key, val));
            Ok(EvalValue::Null)
        }

        // gui.pressed("name") → true si ese botón fue el último evento
        "pressed" => {
            let name = req_str(&args, 0, "pressed")?;
            let yes  = with_state(|s| s.last_event == name);
            Ok(EvalValue::Bool(yes))
        }

        // gui.ev() → nombre del último evento disparado
        "ev" => {
            let ev = with_state(|s| s.last_event.clone());
            Ok(EvalValue::Str(ev))
        }

        // gui.value("id") → valor actual de un field, toggle, pick o slide
        "value" => {
            let id  = req_str(&args, 0, "value")?;
            let val = with_state(|s| s.field_vals.get(&id).cloned().unwrap_or_default());
            Ok(EvalValue::Str(val))
        }

        // UI-5 — Animaciones
        //
        // gui.fade("id", show_bool) → abre contenedor que hace fade in/out
        "fade" => {
            let id   = req_str(&args, 0, "fade")?;
            let show = match args.get(1) {
                Some(EvalValue::Bool(b)) => *b,
                Some(EvalValue::Int(n))  => *n != 0,
                _                        => true,
            };
            with_state(|s| s.container_stack.push((format!("fade|{id}|{show}"), s.components.len())));
            Ok(EvalValue::Null)
        }

        // gui.slide_in("id") → abre contenedor que aparece animado desde abajo
        "slide_in" => {
            let id = req_str(&args, 0, "slide_in")?;
            with_state(|s| s.container_stack.push((format!("slide_in|{id}"), s.components.len())));
            Ok(EvalValue::Null)
        }

        //    Lanzar ventana
        "run" => {
            // En watch mode o reactive re-run: el runner ya gestiona la ventana
            if IS_WATCH_MODE.load(Ordering::Relaxed)
                || IS_REACTIVE_MODE.load(Ordering::Relaxed)
            {
                return Ok(EvalValue::Null);
            }
            let (title, width, height, components, field_vals) =
                with_state(|s| (
                    s.title.clone(),
                    s.width,
                    s.height,
                    s.components.clone(),
                    s.field_vals.clone(),
                ));
            let path = get_script_path();
            // Si hay path, lanzar en modo reactivo (re-corre en cada evento)
            if !path.is_empty() {
                runner::launch_reactive(title, width, height, components, field_vals, path)?;
            } else {
                runner::launch(title, width, height, components, field_vals)?;
            }
            Ok(EvalValue::Null)
        }

        other => Err(format!("gui.{other} no existe")),
    }
}

/// Lanza el hot-reload GUI si el script tiene componentes. Devuelve true si era un GUI script.
/// Llamado desde cli/watch.rs después de la primera evaluación.
pub fn try_launch_watch(path: &str) -> bool {
    let (has_comps, title, w, h, comps, fields) = with_state(|s| (
        !s.components.is_empty(),
        s.title.clone(),
        s.width,
        s.height,
        s.components.clone(),
        s.field_vals.clone(),
    ));
    if !has_comps { return false; }
    let _ = runner::launch_watch(path, title, w, h, comps, fields);
    true
}

//     Helpers

fn push(c: Component) -> Result<EvalValue, String> {
    with_state(|s| s.components.push(c));
    Ok(EvalValue::Null)
}

fn str_arg(args: &[EvalValue], i: usize) -> Option<String> {
    args.get(i).map(|v| match v {
        EvalValue::Str(s) => s.clone(),
        other => format!("{other:?}"),
    })
}

fn req_str(args: &[EvalValue], i: usize, fn_name: &str) -> Result<String, String> {
    str_arg(args, i).ok_or_else(|| format!("gui.{fn_name} requiere un argumento de texto"))
}

fn f32_arg(args: &[EvalValue], i: usize) -> Option<f32> {
    args.get(i).and_then(|v| match v {
        EvalValue::Int(n)   => Some(*n as f32),
        EvalValue::Float(f) => Some(*f as f32),
        _ => None,
    })
}

/// Parsea "#RRGGBB" o nombre de color del design system de Orion GUI.
fn parse_color(s: &str) -> Option<[u8; 3]> {
    match s.to_lowercase().as_str() {
        // Paleta base Orion
        "accent"        => return Some([108, 99,  255]),
        "surface"       => return Some([26,  26,  40]),
        "bg"            => return Some([15,  15,  23]),
        // Semánticos
        "success"       => return Some([34,  197, 94]),
        "warning"       => return Some([234, 179, 8]),
        "error"         => return Some([239, 68,  68]),
        "info"          => return Some([59,  130, 246]),
        // Colores básicos
        "white"         => return Some([255, 255, 255]),
        "black"         => return Some([0,   0,   0]),
        "red"           => return Some([239, 68,  68]),
        "green"         => return Some([34,  197, 94]),
        "blue"          => return Some([59,  130, 246]),
        "yellow"        => return Some([234, 179, 8]),
        "orange"        => return Some([249, 115, 22]),
        "purple"        => return Some([168, 85,  247]),
        "pink"          => return Some([236, 72,  153]),
        "gray" | "grey" => return Some([107, 114, 128]),
        _ => {}
    }
    let s = s.trim().trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some([r, g, b])
    } else {
        None
    }
}

fn color_arg(args: &[EvalValue], i: usize) -> Option<[u8; 3]> {
    str_arg(args, i).and_then(|s| parse_color(&s))
}

fn style_args(args: &[EvalValue], bg_idx: usize, fg_idx: usize) -> Style {
    Style {
        bg: color_arg(args, bg_idx),
        fg: color_arg(args, fg_idx),
    }
}

// Decodifica "R,G,B" guardado en zone|... → Option<[u8;3]>
fn parse_rgb_tag(s: &str) -> Option<[u8; 3]> {
    let parts: Vec<u8> = s.split(',')
        .filter_map(|x| x.parse().ok())
        .collect();
    if parts.len() == 3 { Some([parts[0], parts[1], parts[2]]) } else { None }
}

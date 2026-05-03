pub mod components;
pub mod runner;
pub mod state;
pub mod theme;

use crate::eval_value::EvalValue;
use components::{Component, Style};
use state::with_state;

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

        //    Tipografía — gui.heading("texto", "color?", "colorTexto?")
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

        //    Display — gui.badge("texto", "bgColor?", "textColor?")
        "badge"   => push(Component::Badge(req_str(&args, 0, "badge")?, style_args(&args, 1, 2))),
        "divider" => push(Component::Divider),

        //    Lanzar ventana
        "run" => {
            let (title, width, height, components, field_vals) =
                with_state(|s| (
                    s.title.clone(),
                    s.width,
                    s.height,
                    s.components.clone(),
                    s.field_vals.clone(),
                ));
            runner::launch(title, width, height, components, field_vals)?;
            Ok(EvalValue::Null)
        }

        other => Err(format!("gui.{other} no existe")),
    }
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

/// Parsea un color desde un string: "#RRGGBB" o nombre ("accent", "red", etc.)
fn parse_color(s: &str) -> Option<[u8; 3]> {
    match s.to_lowercase().as_str() {
        "accent"        => return Some([108, 99,  255]),
        "surface"       => return Some([26,  26,  40]),
        "bg"            => return Some([15,  15,  23]),
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

/// Construye un Style leyendo bg en `bg_idx` y fg en `fg_idx`
fn style_args(args: &[EvalValue], bg_idx: usize, fg_idx: usize) -> Style {
    Style {
        bg: color_arg(args, bg_idx),
        fg: color_arg(args, fg_idx),
    }
}

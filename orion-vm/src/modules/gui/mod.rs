pub mod components;
pub mod runner;
pub mod state;
pub mod theme;

use crate::eval_value::EvalValue;
use components::Component;
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

        //    Tipografía                                                        
        "heading" => push(Component::Heading(req_str(&args, 0, "heading")?)),
        "text"    => push(Component::Text(req_str(&args, 0, "text")?)),
        "caption" => push(Component::Caption(req_str(&args, 0, "caption")?)),

        //    Inputs                                                            
        "field" => {
            let placeholder = str_arg(&args, 0).unwrap_or_default();
            let id = with_state(|s| format!("field_{}", s.components.len()));
            push(Component::Field { id, placeholder })
        }
        "toggle" => {
            let label = str_arg(&args, 0).unwrap_or_default();
            let id = with_state(|s| format!("toggle_{}", s.components.len()));
            push(Component::Toggle { id, label })
        }

        //    Acciones                                                          
        "press" => push(Component::Press(str_arg(&args, 0).unwrap_or_else(|| "OK".into()))),
        "ghost" => push(Component::Ghost(req_str(&args, 0, "ghost")?)),
        "tap"   => push(Component::Tap(req_str(&args, 0, "tap")?)),

        //    Display                                                           
        "badge"   => push(Component::Badge(req_str(&args, 0, "badge")?)),
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

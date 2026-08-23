pub mod runner;

use std::cell::RefCell;
use indexmap::IndexMap as HashMap;
use crate::eval_value::EvalValue;
use ratatui::style::Color;

//    Tipos                                                                   

#[derive(Clone, Default)]
pub struct TuiStyle {
    pub fg:   Option<Color>,
    pub bg:   Option<Color>,
    pub bold: bool,
}

#[derive(Clone)]
pub enum TuiWidget {
    //    Texto
    Text(String, TuiStyle),
    Heading(String, TuiStyle),
    Caption(String, TuiStyle),
    //    Datos
    Gauge   { label: String, percent: u16, style: TuiStyle },
    Items   { items: Vec<String>, style: TuiStyle },
    Grid    { headers: Vec<String>, rows: Vec<Vec<String>>, style: TuiStyle },
    Chart   { label: String, data: Vec<(String, u64)> },
    Spark   { data: Vec<u64> },
    //    Nav
    TuiTabs { labels: Vec<String>, selected: usize },
    //    Layout
    Divider,
    Spacer,
    Row(Vec<TuiWidget>),
    Col(Vec<TuiWidget>),
}

#[derive(Default, Clone)]
pub struct TuiState {
    pub title:           String,
    pub widgets:         Vec<TuiWidget>,
    pub container_stack: Vec<(String, usize)>,
    pub key_handlers:    Vec<(String, String)>,
    pub last_key:        String,
    pub state_store:     HashMap<String, EvalValue>,
}

thread_local! {
    static STATE: RefCell<TuiState> = RefCell::new(TuiState::default());
}

pub fn with_state<F, R>(f: F) -> R
where
    F: FnOnce(&mut TuiState) -> R,
{
    STATE.with(|s| f(&mut s.borrow_mut()))
}

//    Dispatcher                                                              

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // Las funciones de este módulo DECLARAN la interfaz; no dibujan nada
        // hasta que se llama a run(), que toma el control de la terminal.
        //
        // panel(titulo?: string) -> nada → título de la ventana; sin él, "Orion TUI"
        "panel" => {
            let title = str_arg(&args, 0).unwrap_or_else(|| "Orion TUI".into());
            with_state(|s| s.title = title);
            Ok(EvalValue::Null)
        }

        // text(texto: string, estilo?: dict) -> nada → una línea de texto normal
        "text"    => push(TuiWidget::Text(req_str(&args, 0, "text")?,    style_args(&args, 1, 2))),
        // heading(texto: string, estilo?: dict) -> nada → un título destacado
        "heading" => push(TuiWidget::Heading(req_str(&args, 0, "heading")?, style_args(&args, 1, 2))),
        // caption(texto: string, estilo?: dict) -> nada → texto secundario, más apagado
        "caption" => push(TuiWidget::Caption(req_str(&args, 0, "caption")?, style_args(&args, 1, 2))),

        // gauge(etiqueta: string, porcentaje: int, estilo?: dict) -> nada → barra de progreso; el porcentaje se recorta a 0-100
        "gauge" => {
            let label   = req_str(&args, 0, "gauge")?;
            let percent = i64_arg(&args, 1).unwrap_or(0).clamp(0, 100) as u16;
            let style   = style_args(&args, 2, 3);
            push(TuiWidget::Gauge { label, percent, style })
        }

        // list(elementos: list, estilo?: dict) -> nada → lista vertical; cada elemento se muestra como texto
        "list" => {
            let items = list_strings(&args, 0);
            let style = style_args(&args, 1, 2);
            push(TuiWidget::Items { items, style })
        }

        // table(cabeceras: list, filas: list, estilo?: dict) -> nada → tabla; cada fila es una lista de celdas
        "table" => {
            let headers = list_strings(&args, 0);
            let rows: Vec<Vec<String>> = match args.get(1) {
                Some(EvalValue::List(rs)) => rs.iter().map(|r| match r {
                    EvalValue::List(cols) => cols.iter().map(val_str).collect(),
                    other                 => vec![val_str(other)],
                }).collect(),
                _ => vec![],
            };
            let style = style_args(&args, 2, 3);
            push(TuiWidget::Grid { headers, rows, style })
        }

        // chart(etiqueta: string, datos: list) -> nada → barras a partir de pares [["nombre", valor], …]
        "chart" => {
            let label = req_str(&args, 0, "chart")?;
            // acepta lista de pares [["label", value], ...] o dict
            let data: Vec<(String, u64)> = match args.get(1) {
                Some(EvalValue::List(pairs)) => pairs.iter().filter_map(|p| {
                    if let EvalValue::List(kv) = p {
                        let k = kv.first().map(val_str).unwrap_or_default();
                        let v = kv.get(1).and_then(|v| match v {
                            EvalValue::Int(n)   => Some(*n as u64),
                            EvalValue::Float(f) => Some(*f as u64),
                            _ => None,
                        }).unwrap_or(0);
                        Some((k, v))
                    } else { None }
                }).collect(),
                _ => vec![],
            };
            push(TuiWidget::Chart { label, data })
        }

        // spark(datos: list) -> nada → minigráfico de una sola línea, para tendencias
        "spark" => {
            let data: Vec<u64> = match args.first() {
                Some(EvalValue::List(l)) => l.iter().map(|v| match v {
                    EvalValue::Int(n)   => *n as u64,
                    EvalValue::Float(f) => *f as u64,
                    _ => 0,
                }).collect(),
                _ => vec![],
            };
            push(TuiWidget::Spark { data })
        }

        // tabs(etiquetas: list, activa?: int) -> nada → pestañas; `activa` es el índice de la seleccionada
        "tabs" => {
            let labels   = list_strings(&args, 0);
            let selected = i64_arg(&args, 1).unwrap_or(0).max(0) as usize;
            push(TuiWidget::TuiTabs { labels, selected })
        }

        // divider() -> nada → línea horizontal de separación
        "divider" => push(TuiWidget::Divider),
        // spacer() -> nada → hueco vertical vacío
        "spacer"  => push(TuiWidget::Spacer),

        // row() -> nada → abre un contenedor horizontal; lo que declares después va dentro hasta el end()
        "row" => {
            with_state(|s| s.container_stack.push(("row".into(), s.widgets.len())));
            Ok(EvalValue::Null)
        }
        // col() -> nada → abre un contenedor vertical; se cierra con end()
        "col" => {
            with_state(|s| s.container_stack.push(("col".into(), s.widgets.len())));
            Ok(EvalValue::Null)
        }
        // end() -> nada → cierra el row() o col() abierto más recientemente
        "end" => {
            with_state(|s| {
                if let Some((kind, start)) = s.container_stack.pop() {
                    let children: Vec<TuiWidget> = s.widgets.drain(start..).collect();
                    let comp = if kind == "row" { TuiWidget::Row(children) } else { TuiWidget::Col(children) };
                    s.widgets.push(comp);
                }
            });
            Ok(EvalValue::Null)
        }

        // Eventos de teclado
        // on_key(tecla: string, evento: string) -> nada → asocia una tecla a un nombre de evento que luego lee key()
        "on_key" => {
            let key   = req_str(&args, 0, "on_key")?;
            let event = req_str(&args, 1, "on_key")?;
            with_state(|s| s.key_handlers.push((key, event)));
            Ok(EvalValue::Null)
        }
        // key() -> string → el último evento de tecla recibido, o cadena vacía si no hubo ninguno
        "key" => Ok(EvalValue::Str(with_state(|s| s.last_key.clone()))),

        // Estado persistente (igual que gui.val/gui.set)
        // val(clave: string, defecto?: any) -> any → lee del estado; devuelve `defecto` si esa clave no está
        "val" => {
            let key     = req_str(&args, 0, "val")?;
            let default = args.get(1).cloned().unwrap_or(EvalValue::Null);
            Ok(with_state(|s| s.state_store.get(&key).cloned().unwrap_or(default)))
        }
        // set(clave: string, valor: any) -> nada → guarda en el estado, que sobrevive a los redibujados
        "set" => {
            let key = req_str(&args, 0, "set")?;
            let val = args.get(1).cloned().unwrap_or(EvalValue::Null);
            with_state(|s| s.state_store.insert(key, val));
            Ok(EvalValue::Null)
        }

        // run() -> nada → toma la terminal y dibuja lo declarado. BLOQUEA hasta que el usuario sale
        "run" => {
            let (title, widgets, key_handlers) = with_state(|s| (
                s.title.clone(),
                s.widgets.clone(),
                s.key_handlers.clone(),
            ));
            runner::launch(title, widgets, key_handlers)
                .map(|_| EvalValue::Null)
        }

        other => Err(format!("tui.{other} does not exist")),
    }
}

//    Helpers                                                                 

fn push(w: TuiWidget) -> Result<EvalValue, String> {
    with_state(|s| s.widgets.push(w));
    Ok(EvalValue::Null)
}

fn val_str(v: &EvalValue) -> String {
    match v {
        EvalValue::Str(s)   => s.clone(),
        EvalValue::Int(n)   => n.to_string(),
        EvalValue::Float(f) => f.to_string(),
        EvalValue::Bool(b)  => b.to_string(),
        other               => format!("{other:?}"),
    }
}

fn str_arg(args: &[EvalValue], i: usize) -> Option<String> {
    args.get(i).map(val_str)
}

fn req_str(args: &[EvalValue], i: usize, name: &str) -> Result<String, String> {
    str_arg(args, i).ok_or_else(|| format!("tui.{name} requires a text argument"))
}

fn i64_arg(args: &[EvalValue], i: usize) -> Option<i64> {
    args.get(i).and_then(|v| match v {
        EvalValue::Int(n)   => Some(*n),
        EvalValue::Float(f) => Some(*f as i64),
        _ => None,
    })
}

fn list_strings(args: &[EvalValue], i: usize) -> Vec<String> {
    match args.get(i) {
        Some(EvalValue::List(l)) => l.iter().map(val_str).collect(),
        _ => vec![],
    }
}

/// Convierte un nombre de color a Color de ratatui.
/// Solo acepta nombres del sistema de color ANSI estándar (los define el terminal del usuario)
/// o códigos hex #RRGGBB. El desarrollador controla los colores de su app.
pub fn parse_color(s: &str) -> Option<Color> {
    match s.to_lowercase().as_str() {
        // Colores ANSI — el terminal del usuario decide el tono exacto
        "black"                  => Some(Color::Black),
        "red"                    => Some(Color::Red),
        "green"                  => Some(Color::Green),
        "yellow"                 => Some(Color::Yellow),
        "blue"                   => Some(Color::Blue),
        "magenta" | "purple"     => Some(Color::Magenta),
        "cyan"                   => Some(Color::Cyan),
        "white"                  => Some(Color::White),
        "gray" | "grey"          => Some(Color::Gray),
        "darkgray" | "dark_gray" => Some(Color::DarkGray),
        "lightred"   | "pink"    => Some(Color::LightRed),
        "lightgreen"             => Some(Color::LightGreen),
        "lightyellow"            => Some(Color::LightYellow),
        "lightblue"              => Some(Color::LightBlue),
        "lightmagenta"           => Some(Color::LightMagenta),
        "lightcyan"              => Some(Color::LightCyan),
        // Hex exacto — el dev elige el color preciso
        _ => {
            let s = s.trim().trim_start_matches('#');
            if s.len() == 6 {
                let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                Some(Color::Rgb(r, g, b))
            } else {
                None
            }
        }
    }
}

fn color_arg(args: &[EvalValue], i: usize) -> Option<Color> {
    str_arg(args, i).and_then(|s| parse_color(&s))
}

fn style_args(args: &[EvalValue], fg_idx: usize, bg_idx: usize) -> TuiStyle {
    TuiStyle {
        fg:   color_arg(args, fg_idx),
        bg:   color_arg(args, bg_idx),
        bold: false,
    }
}

pub mod components;
pub mod runner;
pub mod state;
pub mod theme;

use std::sync::atomic::Ordering;
use indexmap::IndexMap;
use crate::eval_value::EvalValue;
use components::{Component, Style, ChartKind, ChartConfig, Shape};
use state::{with_state, IS_WATCH_MODE, IS_REACTIVE_MODE, get_script_path};

/// Color de una forma de canvas: nombre/hex en args[pos], o el accent del tema.
fn shape_color(args: &[EvalValue], pos: usize) -> [u8; 3] {
    args.get(pos)
        .and_then(|v| match v { EvalValue::Str(s) => parse_color(s), _ => None })
        .unwrap_or_else(|| with_state(|s| s.theme.accent).unwrap_or([108, 99, 255]))
}

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

        //    Tema configurable — gui.theme({ accent, bg, surface, text,
        //    rounding, heading, body, spacing, light })  — el developer decide.
        "theme" => {
            let cfg = dict_opt(&args, 0);
            let tc = crate::modules::gui::state::ThemeConfig {
                accent:   cfg_color(&cfg, "accent"),
                bg:       cfg_color(&cfg, "bg").or_else(|| cfg_color(&cfg, "background")),
                surface:  cfg_color(&cfg, "surface").or_else(|| cfg_color(&cfg, "card")),
                text:     cfg_color(&cfg, "text").or_else(|| cfg_color(&cfg, "fg")),
                rounding: cfg_f32_opt(&cfg, "rounding").or_else(|| cfg_f32_opt(&cfg, "radius")),
                heading:  cfg_f32_opt(&cfg, "heading").or_else(|| cfg_f32_opt(&cfg, "heading_size")),
                body:     cfg_f32_opt(&cfg, "body").or_else(|| cfg_f32_opt(&cfg, "font_size")),
                spacing:  cfg_f32_opt(&cfg, "spacing"),
                light:    cfg_bool_opt(&cfg, "light").or_else(||
                              cfg_str(&cfg, "mode").map(|m| m.eq_ignore_ascii_case("light") || m.eq_ignore_ascii_case("claro"))),
            };
            with_state(|s| s.theme = tc);
            Ok(EvalValue::Null)
        }

        //    Tipografía — gui.heading("texto", "colorTexto?")
        "heading" => push(Component::Heading(req_str(&args, 0, "heading")?, text_style_args(&args, 1))),
        "text"    => push(Component::Text(req_str(&args, 0, "text")?, text_style_args(&args, 1))),
        "caption" => push(Component::Caption(req_str(&args, 0, "caption")?, text_style_args(&args, 1))),

        //    Inputs — gui.field("placeholder", "bgColor?", "textColor?")
        "field" => {
            let placeholder = str_arg(&args, 0).unwrap_or_default();
            let style = style_args(&args, 1, 2);
            // El dev puede fijar un id ESTABLE: gui.field("...", {"id":"nueva"}).
            // Con id explícito se lee fiable vía gui.value("nueva"); sin él cae a
            // uno posicional (frágil si cambia el layout).
            let id = cfg_str(&dict_opt(&args, 1), "id")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| with_state(|s| format!("field_{}", s.components.len())));
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

        //    Diálogos de archivo del sistema
        //
        //    Abren el selector nativo y bloquean hasta que el usuario responde,
        //    igual que cualquier app de escritorio. Devuelven la ruta elegida o
        //    null si se cancela — nunca un error, cancelar no es un fallo.

        // file_open(opts?) → ruta | null
        // opts = { title, filter, extensions, dir, multiple }
        "file_open" | "abrir_archivo" => {
            let cfg = dict_opt(&args, 0);
            let dialog = file_dialog(&cfg);
            if cfg_bool_opt(&cfg, "multiple").unwrap_or(false) {
                return Ok(match dialog.pick_files() {
                    Some(paths) => EvalValue::List(
                        paths.iter()
                            .map(|p| EvalValue::Str(p.display().to_string()))
                            .collect()
                    ),
                    None => EvalValue::List(vec![]),
                });
            }
            Ok(match dialog.pick_file() {
                Some(p) => EvalValue::Str(p.display().to_string()),
                None    => EvalValue::Null,
            })
        }

        // file_save(opts?) → ruta | null
        // opts = { title, filter, extensions, dir, name }
        "file_save" | "guardar_archivo" => {
            let cfg = dict_opt(&args, 0);
            let mut dialog = file_dialog(&cfg);
            if let Some(name) = cfg_str(&cfg, "name").or_else(|| cfg_str(&cfg, "nombre")) {
                dialog = dialog.set_file_name(name);
            }
            Ok(match dialog.save_file() {
                Some(p) => EvalValue::Str(p.display().to_string()),
                None    => EvalValue::Null,
            })
        }

        // folder_open(opts?) → ruta de carpeta | null
        "folder_open" | "abrir_carpeta" => {
            let cfg = dict_opt(&args, 0);
            Ok(match file_dialog(&cfg).pick_folder() {
                Some(p) => EvalValue::Str(p.display().to_string()),
                None    => EvalValue::Null,
            })
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

        //    Widgets nuevos
        // gui.progress(valor, "color?") — acepta 0..1 o 0..100 (se normaliza).
        "progress" => {
            let raw = f32_arg(&args, 0).unwrap_or(0.0);
            let value = if raw > 1.0 { raw / 100.0 } else { raw };
            push(Component::Progress { value, style: style_args(&args, 1, 2) })
        }
        // gui.tabs([labels], "activa?") — barra de pestañas; click dispara el label.
        "tabs" => {
            let labels: Vec<String> = match args.get(0) {
                Some(EvalValue::List(l)) => l.iter().map(|v| match v {
                    EvalValue::Str(s) => s.clone(),
                    other             => format!("{other}"),
                }).collect(),
                _ => vec![],
            };
            let active = str_arg(&args, 1)
                .or_else(|| labels.first().cloned())
                .unwrap_or_default();
            push(Component::Tabs { labels, active })
        }
        // gui.image("ruta", ancho?, alto?) — png/jpg/bmp/gif. Mantiene aspecto si
        // se da solo una dimensión.
        "image" | "img" => {
            let path = req_str(&args, 0, "image")?;
            push(Component::Image { path, width: f32_arg(&args, 1), height: f32_arg(&args, 2) })
        }
        // gui.modal("título") … gui.end() — diálogo centrado (contenedor).
        "modal" => {
            let title = str_arg(&args, 0).unwrap_or_else(|| "".into());
            with_state(|s| s.container_stack.push((format!("modal|{title}"), s.components.len())));
            Ok(EvalValue::Null)
        }

        //    Layout — containers anidados
        //    gui.card() / gui.row() / gui.col() / gui.zone() → abre el contenedor
        //    gui.end() → cierra el último contenedor abierto
        // gui.card({ width: N?, fill: bool? }) — config opcional.
        // Por defecto la tarjeta llena el ancho de su celda; el dev puede fijar
        // un width concreto o pedir fill:false para que se encoja al contenido.
        "card" => {
            let cfg   = dict_opt(&args, 0);
            let width = cfg_f32_opt(&cfg, "width");
            let fill  = cfg_bool(&cfg, "fill", true);
            let w_tag = width.map(|w| w.to_string()).unwrap_or_default();
            with_state(|s| s.container_stack.push(
                (format!("card|{}|{}", w_tag, fill), s.components.len())
            ));
            Ok(EvalValue::Null)
        }
        "row" => {
            with_state(|s| s.container_stack.push(("row".into(), s.components.len())));
            Ok(EvalValue::Null)
        }

        //    Composiciones de alto nivel
        //
        //    Son los dos arreglos que aparecen en casi cualquier panel y que a
        //    mano cuestan una veintena de líneas de row/col/card/end anidados.
        //    Emiten exactamente los mismos componentes que se escribirían a
        //    mano, así que el tema y el estilo siguen siendo los del developer.

        // gui.stats([{label, value, caption?}, …], opts?) — fila de tarjetas.
        // Cada item admite dict {label, value} o par ["label", "value"].
        // opts = { height, gap } y cualquier estilo de card.
        "stats" | "metricas" => {
            let items = list_arg(&args, 0, "stats")?;
            if items.is_empty() { return Ok(EvalValue::Null); }
            let cfg = dict_opt(&args, 1);
            let card_cfg = args.get(1).cloned().unwrap_or(EvalValue::Null);
            let _ = &cfg;

            call("row", vec![])?;
            for it in &items {
                let (label, value, extra) = tri_texto(it);
                call("col", vec![])?;
                if matches!(card_cfg, EvalValue::Dict(_)) {
                    call("card", vec![card_cfg.clone()])?;
                } else {
                    call("card", vec![])?;
                }
                call("caption", vec![EvalValue::Str(label)])?;
                call("heading", vec![EvalValue::Str(value)])?;
                if let Some(e) = extra {
                    call("caption", vec![EvalValue::Str(e)])?;
                }
                call("end", vec![])?;  // card
                call("end", vec![])?;  // col
            }
            call("end", vec![])       // row
        }

        // gui.header(titulo, subtitulo?, accion?) — encabezado de pantalla.
        // El título y su subtítulo a la izquierda, un botón opcional pegado a
        // la derecha. accion = { press | ghost, event? }.
        "header" | "encabezado" => {
            let titulo = req_str(&args, 0, "header")?;
            let sub    = args.get(1).and_then(|v| match v {
                EvalValue::Str(s) if !s.is_empty() => Some(s.clone()),
                _ => None,
            });
            let accion = args.iter().find_map(|a| match a {
                EvalValue::Dict(_) => Some(a.clone()),
                _ => None,
            });

            call("row", vec![])?;
            call("col", vec![])?;
            call("heading", vec![EvalValue::Str(titulo)])?;
            if let Some(s) = sub {
                call("caption", vec![EvalValue::Str(s)])?;
            }
            call("end", vec![])?;
            if let Some(a) = accion {
                call("col", vec![])?;
                boton_de(&a)?;
                call("end", vec![])?;
            }
            call("end", vec![])
        }

        // gui.section(titulo, accion?) … — card con cabecera y acción opcional.
        // Deja la card ABIERTA: el contenido va después y se cierra con `with`
        // o con gui.end(), igual que gui.card().
        "section" | "seccion" => {
            let titulo = req_str(&args, 0, "section")?;
            let accion = args.iter().skip(1).find_map(|a| match a {
                EvalValue::Dict(_) => Some(a.clone()),
                _ => None,
            });

            call("card", vec![])?;
            match accion {
                Some(a) => {
                    call("row", vec![])?;
                    call("col", vec![])?;
                    call("caption", vec![EvalValue::Str(titulo)])?;
                    call("end", vec![])?;
                    call("col", vec![])?;
                    boton_de(&a)?;
                    call("end", vec![])?;
                    call("end", vec![])?;
                    call("spacer", vec![EvalValue::Float(6.0)])?;
                }
                None => {
                    call("caption", vec![EvalValue::Str(titulo)])?;
                    call("spacer", vec![EvalValue::Float(4.0)])?;
                }
            }
            Ok(EvalValue::Null)
        }

        // gui.chips(lista, opts?) — fila de botones a partir de una lista.
        // opts = { event: "prefijo:" , style… }. El evento de cada botón es el
        // prefijo seguido del propio texto, que es el patrón con el que se
        // manejan listas dinámicas (`gui.ev()` empieza por el prefijo).
        "chips" | "opciones" => {
            let items = list_arg(&args, 0, "chips")?;
            if items.is_empty() { return Ok(EvalValue::Null); }
            let cfg    = dict_opt(&args, 1);
            let prefijo = cfg_str(&cfg, "event").or_else(|| cfg_str(&cfg, "ev")).unwrap_or_default();
            let solido = cfg_bool_opt(&cfg, "solid").unwrap_or(false);

            call("row", vec![])?;
            for it in &items {
                let etiqueta = texto(it);
                let mut estilo: IndexMap<String, EvalValue> = match &cfg {
                    Some(m) => m.clone(),
                    None    => IndexMap::new(),
                };
                estilo.insert("event".into(), EvalValue::Str(format!("{prefijo}{etiqueta}")));
                let args_btn = vec![EvalValue::Str(etiqueta), EvalValue::Dict(estilo)];
                if solido { call("press", args_btn)?; } else { call("ghost", args_btn)?; }
            }
            call("end", vec![])
        }

        // gui.fields([["Etiqueta", "valor"], …], opts?) — rejilla etiqueta/valor.
        // opts = { cols: 3, gap: 6 }. Con cols=1 sale una lista vertical.
        "fields" | "campos" => {
            let items = list_arg(&args, 0, "fields")?;
            if items.is_empty() { return Ok(EvalValue::Null); }
            let cfg  = dict_opt(&args, 1);
            let cols = cfg_f32_opt(&cfg, "cols").unwrap_or(3.0).max(1.0) as usize;
            let gap  = cfg_f32_opt(&cfg, "gap").unwrap_or(6.0);

            // Reparto por columnas: se llena la primera columna hasta agotar su
            // cuota antes de pasar a la siguiente, de modo que al leer en
            // vertical los campos salen en el orden en que se escribieron.
            let por_col = items.len().div_ceil(cols);

            call("row", vec![])?;
            for c in 0..cols {
                let desde = c * por_col;
                if desde >= items.len() { break; }
                let hasta = ((c + 1) * por_col).min(items.len());
                call("col", vec![])?;
                for (i, it) in items[desde..hasta].iter().enumerate() {
                    let (label, value, _) = tri_texto(it);
                    if i > 0 { call("spacer", vec![EvalValue::Float(gap as f64)])?; }
                    call("caption", vec![EvalValue::Str(label)])?;
                    call("text",    vec![EvalValue::Str(value)])?;
                }
                call("end", vec![])?;  // col
            }
            call("end", vec![])        // row
        }
        "col" => {
            with_state(|s| s.container_stack.push(("col".into(), s.components.len())));
            Ok(EvalValue::Null)
        }
        // gui.grid(cols) … gui.end() — rejilla de N columnas, los hijos se acomodan por filas.
        "grid" => {
            let cols = args.get(0).and_then(|v| v.to_i64().ok()).unwrap_or(2).max(1);
            with_state(|s| s.container_stack.push((format!("grid|{}", cols), s.components.len())));
            Ok(EvalValue::Null)
        }

        // ── Animación y dibujo libre ─────────────────────────────────────────
        // Primitivas genéricas: el motor aporta el reloj y el lienzo; qué se
        // anima o dibuja lo decide el developer en su script.

        // tick(ms) → dispara el evento "tick" cada ms milisegundos (re-corre el script)
        "tick" => {
            let ms = args.get(0).and_then(|v| v.to_i64().ok()).unwrap_or(0).max(0) as u32;
            state::TICK_MS.store(ms, Ordering::Relaxed);
            Ok(EvalValue::Null)
        }
        // canvas(width, height) … gui.end() → lienzo de dibujo 2D (coordenadas locales)
        "canvas" => {
            let w = f32_arg(&args, 0).unwrap_or(300.0);
            let h = f32_arg(&args, 1).unwrap_or(300.0);
            with_state(|s| s.container_stack.push((format!("canvas|{}|{}", w, h), s.components.len())));
            Ok(EvalValue::Null)
        }
        // circle(x, y, r, color?, fill?) → círculo; fill=no lo dibuja como contorno
        "circle" => {
            let color = shape_color(&args, 3);
            let fill  = args.get(4).map(|v| matches!(v, EvalValue::Bool(true))).unwrap_or(true);
            push(Component::Shape(Shape::Circle {
                x: f32_arg(&args, 0).unwrap_or(0.0),
                y: f32_arg(&args, 1).unwrap_or(0.0),
                r: f32_arg(&args, 2).unwrap_or(10.0),
                color, fill, stroke: 2.0,
            }))
        }
        // line(x1, y1, x2, y2, color?, width?) → segmento de línea
        "line" => {
            push(Component::Shape(Shape::Line {
                x1: f32_arg(&args, 0).unwrap_or(0.0),
                y1: f32_arg(&args, 1).unwrap_or(0.0),
                x2: f32_arg(&args, 2).unwrap_or(0.0),
                y2: f32_arg(&args, 3).unwrap_or(0.0),
                color: shape_color(&args, 4),
                width: f32_arg(&args, 5).unwrap_or(2.0),
            }))
        }
        // rect(x, y, w, h, color?, fill?) → rectángulo
        "rect" => {
            let color = shape_color(&args, 4);
            let fill  = args.get(5).map(|v| matches!(v, EvalValue::Bool(true))).unwrap_or(true);
            push(Component::Shape(Shape::RectS {
                x: f32_arg(&args, 0).unwrap_or(0.0),
                y: f32_arg(&args, 1).unwrap_or(0.0),
                w: f32_arg(&args, 2).unwrap_or(10.0),
                h: f32_arg(&args, 3).unwrap_or(10.0),
                color, fill, stroke: 2.0,
            }))
        }
        // arrow(x1, y1, x2, y2, color?, width?) → flecha con punta en (x2, y2)
        "arrow" => {
            push(Component::Shape(Shape::Arrow {
                x1: f32_arg(&args, 0).unwrap_or(0.0),
                y1: f32_arg(&args, 1).unwrap_or(0.0),
                x2: f32_arg(&args, 2).unwrap_or(0.0),
                y2: f32_arg(&args, 3).unwrap_or(0.0),
                color: shape_color(&args, 4),
                width: f32_arg(&args, 5).unwrap_or(2.5),
            }))
        }
        // text_at(x, y, texto, size?, color?) → texto centrado en (x, y)
        "text_at" => {
            push(Component::Shape(Shape::TextAt {
                x: f32_arg(&args, 0).unwrap_or(0.0),
                y: f32_arg(&args, 1).unwrap_or(0.0),
                text: str_arg(&args, 2).unwrap_or_default(),
                size: f32_arg(&args, 3).unwrap_or(13.0),
                color: shape_color(&args, 4),
            }))
        }
        // gui.sidebar(ancho?) … gui.end() — barra lateral fija (SidePanel izquierdo).
        // El ancho lo decide el dev; 220 es solo el fallback si no se indica.
        "sidebar" => {
            let width = f32_arg(&args, 0).unwrap_or(220.0);
            with_state(|s| s.container_stack.push((format!("sidebar|{}", width), s.components.len())));
            Ok(EvalValue::Null)
        }
        "zone" => {
            let style = style_args(&args, 0, 1);
            with_state(|s| {
                let idx = s.components.len();
                // Estilo completo codificado en el tag del contenedor:
                // zone|bg|fg|border|border_w|rounding|pad  (vacío = None).
                s.container_stack.push((format!("zone|{}|{}|{}|{}|{}|{}",
                    enc_rgb(style.bg), enc_rgb(style.fg), enc_rgb(style.border),
                    enc_f(style.border_w), enc_f(style.rounding), enc_f(style.pad),
                ), idx));
            });
            Ok(EvalValue::Null)
        }
        // free(handle) → cierra el contenedor abierto; lo llama `with`.
        //
        // `with c = gui.card() { … }` desugar a una llamada a `gui.free` al
        // salir del bloque, también si el cuerpo lanza un error. La pila de
        // contenedores es LIFO, así que el del tope es siempre el de este
        // bloque y el handle en sí no hace falta para nada.
        "free" => call("end", vec![]),

        "end" => {
            with_state(|s| {
                if let Some((kind, start)) = s.container_stack.pop() {
                    let children: Vec<Component> = s.components.drain(start..).collect();
                    let comp = if kind == "card" || kind.starts_with("card|") {
                        // "card|<width?>|<fill>" — width vacío = None
                        let parts: Vec<&str> = kind.splitn(3, '|').collect();
                        let width = parts.get(1).and_then(|s| s.parse::<f32>().ok());
                        let fill  = parts.get(2).map(|s| *s == "true").unwrap_or(true);
                        Component::Card { children, width, fill }
                    } else if kind == "row" {
                        Component::Row(children)
                    } else if kind == "col" {
                        Component::Col(children)
                    } else if kind.starts_with("grid|") {
                        let n = kind.strip_prefix("grid|")
                            .and_then(|s| s.parse::<usize>().ok())
                            .unwrap_or(2).max(1);
                        Component::Grid(n, children)
                    } else if kind.starts_with("sidebar|") {
                        let w = kind.strip_prefix("sidebar|")
                            .and_then(|s| s.parse::<f32>().ok())
                            .unwrap_or(220.0);
                        Component::Sidebar(w, children)
                    } else if kind.starts_with("zone|") {
                        let p: Vec<&str> = kind.split('|').collect();
                        let g = |i: usize| p.get(i).copied().unwrap_or("");
                        Component::Zone(children, Style {
                            bg:       parse_rgb_tag(g(1)),
                            fg:       parse_rgb_tag(g(2)),
                            border:   parse_rgb_tag(g(3)),
                            border_w: g(4).parse().ok(),
                            rounding: g(5).parse().ok(),
                            pad:      g(6).parse().ok(),
                            size:     None,
                            width:    None,
                            event:    None,
                        })
                    } else if kind.starts_with("fade|") {
                        // "fade|id|show"
                        let parts: Vec<&str> = kind.splitn(3, '|').collect();
                        let id   = parts.get(1).copied().unwrap_or("").to_string();
                        let show = parts.get(2).copied().unwrap_or("false") == "true";
                        Component::FadeGroup { id, show, children }
                    } else if kind.starts_with("slide_in|") {
                        let id = kind.strip_prefix("slide_in|").unwrap_or("").to_string();
                        Component::SlideIn { id, children }
                    } else if kind.starts_with("modal|") {
                        let title = kind.strip_prefix("modal|").unwrap_or("").to_string();
                        Component::Modal { title, children }
                    } else if kind.starts_with("canvas|") {
                        let p: Vec<&str> = kind.split('|').collect();
                        let w = p.get(1).and_then(|s| s.parse::<f32>().ok()).unwrap_or(300.0);
                        let h = p.get(2).and_then(|s| s.parse::<f32>().ok()).unwrap_or(300.0);
                        Component::Canvas { width: w, height: h, children }
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

        // gui.setval("id", texto) → fija el valor de un field (p.ej. limpiarlo
        // tras agregar una tarea). Escribe en field_vals, que el runner preserva
        // entre re-ejecuciones reactivas.
        "setval" | "set_field" => {
            let id  = req_str(&args, 0, "setval")?;
            let val = str_arg(&args, 1).unwrap_or_default();
            with_state(|s| { s.field_vals.insert(id, val); });
            Ok(EvalValue::Null)
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

        // ── Datos visuales ────────────────────────────────────────────────────

        // gui.table(datos, config?)
        // datos: lista de dicts  |  config: {cols:[..], height:300}
        "table" => {
            let rows_data = match args.get(0) {
                Some(EvalValue::List(l)) => l.clone(),
                _ => return Err("gui.table requires a list of dicts".into()),
            };
            let cfg = dict_opt(&args, 1);
            let height = cfg_f32(&cfg, "height", 300.0);
            let filter = cfg_list_str(&cfg, "cols");
            let (headers, rows) = extract_table(&rows_data, filter);
            push(Component::Table { headers, rows, height })
        }

        // gui.chart(datos, tipo, config?)
        // tipo: "bar"|"line"|"area"|"scatter"|"pie"|"hist"
        // config: {x:"col", y:"col", y2:"col", titulo:"", color:"", height:260, bins:10}
        "chart" => {
            let rows_data = match args.get(0) {
                Some(EvalValue::List(l)) => l.clone(),
                _ => return Err("gui.chart requires (datos, tipo, config?)".into()),
            };
            let kind_str = str_arg(&args, 1).unwrap_or_else(|| "bar".into());
            let kind = match kind_str.to_lowercase().as_str() {
                "line"    => ChartKind::Line,
                "area"    => ChartKind::Area,
                "scatter" => ChartKind::Scatter,
                "pie"     => ChartKind::Pie,
                "hist"    => ChartKind::Hist,
                _         => ChartKind::Bar,
            };

            let cfg    = dict_opt(&args, 2);
            let x_col  = cfg_str(&cfg, "x");
            let y_col  = cfg_str(&cfg, "y").or_else(|| cfg_str(&cfg, "value"));
            let y2_col = cfg_str(&cfg, "y2");
            let lbl    = cfg_str(&cfg, "label").or_else(|| x_col.clone());
            let titulo = cfg_str(&cfg, "titulo").or_else(|| cfg_str(&cfg, "title"));
            let height = cfg_f32(&cfg, "height", 260.0);
            let bins   = cfg_usize(&cfg, "bins", 10);

            // Etiquetas del eje X / segmentos de pie
            let labels: Vec<String> = rows_data.iter().map(|row| {
                match (row, &lbl) {
                    (EvalValue::Dict(m), Some(col)) =>
                        m.get(col.as_str()).map(eval_val_to_str).unwrap_or_default(),
                    _ => String::new(),
                }
            }).collect();

            let extract_vals = |col: &str| -> Vec<f64> {
                rows_data.iter().map(|row| match row {
                    EvalValue::Dict(m) => m.get(col).and_then(|v| match v {
                        EvalValue::Float(f) => Some(*f),
                        EvalValue::Int(n)   => Some(*n as f64),
                        EvalValue::Str(s)   => s.parse().ok(),
                        _                   => None,
                    }).unwrap_or(0.0),
                    _ => 0.0,
                }).collect()
            };

            // Valores X para scatter (columna x numérica)
            let xs: Vec<f64> = if kind == ChartKind::Scatter {
                x_col.as_deref().map(extract_vals).unwrap_or_default()
            } else {
                vec![]
            };

            // Series nombradas
            let mut series: Vec<(String, Vec<f64>)> = Vec::new();
            if let Some(col) = &y_col  { series.push((col.clone(), extract_vals(col))); }
            if let Some(col) = &y2_col { series.push((col.clone(), extract_vals(col))); }

            // Colores personalizados
            let palette: Vec<[u8; 3]> = [
                cfg_str(&cfg, "color").and_then(|s| parse_color(&s)),
                cfg_str(&cfg, "color2").and_then(|s| parse_color(&s)),
            ].into_iter().flatten().collect();

            let chart = ChartConfig { kind, title: titulo, height, labels, series, xs, bins, palette };
            push(Component::Chart(Box::new(chart)))
        }

        other => Err(format!("gui.{other} does not exist")),
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
    str_arg(args, i).ok_or_else(|| format!("gui.{fn_name} requires a text argument"))
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
        // Paleta del TEMA: estos nombres siguen lo que el developer fijó con
        // gui.theme({...}); si no fijó nada, caen al default. (No hardcodeados.)
        "accent"        => return Some(with_state(|s| s.theme.accent).unwrap_or([108, 99, 255])),
        "surface"       => return Some(with_state(|s| s.theme.surface).unwrap_or([26, 26, 40])),
        "bg"            => return Some(with_state(|s| s.theme.bg).unwrap_or([15, 15, 23])),
        "text"          => return Some(with_state(|s| s.theme.text).unwrap_or([235, 235, 245])),
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

/// Estilo para widgets con fondo (botones, cards, badges): color posicional = bg.
/// Si en la posición de estilo viene un DICT, se parsea estilo completo
/// (bg, fg, border, border_w, rounding, size, pad) — el developer decide.
fn style_args(args: &[EvalValue], bg_idx: usize, fg_idx: usize) -> Style {
    if let Some(EvalValue::Dict(m)) = args.get(bg_idx) {
        return parse_style_dict(m);
    }
    Style {
        bg: color_arg(args, bg_idx),
        fg: color_arg(args, fg_idx),
        ..Default::default()
    }
}

/// Estilo para widgets de TEXTO (heading/text/caption): color posicional = texto.
/// También acepta un dict de estilo completo.
fn text_style_args(args: &[EvalValue], idx: usize) -> Style {
    if let Some(EvalValue::Dict(m)) = args.get(idx) {
        return parse_style_dict(m);
    }
    Style { fg: color_arg(args, idx), ..Default::default() }
}

/// Parsea un dict de estilo completo. Acepta sinónimos para ser amable con el dev.
fn parse_style_dict(m: &indexmap::IndexMap<String, EvalValue>) -> Style {
    let cfg = Some(m.clone());
    Style {
        bg:       cfg_color(&cfg, "bg").or_else(|| cfg_color(&cfg, "fill")),
        fg:       cfg_color(&cfg, "fg").or_else(|| cfg_color(&cfg, "color")).or_else(|| cfg_color(&cfg, "text")),
        border:   cfg_color(&cfg, "border").or_else(|| cfg_color(&cfg, "stroke")),
        border_w: cfg_f32_opt(&cfg, "border_w").or_else(|| cfg_f32_opt(&cfg, "stroke_w")),
        rounding: cfg_f32_opt(&cfg, "rounding").or_else(|| cfg_f32_opt(&cfg, "radius")),
        size:     cfg_f32_opt(&cfg, "size").or_else(|| cfg_f32_opt(&cfg, "font_size")),
        pad:      cfg_f32_opt(&cfg, "pad").or_else(|| cfg_f32_opt(&cfg, "padding")),
        width:    cfg_f32_opt(&cfg, "width").or_else(|| cfg_f32_opt(&cfg, "w")),
        event:    cfg_str(&cfg, "event").or_else(|| cfg_str(&cfg, "ev")),
    }
}

// Decodifica "R,G,B" guardado en zone|... → Option<[u8;3]>
fn enc_rgb(c: Option<[u8; 3]>) -> String {
    c.map(|c| format!("{},{},{}", c[0], c[1], c[2])).unwrap_or_default()
}
fn enc_f(v: Option<f32>) -> String {
    v.map(|v| v.to_string()).unwrap_or_default()
}

fn parse_rgb_tag(s: &str) -> Option<[u8; 3]> {
    let parts: Vec<u8> = s.split(',')
        .filter_map(|x| x.parse().ok())
        .collect();
    if parts.len() == 3 { Some([parts[0], parts[1], parts[2]]) } else { None }
}

// ── Helpers para gui.table / gui.chart ──────────────────────────────────────

type CfgMap = Option<indexmap::IndexMap<String, EvalValue>>;

/// Emite el botón descrito por `{ press | ghost, event? }`.
///
/// Lo comparten `header` y `section`: en ambos la acción es un único botón
/// alineado a la derecha, y el resto de claves del dict pasan como estilo.
fn boton_de(accion: &EvalValue) -> Result<EvalValue, String> {
    let EvalValue::Dict(m) = accion else { return Ok(EvalValue::Null) };
    let (etiqueta, solido) = match m.get("press").or_else(|| m.get("boton")) {
        Some(v) => (texto(v), true),
        None => match m.get("ghost") {
            Some(v) => (texto(v), false),
            None    => return Ok(EvalValue::Null),
        },
    };
    let args = vec![EvalValue::Str(etiqueta), EvalValue::Dict(m.clone())];
    if solido { call("press", args) } else { call("ghost", args) }
}

/// Lista obligatoria en la posición `i`.
fn list_arg(args: &[EvalValue], i: usize, fn_name: &str) -> Result<Vec<EvalValue>, String> {
    match args.get(i) {
        Some(EvalValue::List(l)) => Ok(l.clone()),
        _ => Err(format!("gui.{fn_name} requires a list as its first argument")),
    }
}

/// Texto plano de un valor, sin la representación de depuración.
fn texto(v: &EvalValue) -> String {
    match v {
        EvalValue::Str(s) => s.clone(),
        other             => format!("{other}"),
    }
}

/// (etiqueta, valor, extra) de un item de `stats`/`fields`.
///
/// Se aceptan las dos formas que resultan naturales al escribir un panel: el
/// par posicional `["Etiqueta", valor]` y el dict `{label, value}`. En el dict
/// se admiten alias en español y las claves `Campo`/`Valor`, que son las que
/// produce un reporte leído de Excel.
fn tri_texto(v: &EvalValue) -> (String, String, Option<String>) {
    match v {
        EvalValue::List(l) => (
            l.first().map(texto).unwrap_or_default(),
            l.get(1).map(texto).unwrap_or_default(),
            l.get(2).map(texto),
        ),
        EvalValue::Dict(m) => {
            let pick = |keys: &[&str]| -> Option<String> {
                keys.iter().find_map(|k| m.get(*k).map(texto))
            };
            (
                pick(&["label", "etiqueta", "Campo", "campo", "name", "nombre"]).unwrap_or_default(),
                pick(&["value", "valor", "Valor"]).unwrap_or_default(),
                pick(&["caption", "nota", "hint"]),
            )
        }
        other => (texto(other), String::new(), None),
    }
}

/// Construye el diálogo nativo a partir de las opciones del developer.
///
/// `extensions` acepta una lista (`["xlsx", "xls"]`) o una cadena suelta; el
/// punto inicial es opcional para que dé igual escribir "xlsx" o ".xlsx".
fn file_dialog(cfg: &CfgMap) -> rfd::FileDialog {
    let mut d = rfd::FileDialog::new();

    if let Some(t) = cfg_str(cfg, "title").or_else(|| cfg_str(cfg, "titulo")) {
        d = d.set_title(t);
    }
    if let Some(dir) = cfg_str(cfg, "dir").or_else(|| cfg_str(cfg, "carpeta")) {
        d = d.set_directory(dir);
    }

    let exts: Vec<String> = match cfg.as_ref()
        .and_then(|m| m.get("extensions").or_else(|| m.get("extensiones")))
    {
        Some(EvalValue::List(l)) => l.iter().filter_map(|v| match v {
            EvalValue::Str(s) => Some(s.trim_start_matches('.').to_string()),
            _ => None,
        }).collect(),
        Some(EvalValue::Str(s)) => vec![s.trim_start_matches('.').to_string()],
        _ => Vec::new(),
    };
    if !exts.is_empty() {
        let label = cfg_str(cfg, "filter")
            .or_else(|| cfg_str(cfg, "filtro"))
            .unwrap_or_else(|| "Files".into());
        let refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
        d = d.add_filter(label, &refs);
    }
    d
}

fn dict_opt(args: &[EvalValue], i: usize) -> CfgMap {
    match args.get(i) {
        Some(EvalValue::Dict(m)) => Some(m.clone()),
        _ => None,
    }
}

fn cfg_str(cfg: &CfgMap, key: &str) -> Option<String> {
    cfg.as_ref()?.get(key).and_then(|v| match v {
        EvalValue::Str(s) => Some(s.clone()),
        _ => None,
    })
}

fn cfg_f32(cfg: &CfgMap, key: &str, default: f32) -> f32 {
    cfg.as_ref()
        .and_then(|m| m.get(key))
        .and_then(|v| match v {
            EvalValue::Float(f) => Some(*f as f32),
            EvalValue::Int(n)   => Some(*n as f32),
            _ => None,
        })
        .unwrap_or(default)
}

fn cfg_f32_opt(cfg: &CfgMap, key: &str) -> Option<f32> {
    cfg.as_ref()?.get(key).and_then(|v| match v {
        EvalValue::Float(f) => Some(*f as f32),
        EvalValue::Int(n)   => Some(*n as f32),
        _ => None,
    })
}

fn cfg_color(cfg: &CfgMap, key: &str) -> Option<[u8; 3]> {
    cfg_str(cfg, key).and_then(|s| parse_color(&s))
}

fn cfg_bool_opt(cfg: &CfgMap, key: &str) -> Option<bool> {
    cfg.as_ref()?.get(key).and_then(|v| match v {
        EvalValue::Bool(b) => Some(*b),
        EvalValue::Int(n)  => Some(*n != 0),
        _ => None,
    })
}

fn cfg_bool(cfg: &CfgMap, key: &str, default: bool) -> bool {
    cfg.as_ref()
        .and_then(|m| m.get(key))
        .and_then(|v| match v {
            EvalValue::Bool(b) => Some(*b),
            EvalValue::Int(n)  => Some(*n != 0),
            _ => None,
        })
        .unwrap_or(default)
}

fn cfg_usize(cfg: &CfgMap, key: &str, default: usize) -> usize {
    cfg.as_ref()
        .and_then(|m| m.get(key))
        .and_then(|v| match v {
            EvalValue::Int(n)   => Some(*n as usize),
            EvalValue::Float(f) => Some(*f as usize),
            _ => None,
        })
        .unwrap_or(default)
}

fn cfg_list_str(cfg: &CfgMap, key: &str) -> Option<Vec<String>> {
    cfg.as_ref()?.get(key).and_then(|v| match v {
        EvalValue::List(l) => Some(l.iter().map(eval_val_to_str).collect()),
        _ => None,
    })
}

fn eval_val_to_str(v: &EvalValue) -> String {
    match v {
        EvalValue::Str(s)   => s.clone(),
        EvalValue::Int(n)   => n.to_string(),
        EvalValue::Float(f) => format!("{:.2}", f),
        EvalValue::Bool(b)  => b.to_string(),
        EvalValue::Null     => String::new(),
        other               => format!("{other:?}"),
    }
}

fn extract_table(
    rows: &[EvalValue],
    filter: Option<Vec<String>>,
) -> (Vec<String>, Vec<Vec<String>>) {
    let headers: Vec<String> = match rows.first() {
        Some(EvalValue::Dict(m)) => {
            let all: Vec<String> = m.keys().cloned().collect();
            filter.unwrap_or(all)
        }
        _ => return (vec![], vec![]),
    };
    let data: Vec<Vec<String>> = rows.iter().map(|row| match row {
        EvalValue::Dict(m) => headers.iter().map(|h| {
            m.get(h.as_str()).map(eval_val_to_str).unwrap_or_default()
        }).collect(),
        _ => headers.iter().map(|_| String::new()).collect(),
    }).collect();
    (headers, data)
}

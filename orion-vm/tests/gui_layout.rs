//! Composiciones de alto nivel de la GUI: `gui.stats`, `gui.fields` y el cierre
//! de contenedores con `with`.
//!
//! Todas emiten los mismos componentes que se escribirían a mano con
//! row/col/card/end, así que lo que se verifica aquí es justamente eso: que el
//! árbol resultante sea el esperado y quede bien cerrado.

use orion_vm::eval_value::EvalValue;
use orion_vm::modules::gui::components::Component;
use orion_vm::modules::gui::state::with_state;
use orion_vm::{codegen, lexer, parser, vm};

/// Ejecuta un script de GUI y devuelve los componentes que produjo.
fn render(src: &str) -> Vec<Component> {
    let tokens = lexer::lex(src).expect("lex");
    let stmts = parser::parse(tokens).expect("parse");
    let bc = codegen::compile(stmts).expect("codegen");

    with_state(|s| {
        s.components.clear();
        s.container_stack.clear();
    });

    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    machine.run().expect("vm run");

    with_state(|s| {
        // Un contenedor sin cerrar dejaría componentes colgando en la pila:
        // que esté vacía es parte de lo que se comprueba.
        assert!(s.container_stack.is_empty(), "quedaron contenedores sin cerrar");
        s.components.clone()
    })
}

fn textos(c: &Component, out: &mut Vec<String>) {
    match c {
        Component::Heading(t, _) => out.push(format!("h:{t}")),
        Component::Text(t, _)    => out.push(format!("t:{t}")),
        Component::Caption(t, _) => out.push(format!("c:{t}")),
        Component::Row(hijos) | Component::Col(hijos) => {
            for h in hijos { textos(h, out); }
        }
        Component::Card { children, .. } => {
            for h in children { textos(h, out); }
        }
        _ => {}
    }
}

fn aplanar(comps: &[Component]) -> Vec<String> {
    let mut out = Vec::new();
    for c in comps { textos(c, &mut out); }
    out
}

/// Descripción del árbol: tipo de contenedor, anidamiento y textos.
/// `Component` no implementa Debug, así que esto hace de forma canónica para
/// comparar dos layouts.
fn forma(c: &Component) -> String {
    match c {
        Component::Row(h)  => format!("Row[{}]", h.iter().map(forma).collect::<Vec<_>>().join(",")),
        Component::Col(h)  => format!("Col[{}]", h.iter().map(forma).collect::<Vec<_>>().join(",")),
        Component::Card { children, width, fill } =>
            format!("Card(w={width:?},fill={fill})[{}]",
                    children.iter().map(forma).collect::<Vec<_>>().join(",")),
        Component::Heading(t, _) => format!("H({t})"),
        Component::Text(t, _)    => format!("T({t})"),
        Component::Caption(t, _) => format!("C({t})"),
        Component::Spacer(n)     => format!("S({n})"),
        _ => "otro".into(),
    }
}

fn formas(comps: &[Component]) -> String {
    comps.iter().map(forma).collect::<Vec<_>>().join(",")
}

//    gui.stats

#[test]
fn stats_genera_una_card_por_metrica_dentro_de_una_row() {
    let comps = render(
        r#"use "gui" as gui
gui.stats([
    { "label": "CERTIFICADOS", "value": "15" },
    { "label": "SUMA",         "value": "278,108.30" }
])"#,
    );

    assert_eq!(comps.len(), 1, "stats debe producir UNA fila");
    let Component::Row(cols) = &comps[0] else { panic!("se esperaba Row, hubo {}", forma(&comps[0])) };
    assert_eq!(cols.len(), 2, "una columna por métrica");

    // Cada columna lleva una card con la etiqueta y el valor.
    for col in cols {
        let Component::Col(hijos) = col else { panic!("se esperaba Col") };
        assert_eq!(hijos.len(), 1);
        assert!(matches!(hijos[0], Component::Card { .. }), "cada métrica va en una card");
    }

    assert_eq!(
        aplanar(&comps),
        vec!["c:CERTIFICADOS", "h:15", "c:SUMA", "h:278,108.30"]
    );
}

#[test]
fn stats_acepta_pares_posicionales() {
    let comps = render(
        r#"use "gui" as gui
gui.stats([ ["A", "1"], ["B", "2"] ])"#,
    );
    assert_eq!(aplanar(&comps), vec!["c:A", "h:1", "c:B", "h:2"]);
}

#[test]
fn stats_admite_una_nota_como_tercer_elemento() {
    let comps = render(
        r#"use "gui" as gui
gui.stats([ ["TOTAL", "42", "en lo que va de mes"] ])"#,
    );
    assert_eq!(aplanar(&comps), vec!["c:TOTAL", "h:42", "c:en lo que va de mes"]);
}

#[test]
fn stats_con_lista_vacia_no_emite_nada() {
    assert!(render("use \"gui\" as gui\ngui.stats([])").is_empty());
}

//    gui.fields

#[test]
fn fields_reparte_en_columnas_leyendo_en_vertical() {
    // 6 campos en 3 columnas → 2 por columna, y el orden de lectura vertical
    // debe coincidir con el orden en que se escribieron.
    let comps = render(
        r#"use "gui" as gui
gui.fields([
    ["A", "1"], ["B", "2"], ["C", "3"],
    ["D", "4"], ["E", "5"], ["F", "6"]
], { "cols": 3 })"#,
    );

    assert_eq!(comps.len(), 1);
    let Component::Row(cols) = &comps[0] else { panic!("se esperaba Row") };
    assert_eq!(cols.len(), 3, "tres columnas");

    assert_eq!(
        aplanar(&comps),
        vec!["c:A", "t:1", "c:B", "t:2",
             "c:C", "t:3", "c:D", "t:4",
             "c:E", "t:5", "c:F", "t:6"]
    );
}

#[test]
fn fields_con_una_columna_es_una_lista_vertical() {
    let comps = render(
        r#"use "gui" as gui
gui.fields([ ["A", "1"], ["B", "2"] ], { "cols": 1 })"#,
    );
    let Component::Row(cols) = &comps[0] else { panic!("se esperaba Row") };
    assert_eq!(cols.len(), 1);
    assert_eq!(aplanar(&comps), vec!["c:A", "t:1", "c:B", "t:2"]);
}

#[test]
fn fields_no_deja_columnas_vacias_si_sobran() {
    // 2 campos en 5 columnas: solo deben crearse las columnas con contenido.
    let comps = render(
        r#"use "gui" as gui
gui.fields([ ["A", "1"], ["B", "2"] ], { "cols": 5 })"#,
    );
    let Component::Row(cols) = &comps[0] else { panic!("se esperaba Row") };
    assert_eq!(cols.len(), 2, "sin columnas vacías de relleno");
}

#[test]
fn fields_acepta_las_claves_de_un_reporte_de_excel() {
    // {Campo, Valor} es lo que sale de leer una hoja, y debe servir tal cual.
    let comps = render(
        r#"use "gui" as gui
gui.fields([
    { "Campo": "Operación", "Valor": "913916" }
], { "cols": 1 })"#,
    );
    assert_eq!(aplanar(&comps), vec!["c:Operación", "t:913916"]);
}

//    with sobre contenedores

#[test]
fn with_cierra_el_contenedor_al_salir() {
    let comps = render(
        r#"use "gui" as gui
with c = gui.card() {
    gui.text("dentro")
}
gui.text("fuera")"#,
    );

    assert_eq!(comps.len(), 2, "la card y el texto de fuera");
    let Component::Card { children, .. } = &comps[0] else { panic!("se esperaba Card") };
    assert_eq!(aplanar(children), vec!["t:dentro"]);
    assert!(matches!(comps[1], Component::Text(_, _)), "el segundo texto queda fuera");
}

#[test]
fn with_anidado_cierra_en_el_orden_correcto() {
    let comps = render(
        r#"use "gui" as gui
with r = gui.row() {
    with c = gui.col() {
        gui.text("interior")
    }
}"#,
    );
    assert_eq!(comps.len(), 1);
    let Component::Row(hijos) = &comps[0] else { panic!("se esperaba Row") };
    assert_eq!(hijos.len(), 1);
    assert!(matches!(hijos[0], Component::Col(_)), "la col queda dentro de la row");
    assert_eq!(aplanar(&comps), vec!["t:interior"]);
}

//    Equivalencia con la forma manual

#[test]
fn stats_equivale_a_escribirlo_a_mano() {
    let manual = render(
        r#"use "gui" as gui
gui.row()
  gui.col()
    gui.card()
      gui.caption("A")
      gui.heading("1")
    gui.end()
  gui.end()
  gui.col()
    gui.card()
      gui.caption("B")
      gui.heading("2")
    gui.end()
  gui.end()
gui.end()"#,
    );
    let helper = render(
        r#"use "gui" as gui
gui.stats([ ["A", "1"], ["B", "2"] ])"#,
    );
    assert_eq!(formas(&manual), formas(&helper));
}

#[test]
fn el_valor_no_tiene_que_ser_texto() {
    // Números y booleanos se muestran como los imprimiría `show`, no en su
    // representación de depuración.
    let comps = render(
        r#"use "gui" as gui
gui.stats([ ["N", 42], ["F", 3.5] ])"#,
    );
    assert_eq!(aplanar(&comps), vec!["c:N", "h:42", "c:F", "h:3.5"]);
}

#[test]
fn stats_rechaza_un_argumento_que_no_sea_lista() {
    let src = "use \"gui\" as gui\ngui.stats(\"no soy lista\")";
    let tokens = lexer::lex(src).expect("lex");
    let stmts = parser::parse(tokens).expect("parse");
    let bc = codegen::compile(stmts).expect("codegen");
    with_state(|s| { s.components.clear(); s.container_stack.clear(); });
    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    let err = machine.run().expect_err("debe fallar con un argumento que no es lista");
    assert!(err.contains("lista"), "el error debe explicar qué se esperaba: {err}");
    let _ = EvalValue::Null;
}

//    gui.header

#[test]
fn header_pone_titulo_y_accion_en_columnas_opuestas() {
    let comps = render(
        r#"use "gui" as gui
gui.header("Reporte", "12 filas", { "press": "Examinar…", "event": "examinar" })"#,
    );
    assert_eq!(comps.len(), 1);
    let Component::Row(cols) = &comps[0] else { panic!("se esperaba Row") };
    assert_eq!(cols.len(), 2, "título a un lado, acción al otro");
    assert_eq!(aplanar(&comps), vec!["h:Reporte", "c:12 filas"]);
}

#[test]
fn header_sin_accion_no_crea_la_segunda_columna() {
    let comps = render(
        r#"use "gui" as gui
gui.header("Solo título")"#,
    );
    let Component::Row(cols) = &comps[0] else { panic!("se esperaba Row") };
    assert_eq!(cols.len(), 1);
    assert_eq!(aplanar(&comps), vec!["h:Solo título"]);
}

#[test]
fn header_sin_subtitulo_no_emite_caption() {
    let comps = render(
        r#"use "gui" as gui
gui.header("Título", "")"#,
    );
    assert_eq!(aplanar(&comps), vec!["h:Título"]);
}

//    gui.section

#[test]
fn section_abre_una_card_que_cierra_el_with() {
    let comps = render(
        r#"use "gui" as gui
with s = gui.section("CERTIFICADOS") {
    gui.text("contenido")
}
gui.text("fuera")"#,
    );
    assert_eq!(comps.len(), 2);
    let Component::Card { children, .. } = &comps[0] else { panic!("se esperaba Card") };
    assert_eq!(aplanar(children), vec!["c:CERTIFICADOS", "t:contenido"]);
}

#[test]
fn section_con_accion_pone_el_boton_junto_al_titulo() {
    let comps = render(
        r#"use "gui" as gui
with s = gui.section("CERTIFICADOS", { "press": "Exportar", "event": "exportar" }) {
    gui.text("tabla")
}"#,
    );
    let Component::Card { children, .. } = &comps[0] else { panic!("se esperaba Card") };
    // La cabecera es una fila de dos columnas; el contenido va después.
    assert!(matches!(children[0], Component::Row(_)), "cabecera en fila");
    assert_eq!(aplanar(children), vec!["c:CERTIFICADOS", "t:tabla"]);
}

//    gui.chips

#[test]
fn chips_genera_un_boton_por_elemento_con_evento_prefijado() {
    let comps = render(
        r#"use "gui" as gui
gui.chips(["913916", "74979152"], { "event": "sug:" })"#,
    );
    assert_eq!(comps.len(), 1);
    let Component::Row(hijos) = &comps[0] else { panic!("se esperaba Row") };
    assert_eq!(hijos.len(), 2, "un botón por elemento");
    // El evento de cada botón es prefijo + etiqueta.
    for (h, esperado) in hijos.iter().zip(["sug:913916", "sug:74979152"]) {
        let Component::Ghost(txt, style) = h else { panic!("se esperaba Ghost") };
        assert_eq!(style.event.as_deref(), Some(esperado), "evento de '{txt}'");
    }
}

#[test]
fn chips_con_lista_vacia_no_emite_nada() {
    assert!(render("use \"gui\" as gui\ngui.chips([])").is_empty());
}

#[test]
fn chips_acepta_numeros() {
    let comps = render(
        r#"use "gui" as gui
gui.chips([913916, 70063888], { "event": "op:" })"#,
    );
    let Component::Row(hijos) = &comps[0] else { panic!("se esperaba Row") };
    let Component::Ghost(txt, style) = &hijos[0] else { panic!("se esperaba Ghost") };
    assert_eq!(txt, "913916");
    assert_eq!(style.event.as_deref(), Some("op:913916"));
}

//    Equivalencia con la forma manual

#[test]
fn header_equivale_a_escribirlo_a_mano() {
    let manual = render(
        r#"use "gui" as gui
gui.row()
  gui.col()
    gui.heading("Reporte")
    gui.caption("12 filas")
  gui.end()
  gui.col()
    gui.press("Examinar…", { "event": "examinar" })
  gui.end()
gui.end()"#,
    );
    let helper = render(
        r#"use "gui" as gui
gui.header("Reporte", "12 filas", { "press": "Examinar…", "event": "examinar" })"#,
    );
    assert_eq!(formas(&manual), formas(&helper));
}

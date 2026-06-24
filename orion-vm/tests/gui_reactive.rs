// Verifica que el estado reactivo de la GUI (gui.set/gui.val sobre state_store)
// PERSISTE entre re-ejecuciones del script cuando estas corren en el mismo hilo.
//
// El bug: la re-evaluación se lanzaba en un thread::spawn nuevo, y STATE es
// thread_local, así que cada evento arrancaba con un state_store vacío y el
// estado (acc/op/cur de la calculadora) se perdía en cada clic.
//
// Este test replica el flujo de runner::rerun_script disparando una secuencia
// de eventos en el hilo de test y comprobando que el display acumula.

use std::sync::atomic::Ordering;

use orion_vm::eval_value::EvalValue;
use orion_vm::modules::gui::components::Component;
use orion_vm::modules::gui::state::{with_state, IS_REACTIVE_MODE};
use orion_vm::modules::gui::call as gui_call;
use orion_vm::{codegen, lexer, parser, vm};

/// Re-ejecuta `src` inyectando `event` como último evento de UI, igual que el
/// runner reactivo. Corre en el hilo actual → el state_store persiste entre
/// llamadas. Devuelve el valor de `disp` en el state_store tras la corrida.
fn fire(src: &str, event: &str) -> String {
    let tokens = lexer::lex(src).expect("lex");
    let stmts = parser::parse(tokens).expect("parse");
    let bc = codegen::compile(stmts).expect("codegen");

    with_state(|s| {
        s.components.clear();
        s.container_stack.clear();
        s.last_event = event.to_string();
        s.is_reactive = true;
    });
    IS_REACTIVE_MODE.store(true, Ordering::Relaxed);

    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    machine.run().expect("vm run");

    IS_REACTIVE_MODE.store(false, Ordering::Relaxed);

    with_state(|s| match s.state_store.get("disp") {
        Some(orion_vm::eval_value::EvalValue::Str(v)) => v.clone(),
        other => format!("{other:?}"),
    })
}

// Un único test: la GUI usa estado global de proceso (IS_REACTIVE_MODE), así que
// los casos no pueden correr en paralelo sin pisarse (uno apagaría el flag mientras
// el otro ejecuta gui.run() y este intentaría abrir la ventana real). Mantenerlos
// en una sola función garantiza ejecución secuencial en el mismo hilo.
#[test]
fn calc_reactive_state() {
    let src = include_str!("../../demo/demo_calc.orx");

    // Pulsar 7, luego 8: si el estado persiste, el display acumula a "78".
    // Con el bug (state_store nuevo por evento) daría "8".
    assert_eq!(fire(src, "7"), "7", "primer dígito");
    assert_eq!(fire(src, "8"), "78", "segundo dígito debe acumular (no reiniciar)");
    assert_eq!(fire(src, "9"), "789", "tercer dígito debe acumular");

    // Operador: cierra el operando (acc=789) y deja el display mostrando acc.
    let after_plus = fire(src, "+");
    assert!(
        after_plus.starts_with("789"),
        "tras '+', display muestra acc=789, fue {after_plus:?}"
    );

    // Segundo operando y resultado: 789 + 10 = 799.
    assert_eq!(fire(src, "1"), "1", "nuevo operando reinicia el input actual");
    assert_eq!(fire(src, "0"), "10", "segundo dígito del operando acumula");

    let result = fire(src, "=");
    assert!(
        result.starts_with("799"),
        "789 + 10 debe dar 799, display fue {result:?}"
    );

    // C limpia todo el estado.
    assert_eq!(fire(src, "C"), "0", "C reinicia el display a 0");

    // Contraste — modelo VIEJO (bug): cada evento corría en un thread::spawn
    // nuevo, y como STATE es thread_local el state_store nacía vacío en cada
    // clic. Lo emulamos limpiando el state_store antes de cada evento: sin
    // persistencia el display NO acumula, exactamente el "se borra" reportado.
    let fire_fresh = |event: &str| -> String {
        with_state(|s| s.state_store.clear()); // simula state_store de hilo nuevo
        fire(src, event)
    };
    assert_eq!(fire_fresh("7"), "7");
    assert_eq!(
        fire_fresh("8"),
        "8",
        "sin state_store persistente el estado se pierde en cada clic"
    );
}

/// Construye una tarjeta vía gui.call(...) y devuelve su (width, fill).
fn build_card(cfg: Option<EvalValue>) -> (Option<f32>, bool) {
    with_state(|s| {
        s.components.clear();
        s.container_stack.clear();
    });
    let args = cfg.map(|c| vec![c]).unwrap_or_default();
    gui_call("card", args).expect("card");
    gui_call("heading", vec![EvalValue::Str("x".into())]).expect("heading");
    gui_call("end", vec![]).expect("end");
    with_state(|s| match s.components.last() {
        Some(Component::Card { width, fill, .. }) => (*width, *fill),
        _ => panic!("se esperaba un Component::Card"),
    })
}

fn dict(pairs: &[(&str, EvalValue)]) -> EvalValue {
    EvalValue::Dict(pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}

#[test]
fn card_width_config_is_developer_controlled() {
    // Default: sin config → llena el ancho (fill=true, sin width fijo).
    assert_eq!(build_card(None), (None, true), "default = fill");

    // Ancho fijo: el dev manda con { width: 280 }.
    assert_eq!(
        build_card(Some(dict(&[("width", EvalValue::Int(280))]))),
        (Some(280.0), true),
        "width fijo lo decide el dev"
    );

    // Encoger al contenido: { fill: false }.
    assert_eq!(
        build_card(Some(dict(&[("fill", EvalValue::Bool(false))]))),
        (None, false),
        "fill:false = se encoge al contenido"
    );
}

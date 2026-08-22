//! La tabla de alias obsoletos tiene que seguir describiendo al registro.
//!
//! `src/deprecated.rs` es una lista CURADA a mano, no derivada: distinguir un
//! alias en español de uno en inglés no es algo que se pueda inferir (`log.warn`
//! comparte brazo con `log.info` y no está obsoleto; `state.increment` tampoco).
//! El precio de curarla es que puede quedarse desincronizada, y una entrada
//! muerta produciría el peor aviso posible: "usa X en vez de Y" donde X no
//! existe. Esto lo impide.

use std::collections::HashSet;

#[test]
fn cada_alias_obsoleto_existe_en_el_registro() {
    let registro: HashSet<String> = orion_vm::cli::builtins::registry()
        .into_iter()
        .filter(|d| !d.owner.is_empty())
        .map(|d| d.qualified.clone())
        .collect();

    let mut fallos = Vec::new();
    for (clave, ingles) in orion_vm::deprecated::todos() {
        let (modulo, _) = clave.split_once('.').expect("la clave es modulo.funcion");

        if !registro.contains(*clave) {
            fallos.push(format!("  '{clave}' ya no existe en el registro"));
        }
        let destino = format!("{modulo}.{ingles}");
        if !registro.contains(&destino) {
            fallos.push(format!(
                "  '{clave}' apunta a '{destino}', que no existe: el aviso mandaría a un nombre inexistente"
            ));
        }
    }

    assert!(
        fallos.is_empty(),
        "src/deprecated.rs no coincide con el registro:\n{}",
        fallos.join("\n")
    );
}

#[test]
fn la_tabla_esta_ordenada_porque_se_busca_por_biseccion() {
    let t = orion_vm::deprecated::todos();
    for par in t.windows(2) {
        assert!(
            par[0].0 < par[1].0,
            "fuera de orden: '{}' antes de '{}' — canonical_for usa binary_search",
            par[0].0, par[1].0
        );
    }
}

#[test]
fn los_modulos_obsoletos_resuelven_a_uno_real() {
    for es in ["tarea", "cola", "formato", "grafo"] {
        let ingles = orion_vm::deprecated::module_canonical(es)
            .unwrap_or_else(|| panic!("'{es}' debería estar marcado como obsoleto"));
        assert!(
            orion_vm::modules::is_known_module(ingles),
            "'{es}' apunta a '{ingles}', que no es un módulo conocido"
        );
    }
    // `df` y `embeddings` son abreviaturas inglesas, NO deprecaciones.
    assert!(orion_vm::deprecated::module_canonical("df").is_none());
    assert!(orion_vm::deprecated::module_canonical("embeddings").is_none());
}

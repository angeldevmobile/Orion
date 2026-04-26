use std::time::Instant;
use std::fs;
use crate::{lexer, parser, codegen, vm, bytecode};
use super::banner;

pub fn run_bench(path: &str, runs: u32) {
    banner::section(&format!("Benchmark — {path}"));

    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            banner::fail(&format!("No se puede leer '{path}': {e}"));
            std::process::exit(1);
        }
    };

    // Compile once and serialize to JSON so we can cheaply re-create the bytecode per run
    // (OrionBytecode doesn't derive Clone, but does derive Serialize + Deserialize)
    let t_compile = Instant::now();
    let tokens = match lexer::lex(&src) {
        Ok(t) => t,
        Err(e) => { banner::fail(&format!("Léxico: {}", e.message)); std::process::exit(1); }
    };
    let stmts = match parser::parse(tokens) {
        Ok(s) => s,
        Err(e) => { banner::fail(&format!("Parse: {}", e.message)); std::process::exit(1); }
    };
    let bc = match codegen::compile(stmts) {
        Ok(b) => b,
        Err(e) => { banner::fail(&format!("Codegen: {}", e.message)); std::process::exit(1); }
    };
    let compile_ms = t_compile.elapsed().as_secs_f64() * 1000.0;

    // Serialize once to reuse cheaply
    let bc_json = serde_json::to_string(&bc).expect("serializar bytecode");

    banner::info(&format!("Compilado en {compile_ms:.2} ms — ejecutando {runs} veces..."));
    println!();

    let mut times: Vec<f64> = Vec::with_capacity(runs as usize);

    for i in 0..runs {
        let bc_run: bytecode::OrionBytecode = serde_json::from_str(&bc_json)
            .expect("deserializar bytecode");
        let t = Instant::now();
        let mut machine = vm::VM::new(bc_run.main, bc_run.lines, bc_run.functions, bc_run.shapes);
        match machine.run() {
            Ok(_) => {}
            Err(e) => {
                banner::fail(&format!("Error en ejecución #{}: {}", i + 1, e));
                std::process::exit(1);
            }
        }
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    let min    = times.iter().cloned().fold(f64::INFINITY, f64::min);
    let max    = times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let avg    = times.iter().sum::<f64>() / times.len() as f64;
    let mut s  = times.clone();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = s[s.len() / 2];
    let p95    = s[(s.len() as f64 * 0.95) as usize];

    println!();
    banner::table_header(&["Métrica", "Tiempo"]);
    banner::table_row(&["Mínimo",      &fmt_ms(min)]);
    banner::table_row(&["Mediana",     &fmt_ms(median)]);
    banner::table_row(&["Promedio",    &fmt_ms(avg)]);
    banner::table_row(&["p95",         &fmt_ms(p95)]);
    banner::table_row(&["Máximo",      &fmt_ms(max)]);
    println!();
    banner::info(&format!("{runs} ejecuciones completadas"));
}

fn fmt_ms(ms: f64) -> String {
    format!("{ms:.3} ms")
}

mod instruction;
mod value;
mod vm;
mod bytecode;

use std::env;
use std::time::Instant;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Uso: orion <archivo.orbc>");
        eprintln!("     orion --version");
        std::process::exit(1);
    }

    if args[1] == "--version" {
        println!("Orion VM v0.1.0 (Rust)");
        return;
    }

    let path = &args[1];
    let t_total = Instant::now();

    // Cargar bytecode
    let t0 = Instant::now();
    let instructions = match bytecode::load(path) {
        Ok(instrs) => instrs,
        Err(e) => {
            eprintln!("[ORION ERROR] {}", e);
            std::process::exit(1);
        }
    };
    let t_load = t0.elapsed();

    // Ejecutar VM
    let t0 = Instant::now();
    let mut machine = vm::VM::new(instructions.main, instructions.lines, instructions.functions);
    match machine.run() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("[ORION RUNTIME ERROR] {}", e);
            std::process::exit(1);
        }
    }
    let t_exec = t0.elapsed();
    let t_total = t_total.elapsed();

    eprintln!();
    eprintln!("                                                  ");
    eprintln!("  Carga   : {:.3} ms", t_load.as_secs_f64() * 1000.0);
    eprintln!("  Exec    : {:.3} ms", t_exec.as_secs_f64() * 1000.0);
    eprintln!("  Total   : {:.3} ms", t_total.as_secs_f64() * 1000.0);
    eprintln!("                                                  ");
}

//! Debugger interactivo de terminal para Orion.
//!
//! Uso:  orion --debug <archivo.orx>
//!
//! Comandos:
//!   b <línea> [if <cond>]   — breakpoint
//!   rb <id|línea>           — eliminar breakpoint
//!   tb <id|línea>           — toggle breakpoint
//!   lb                      — listar breakpoints
//!   n                       — next (step over)
//!   s                       — step into
//!   o                       — step out
//!   c                       — continue
//!   p <var>                 — imprimir variable
//!   w <var>                 — agregar watch
//!   rw <var>                — eliminar watch
//!   lw                      — listar watches
//!   v                       — variables del scope
//!   bt                      — backtrace
//!   stack                   — value stack
//!   l [n]                   — código fuente (contexto ±n líneas, por defecto 3)
//!   q                       — salir

use std::io::{self, BufRead, Write};
use crate::debugger::{DebugSession, PauseReason};
use crate::error::OrionError;
use crate::value::Value;
use super::banner;

// ─── Paleta ANSI ─────────────────────────────────────────────────────────────

const GRN:  &str = "\x1b[32m";
const YLW:  &str = "\x1b[33m";
const CYN:  &str = "\x1b[36m";
const RED:  &str = "\x1b[31m";
const DIM:  &str = "\x1b[2m";
const BLD:  &str = "\x1b[1m";
const RST:  &str = "\x1b[0m";

// ─── Entrada principal ───────────────────────────────────────────────────────

pub fn run_debug(path: &str) {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s.strip_prefix('\u{FEFF}').unwrap_or(&s).to_string(),
        Err(e) => {
            banner::fail(&format!("No se puede leer '{}': {}", path, e));
            return;
        }
    };

    let bc = match compile_src(&src, path) {
        Ok(bc) => bc,
        Err(e) => { eprint!("{}", e.render(&src)); return; }
    };

    let mut session = DebugSession::new(bc, &src);

    banner::section("Orion Debugger");
    println!("  {}Archivo :{} {}", CYN, RST, path);
    println!("  {}{}h{} — ayuda   {}q{} — salir{}", DIM, YLW, RST, YLW, RST, RST);
    println!();

    show_pause(&session);

    let stdin = io::stdin();

    loop {
        if session.done {
            println!("\n{}[DEBUG]{} Programa terminado.", GRN, RST);
            break;
        }

        if session.paused && !session.watches.is_empty() {
            show_watches(&session);
        }

        print!("{}orion-dbg{} {}»{}  ", CYN, RST, YLW, RST);
        io::stdout().flush().unwrap();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).is_err() { break; }
        let cmd = line.trim();

        if cmd.is_empty() { continue; }

        let parts: Vec<&str> = cmd.splitn(3, ' ').collect();

        match parts[0] {
            "h" | "help"      => show_help(),
            "v" | "vars"      => show_vars(&session),
            "bt" | "backtrace"=> show_backtrace(&session),
            "stack"           => show_stack(&session),
            "lb" | "breakpoints" => show_breakpoints(&session),
            "lw" | "watches"  => show_watches(&session),

            "l" | "list" => {
                let radius = parts.get(1)
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(3);
                print_context(&session, radius);
            }

            "c" | "continue" => step_action(&mut session, DebugSession::do_continue),
            "n" | "next"     => step_action(&mut session, DebugSession::do_step_over),
            "s" | "step"     => step_action(&mut session, DebugSession::do_step_into),
            "o" | "out"      => step_action(&mut session, DebugSession::do_step_out),

            "q" | "quit" | "exit" => {
                println!("Saliendo del debugger.");
                break;
            }

            "b" => {
                if parts.len() < 2 {
                    println!("{}Uso: b <línea> [if <condición>]{}", RED, RST);
                    continue;
                }
                match parts[1].parse::<u32>() {
                    Ok(n) => {
                        let cond = if parts.len() == 3 {
                            Some(parts[2].trim_start_matches("if ").trim().to_string())
                        } else {
                            None
                        };
                        let id = session.add_breakpoint(n, cond.clone());
                        let cs = cond.map(|c| format!(" {}[if {}]{}", DIM, c, RST)).unwrap_or_default();
                        println!("{}Breakpoint #{} en línea {}{}{}", GRN, id, n, cs, RST);
                    }
                    Err(_) => println!("{}Número de línea inválido{}", RED, RST),
                }
            }

            "rb" => match parts.get(1).and_then(|s| s.parse::<u32>().ok()) {
                Some(n) => { session.remove_breakpoint(n); println!("Breakpoint eliminado"); }
                None    => println!("{}Uso: rb <id|línea>{}", RED, RST),
            },

            "tb" => match parts.get(1).and_then(|s| s.parse::<u32>().ok()) {
                Some(n) => match session.toggle_breakpoint(n) {
                    Some(true)  => println!("Breakpoint {} {}habilitado{}", n, GRN, RST),
                    Some(false) => println!("Breakpoint {} {}deshabilitado{}", n, RED, RST),
                    None        => println!("{}Breakpoint no encontrado{}", RED, RST),
                },
                None => println!("{}Uso: tb <id|línea>{}", RED, RST),
            },

            "p" => {
                if parts.len() < 2 { println!("{}Uso: p <variable>{}", RED, RST); continue; }
                let name = parts[1..].join(" ");
                match session.lookup_var(name.trim()) {
                    Some(v) => println!("{}{}{} = {}", YLW, name.trim(), RST, dbg_val(&v)),
                    None    => println!("{}'{}'  no está definida{}", RED, name.trim(), RST),
                }
            }

            "w" => {
                if parts.len() < 2 { println!("{}Uso: w <variable>{}", RED, RST); continue; }
                let expr = parts[1..].join(" ").trim().to_string();
                if session.add_watch(expr.clone()) {
                    println!("Watch agregado: {}{}{}", YLW, expr, RST);
                } else {
                    println!("Ya existe ese watch");
                }
            }

            "rw" => {
                if parts.len() < 2 { println!("{}Uso: rw <variable>{}", RED, RST); continue; }
                let expr = parts[1..].join(" ").trim().to_string();
                session.remove_watch(&expr);
                println!("Watch eliminado: {}{}{}", YLW, expr, RST);
            }

            _ => println!("{}Comando desconocido — escribe 'h' para ayuda{}", RED, RST),
        }
    }
}

// ─── Helpers de ejecución ────────────────────────────────────────────────────

fn step_action(session: &mut DebugSession, action: fn(&mut DebugSession)) {
    action(session);
    match session.run_until_pause() {
        Ok(())  => { if !session.done { show_pause(session); } }
        Err(e)  => println!("\n{}[ERROR]{} {}", RED, RST, e),
    }
}

fn print_context(session: &DebugSession, radius: u32) {
    let line = session.vm.current_line();
    for (ln, src, cur) in session.source_context(line, radius) {
        if cur { println!("{}→ {:>4} │ {}{}", GRN, ln, src, RST); }
        else   { println!("  {:>4} │ {}", ln, src); }
    }
}

// ─── Displays de estado ───────────────────────────────────────────────────────

fn show_pause(session: &DebugSession) {
    let line = session.vm.current_line();
    let reason = match &session.pause_reason {
        Some(PauseReason::Breakpoint { id, .. }) => format!("breakpoint #{}", id),
        Some(PauseReason::Step)      => "step".to_string(),
        Some(PauseReason::Entry)     => "inicio del programa".to_string(),
        Some(PauseReason::UserPause) => "pausado".to_string(),
        Some(PauseReason::Error(e))  => format!("{}error:{} {}", RED, RST, e),
        None                         => "desconocido".to_string(),
    };
    println!("\n{}[DEBUG]{} Línea {} — {}", CYN, RST, line, reason);
    print_context(session, 2);
    println!();
}

fn show_vars(session: &DebugSession) {
    let vars = session.vm.debug_frame_vars();
    if vars.is_empty() {
        println!("{}(sin variables en el scope actual){}", DIM, RST);
        return;
    }
    println!("{}Variables:{}", CYN, RST);
    for (name, val) in &vars {
        println!("  {}{:<20}{} = {}", YLW, name, RST, dbg_val(val));
    }
}

fn show_backtrace(session: &DebugSession) {
    let frames = session.debug_frames();
    if frames.is_empty() {
        println!("{}(call stack vacío){}", DIM, RST);
        return;
    }
    println!("{}Call stack:{}", CYN, RST);
    for frame in &frames {
        let marker = if frame.id == 0 { "→" } else { " " };
        if frame.line > 0 {
            println!("  {} #{} {}{}{}  {}(línea {}){}", marker, frame.id, BLD, frame.name, RST, DIM, frame.line, RST);
        } else {
            println!("  {} #{} {}{}{}", marker, frame.id, BLD, frame.name, RST);
        }
    }
}

fn show_stack(session: &DebugSession) {
    let stack = session.vm.debug_value_stack();
    if stack.is_empty() {
        println!("{}(value stack vacío){}", DIM, RST);
        return;
    }
    println!("{}Value stack{} {}(top → bottom):{}", CYN, RST, DIM, RST);
    for (i, val) in stack.iter().rev().enumerate() {
        println!("  [{:>2}] {}", i, dbg_val(val));
    }
}

fn show_breakpoints(session: &DebugSession) {
    let bps = session.list_breakpoints();
    if bps.is_empty() {
        println!("{}(sin breakpoints){}", DIM, RST);
        return;
    }
    println!("{}Breakpoints:{}", CYN, RST);
    for bp in bps {
        let color  = if bp.enabled { GRN } else { RED };
        let status = if bp.enabled { "●" }  else { "○" };
        let cond   = bp.condition.as_deref()
            .map(|c| format!("  {}[if {}]{}", DIM, c, RST))
            .unwrap_or_default();
        println!("  {}{} #{}{}  línea {}  hits: {}{}", color, status, bp.id, RST, bp.line, bp.hit_count, cond);
    }
}

fn show_watches(session: &DebugSession) {
    let watches = session.eval_watches();
    if watches.is_empty() {
        println!("{}(sin watches){}", DIM, RST);
        return;
    }
    println!("{}Watches:{}", CYN, RST);
    for (expr, val) in &watches {
        let display = val.as_ref()
            .map(|v| dbg_val(v))
            .unwrap_or_else(|| format!("{}«no definida»{}", DIM, RST));
        println!("  {}{:<20}{} = {}", YLW, expr, RST, display);
    }
}

fn show_help() {
    println!("\n{}{}Orion Debugger — Comandos:{}{}", BLD, CYN, RST, RST);
    let cmds: &[(&str, &str)] = &[
        ("b <línea>",            "Poner breakpoint en esa línea"),
        ("b <línea> if <cond>",  "Breakpoint condicional"),
        ("rb <id|línea>",        "Eliminar breakpoint"),
        ("tb <id|línea>",        "Habilitar / deshabilitar breakpoint"),
        ("lb",                   "Listar todos los breakpoints"),
        ("─────────────────────",  ""),
        ("n",                    "Next — step over (no entra en funciones)"),
        ("s",                    "Step into (entra en la función llamada)"),
        ("o",                    "Step out (ejecuta hasta salir del frame)"),
        ("c",                    "Continue (hasta el próximo breakpoint)"),
        ("─────────────────────",  ""),
        ("p <var>",              "Imprimir valor de una variable"),
        ("w <var>",              "Agregar variable a watches"),
        ("rw <var>",             "Eliminar watch"),
        ("lw",                   "Listar watches con valores actuales"),
        ("─────────────────────",  ""),
        ("v",                    "Variables del scope actual"),
        ("bt",                   "Backtrace — call stack completo"),
        ("stack",                "Value stack de la VM"),
        ("l [n]",                "Código fuente alrededor de la línea actual (±n)"),
        ("─────────────────────",  ""),
        ("h",                    "Esta ayuda"),
        ("q",                    "Salir del debugger"),
    ];
    for (cmd, desc) in cmds {
        if desc.is_empty() {
            println!("  {}{}{}", DIM, cmd, RST);
        } else {
            println!("  {}{:<26}{} {}", YLW, cmd, RST, desc);
        }
    }
    println!();
}

// ─── Formato de valores ──────────────────────────────────────────────────────

fn dbg_val(v: &Value) -> String {
    match v {
        Value::Null            => format!("{}null{}", DIM, RST),
        Value::Bool(b)         => format!("{}{}{}", CYN, b, RST),
        Value::Int(n)          => n.to_string(),
        Value::Float(f)        => format!("{}", f),
        Value::Str(s)          => format!("{}\"{}\"{}",  GRN, s, RST),
        Value::List(items)     => {
            if items.is_empty() { return format!("{}[]{}", DIM, RST); }
            let inner: Vec<String> = items.iter().map(dbg_val).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Dict(map)       => {
            if map.is_empty() { return format!("{}{{}}{}", DIM, RST); }
            let inner: Vec<String> = map.iter()
                .map(|(k, v)| format!("{}{}{}: {}", YLW, k, RST, dbg_val(v)))
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
        Value::Instance(rc)    => format!("{}«{}»{}", CYN, rc.borrow().shape_name, RST),
        Value::Closure { fn_name, .. } => format!("{}‹fn {}›{}", DIM, fn_name, RST),
        Value::Task(_)         => format!("{}‹task›{}", DIM, RST),
        Value::Ptr(p)          => format!("{}0x{:x}{}", DIM, p, RST),
    }
}

// ─── Pipeline de compilación local ───────────────────────────────────────────

fn compile_src(src: &str, path: &str) -> Result<crate::bytecode::OrionBytecode, OrionError> {
    let tokens = crate::lexer::lex(src)
        .map_err(|e| OrionError::from(e).with_file(path))?;
    let stmts = crate::parser::parse(tokens)
        .map_err(|e| OrionError::from(e).with_file(path))?;
    crate::codegen::compile(stmts)
        .map_err(|e| OrionError::from(e).with_file(path))
}

//! Servidor Debug Adapter Protocol (DAP) sobre stdio.
//!
//! VS Code lanza este proceso con `orion --dap <archivo.orx>` y se comunica
//! con él mediante el protocolo DAP: mensajes JSON precedidos de una cabecera
//! `Content-Length: N\r\n\r\n`.
//!
//! Capacidades implementadas:
//!   initialize, launch, setBreakpoints, configurationDone
//!   threads, stackTrace, scopes, variables, evaluate
//!   continue, next, stepIn, stepOut, pause, disconnect
//!
//! Eventos emitidos:
//!   initialized, stopped, continued, exited, terminated, output

use std::io::{self, BufRead, Read, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use serde_json::{json, Value};

use crate::debugger::{DebugSession, PauseReason};
use crate::error::OrionError;

// ─── Constantes DAP ──────────────────────────────────────────────────────────

/// variablesReference base para scopes locales de cada frame (frame_id + BASE_REF).
const LOCALS_REF_BASE: u64 = 1000;
/// variablesReference para el value stack de la VM.
const STACK_REF: u64 = 9999;

// ─── I/O DAP ─────────────────────────────────────────────────────────────────

/// Lee un mensaje DAP completo desde un BufReader.
fn read_message(stdin: &mut impl BufRead) -> Option<Value> {
    let mut content_length: usize = 0;
    loop {
        let mut header = String::new();
        if stdin.read_line(&mut header).ok()? == 0 {
            return None;
        }
        let h = header.trim_end_matches(['\r', '\n']);
        if h.is_empty() { break; }
        if let Some(rest) = h.strip_prefix("Content-Length: ") {
            content_length = rest.parse().ok()?;
        }
    }
    if content_length == 0 { return None; }
    let mut buf = vec![0u8; content_length];
    stdin.read_exact(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

/// Escribe un mensaje DAP a stdout.
fn write_message(msg: &Value) {
    let body = serde_json::to_string(msg).unwrap();
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut out = io::stdout().lock();
    out.write_all(header.as_bytes()).unwrap();
    out.write_all(body.as_bytes()).unwrap();
    out.flush().unwrap();
}

fn send_response(seq: u64, req_seq: u64, command: &str, success: bool, body: Value) {
    write_message(&json!({
        "seq": seq,
        "type": "response",
        "request_seq": req_seq,
        "success": success,
        "command": command,
        "body": body,
    }));
}

fn send_event(seq: u64, event: &str, body: Value) {
    write_message(&json!({
        "seq": seq,
        "type": "event",
        "event": event,
        "body": body,
    }));
}

// ─── Canal de comandos entre hilo-stdin y hilo-VM ─────────────────────────────

#[derive(Debug)]
enum DapCmd {
    /// El cliente envió un mensaje completo.
    Message(Value),
    /// EOF en stdin.
    Disconnect,
}

// ─── Punto de entrada ────────────────────────────────────────────────────────

pub fn run_dap(path: &str) {
    // Compilar fuente
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s.strip_prefix('\u{FEFF}').unwrap_or(&s).to_string(),
        Err(e) => {
            eprintln!("[dap] no se puede leer '{}': {}", path, e);
            return;
        }
    };

    let bc = match compile_src(&src, path) {
        Ok(bc) => bc,
        Err(e) => {
            // Enviar error como output event antes de terminar
            send_event(1, "output", json!({
                "category": "stderr",
                "output": format!("{}\n", e.render(&src)),
            }));
            return;
        }
    };

    // Hilo lector de stdin
    let (tx, rx): (Sender<DapCmd>, Receiver<DapCmd>) = mpsc::channel();
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = io::BufReader::new(stdin.lock());
        loop {
            match read_message(&mut reader) {
                Some(msg) => { if tx.send(DapCmd::Message(msg)).is_err() { break; } }
                None      => { let _ = tx.send(DapCmd::Disconnect); break; }
            }
        }
    });

    let mut session = DebugSession::new(bc, &src);
    let mut seq: u64 = 1;
    let mut configured = false;
    let mut source_path = path.to_string();

    // Bucle principal DAP
    loop {
        // ── Esperar mensaje del cliente ──────────────────────────────────────
        let msg = match rx.recv() {
            Ok(DapCmd::Message(m)) => m,
            Ok(DapCmd::Disconnect) | Err(_) => break,
        };

        let req_seq  = msg["seq"].as_u64().unwrap_or(0);
        let cmd      = msg["command"].as_str().unwrap_or("").to_string();
        let args     = msg.get("arguments").cloned().unwrap_or(json!({}));

        seq += 1;

        match cmd.as_str() {

            // ── initialize ───────────────────────────────────────────────────
            "initialize" => {
                send_response(seq, req_seq, "initialize", true, json!({
                    "supportsConfigurationDoneRequest": true,
                    "supportsFunctionBreakpoints":      false,
                    "supportsConditionalBreakpoints":   true,
                    "supportsHitConditionalBreakpoints":false,
                    "supportsEvaluateForHovers":        true,
                    "supportsSetVariable":              false,
                    "supportsRestartRequest":           false,
                    "supportsTerminateRequest":         true,
                    "supportsLogPoints":                false,
                }));
                seq += 1;
                send_event(seq, "initialized", json!({}));
            }

            // ── launch ───────────────────────────────────────────────────────
            "launch" => {
                if let Some(p) = args["program"].as_str() {
                    source_path = p.to_string();
                }
                send_response(seq, req_seq, "launch", true, json!({}));
            }

            // ── setBreakpoints ───────────────────────────────────────────────
            "setBreakpoints" => {
                let lines: Vec<u32> = args["breakpoints"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|bp| bp["line"].as_u64().map(|l| l as u32))
                    .collect();

                let bp_ids = session.set_breakpoints_for_file(&lines);

                let bp_list: Vec<Value> = lines.iter().zip(bp_ids.iter()).map(|(line, id)| {
                    json!({ "id": id, "verified": true, "line": line })
                }).collect();

                send_response(seq, req_seq, "setBreakpoints", true, json!({
                    "breakpoints": bp_list,
                }));
            }

            // ── configurationDone ────────────────────────────────────────────
            "configurationDone" => {
                configured = true;
                send_response(seq, req_seq, "configurationDone", true, json!({}));

                // Arrancar ejecución hasta el primer breakpoint / pausa
                if !session.done {
                    session.do_continue();
                    match session.run_until_pause() {
                        Ok(()) => {}
                        Err(e) => {
                            seq += 1;
                            send_event(seq, "output", json!({
                                "category": "stderr",
                                "output": format!("{}\n", e),
                            }));
                        }
                    }
                    seq += 1;
                    send_stopped_or_terminated(&mut seq, &session, &source_path);
                }
            }

            // ── threads ──────────────────────────────────────────────────────
            "threads" => {
                send_response(seq, req_seq, "threads", true, json!({
                    "threads": [{ "id": 1, "name": "main" }],
                }));
            }

            // ── stackTrace ───────────────────────────────────────────────────
            "stackTrace" => {
                let frames: Vec<Value> = session.debug_frames().iter().map(|f| {
                    json!({
                        "id":     f.id,
                        "name":   f.name,
                        "line":   f.line,
                        "column": 1,
                        "source": { "path": source_path, "name": source_path.split(['/', '\\']).last().unwrap_or("") },
                    })
                }).collect();
                send_response(seq, req_seq, "stackTrace", true, json!({
                    "stackFrames": frames,
                    "totalFrames": frames.len(),
                }));
            }

            // ── scopes ───────────────────────────────────────────────────────
            "scopes" => {
                let frame_id = args["frameId"].as_u64().unwrap_or(0) as usize;
                let vars = session.frame_vars(frame_id);
                send_response(seq, req_seq, "scopes", true, json!({
                    "scopes": [
                        {
                            "name":               "Locals",
                            "presentationHint":   "locals",
                            "variablesReference": LOCALS_REF_BASE + frame_id as u64,
                            "namedVariables":     vars.len(),
                            "expensive":          false,
                        },
                        {
                            "name":               "Value Stack",
                            "presentationHint":   "registers",
                            "variablesReference": STACK_REF,
                            "indexedVariables":   session.vm.debug_value_stack().len(),
                            "expensive":          false,
                        },
                    ],
                }));
            }

            // ── variables ────────────────────────────────────────────────────
            "variables" => {
                let vref = args["variablesReference"].as_u64().unwrap_or(0);

                let variables: Vec<Value> = if vref == STACK_REF {
                    // Value stack
                    session.vm.debug_value_stack().iter().rev().enumerate()
                        .map(|(i, v)| json!({
                            "name":               format!("[{}]", i),
                            "value":              dap_val(v),
                            "type":               v.type_name(),
                            "variablesReference": 0,
                        }))
                        .collect()
                } else if vref >= LOCALS_REF_BASE {
                    let frame_id = (vref - LOCALS_REF_BASE) as usize;
                    session.frame_vars(frame_id).iter()
                        .map(|(name, val)| {
                            let vr = children_ref(val);
                            json!({
                                "name":               name,
                                "value":              dap_val(val),
                                "type":               val.type_name(),
                                "variablesReference": vr,
                            })
                        })
                        .collect()
                } else {
                    vec![]
                };

                send_response(seq, req_seq, "variables", true, json!({
                    "variables": variables,
                }));
            }

            // ── evaluate ─────────────────────────────────────────────────────
            "evaluate" => {
                let expr = args["expression"].as_str().unwrap_or("").trim();
                let result = session.lookup_var(expr)
                    .map(|v| dap_val(&v))
                    .unwrap_or_else(|| format!("«{}» no definida", expr));
                send_response(seq, req_seq, "evaluate", true, json!({
                    "result":             result,
                    "variablesReference": 0,
                }));
            }

            // ── continue ─────────────────────────────────────────────────────
            "continue" => {
                send_response(seq, req_seq, "continue", true, json!({
                    "allThreadsContinued": true,
                }));
                seq += 1;
                send_event(seq, "continued", json!({ "threadId": 1, "allThreadsContinued": true }));

                session.do_continue();
                let result = session.run_until_pause();
                if let Err(e) = result {
                    seq += 1;
                    send_event(seq, "output", json!({ "category": "stderr", "output": format!("{}\n", e) }));
                }
                seq += 1;
                send_stopped_or_terminated(&mut seq, &session, &source_path);
            }

            // ── next (step over) ──────────────────────────────────────────────
            "next" => {
                send_response(seq, req_seq, "next", true, json!({}));
                session.do_step_over();
                let result = session.run_until_pause();
                if let Err(e) = result {
                    seq += 1;
                    send_event(seq, "output", json!({ "category": "stderr", "output": format!("{}\n", e) }));
                }
                seq += 1;
                send_stopped_or_terminated(&mut seq, &session, &source_path);
            }

            // ── stepIn ───────────────────────────────────────────────────────
            "stepIn" => {
                send_response(seq, req_seq, "stepIn", true, json!({}));
                session.do_step_into();
                let result = session.run_until_pause();
                if let Err(e) = result {
                    seq += 1;
                    send_event(seq, "output", json!({ "category": "stderr", "output": format!("{}\n", e) }));
                }
                seq += 1;
                send_stopped_or_terminated(&mut seq, &session, &source_path);
            }

            // ── stepOut ──────────────────────────────────────────────────────
            "stepOut" => {
                send_response(seq, req_seq, "stepOut", true, json!({}));
                session.do_step_out();
                let result = session.run_until_pause();
                if let Err(e) = result {
                    seq += 1;
                    send_event(seq, "output", json!({ "category": "stderr", "output": format!("{}\n", e) }));
                }
                seq += 1;
                send_stopped_or_terminated(&mut seq, &session, &source_path);
            }

            // ── pause ─────────────────────────────────────────────────────────
            "pause" => {
                send_response(seq, req_seq, "pause", true, json!({}));
                // La ejecución ya está detenida en el loop; simplemente reportamos
                seq += 1;
                send_event(seq, "stopped", json!({
                    "reason":            "pause",
                    "threadId":          1,
                    "allThreadsStopped": true,
                }));
            }

            // ── terminate / disconnect ────────────────────────────────────────
            "terminate" | "disconnect" => {
                send_response(seq, req_seq, &cmd, true, json!({}));
                seq += 1;
                send_event(seq, "terminated", json!({}));
                break;
            }

            other => {
                // Responder OK vacío para mensajes no reconocidos
                send_response(seq, req_seq, other, true, json!({}));
            }
        }

        // Si el programa terminó naturalmente, notificar
        if configured && session.done {
            break;
        }
    }

    // Fin de sesión
    seq += 1;
    send_event(seq, "exited",     json!({ "exitCode": 0 }));
    seq += 1;
    send_event(seq, "terminated", json!({}));
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn send_stopped_or_terminated(seq: &mut u64, session: &DebugSession, _source: &str) {
    if session.done {
        send_event(*seq, "exited",     json!({ "exitCode": 0 }));
        *seq += 1;
        send_event(*seq, "terminated", json!({}));
    } else {
        let reason = match &session.pause_reason {
            Some(PauseReason::Breakpoint { .. }) => "breakpoint",
            Some(PauseReason::Step)              => "step",
            Some(PauseReason::Entry)             => "entry",
            Some(PauseReason::Error(_))          => "exception",
            _                                    => "pause",
        };
        let description = match &session.pause_reason {
            Some(PauseReason::Breakpoint { id, line }) =>
                format!("Breakpoint #{} en línea {}", id, line),
            Some(PauseReason::Error(e)) => e.clone(),
            _ => String::new(),
        };
        let mut body = json!({
            "reason":            reason,
            "threadId":          1,
            "allThreadsStopped": true,
        });
        if !description.is_empty() {
            body["description"] = json!(description);
        }
        send_event(*seq, "stopped", body);
    }
}

/// Convierte un `Value` de Orion a texto para el panel Variables del DAP.
fn dap_val(v: &crate::value::Value) -> String {
    use crate::value::Value;
    match v {
        Value::Null               => "null".into(),
        Value::Bool(b)            => b.to_string(),
        Value::Int(n)             => n.to_string(),
        Value::Float(f)           => format!("{}", f),
        Value::Str(s)             => format!("\"{}\"", s),
        Value::List(items)        => format!("[{} items]", items.len()),
        Value::Dict(map)          => format!("{{{} keys}}", map.len()),
        Value::Instance(rc)       => format!("<{}>", rc.borrow().shape_name),
        Value::Closure { fn_name, .. } => format!("<fn {}>", fn_name),
        Value::Task(_)            => "<task>".into(),
        Value::Ptr(p)             => format!("0x{:x}", p),
    }
}

/// variablesReference para valores que tienen hijos (listas/dicts/instancias).
/// Por ahora devuelve 0 (sin expansión); se puede extender en el futuro.
fn children_ref(_v: &crate::value::Value) -> u64 {
    0
}

fn compile_src(src: &str, path: &str) -> Result<crate::bytecode::OrionBytecode, OrionError> {
    let tokens = crate::lexer::lex(src)
        .map_err(|e| OrionError::from(e).with_file(path))?;
    let stmts = crate::parser::parse(tokens)
        .map_err(|e| OrionError::from(e).with_file(path))?;
    crate::codegen::compile(stmts)
        .map_err(|e| OrionError::from(e).with_file(path))
}

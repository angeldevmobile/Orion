use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::process::Command;
use std::time::Instant;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // execute(command) → {code, out, err}
        "execute" => {
            let cmd = one_str("execute", args)?;
            let output = run_shell(&cmd)?;
            Ok(output)
        }
        // execute_timed(command) → {code, out, err, elapsed}
        "execute_timed" => {
            let cmd   = one_str("execute_timed", args)?;
            let start = Instant::now();
            let mut output = run_shell(&cmd)?;
            let elapsed = start.elapsed().as_secs_f64();
            if let EvalValue::Dict(ref mut m) = output {
                m.insert("elapsed".into(), EvalValue::Float(elapsed));
            }
            Ok(output)
        }
        // background(command) → {pid}
        "background" => {
            let cmd = one_str("background", args)?;
            let child = spawn_shell(&cmd)?;
            let mut m = HashMap::new();
            m.insert("pid".into(), EvalValue::Int(child as i64));
            Ok(EvalValue::Dict(m))
        }
        // check_dependency(cmd) → bool
        "check_dependency" => {
            let cmd = one_str("check_dependency", args)?;
            let exists = which_exists(&cmd);
            Ok(EvalValue::Bool(exists))
        }
        // pid() → PID del proceso actual
        "pid" => {
            Ok(EvalValue::Int(std::process::id() as i64))
        }
        // exit(code?) → termina el proceso
        "exit" => {
            let code = if args.is_empty() { 0 } else { to_i64(&args[0])? };
            std::process::exit(code as i32);
        }
        // env_var(key) → valor de variable de entorno
        "env_var" => {
            let key = one_str("env_var", args)?;
            match std::env::var(&key) {
                Ok(v)  => Ok(EvalValue::Str(v)),
                Err(_) => Ok(EvalValue::Null),
            }
        }
        // cwd() → directorio actual
        "cwd" => {
            let path = std::env::current_dir().map_err(|e| e.to_string())?;
            Ok(EvalValue::Str(path.to_string_lossy().into_owned()))
        }

        f => Err(format!("process.{}() no existe", f)),
    }
}

fn run_shell(cmd: &str) -> Result<EvalValue, String> {
    let (prog, arg) = shell_args();
    let output = Command::new(prog)
        .arg(arg)
        .arg(cmd)
        .output()
        .map_err(|e| format!("process.execute: {}", e))?;
    let mut m = HashMap::new();
    m.insert("code".into(), EvalValue::Int(output.status.code().unwrap_or(-1) as i64));
    m.insert("out".into(),  EvalValue::Str(String::from_utf8_lossy(&output.stdout).trim().to_string()));
    m.insert("err".into(),  EvalValue::Str(String::from_utf8_lossy(&output.stderr).trim().to_string()));
    Ok(EvalValue::Dict(m))
}

fn spawn_shell(cmd: &str) -> Result<u32, String> {
    let (prog, arg) = shell_args();
    let child = Command::new(prog)
        .arg(arg)
        .arg(cmd)
        .spawn()
        .map_err(|e| format!("process.background: {}", e))?;
    Ok(child.id())
}

fn shell_args() -> (&'static str, &'static str) {
    if cfg!(target_os = "windows") { ("cmd", "/C") } else { ("sh", "-c") }
}

fn which_exists(cmd: &str) -> bool {
    if cfg!(target_os = "windows") {
        Command::new("where").arg(cmd).output()
            .map(|o| o.status.success()).unwrap_or(false)
    } else {
        Command::new("which").arg(cmd).output()
            .map(|o| o.status.success()).unwrap_or(false)
    }
}

fn one_str(fn_name: &str, args: Vec<EvalValue>) -> Result<String, String> {
    if args.is_empty() {
        return Err(format!("process.{}() requiere 1 argumento", fn_name));
    }
    Ok(match args.into_iter().next().unwrap() {
        EvalValue::Str(s) => s,
        other => format!("{}", other),
    })
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("process: esperaba número, recibió {}", other.type_name())),
    }
}

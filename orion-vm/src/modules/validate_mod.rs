use crate::eval_value::EvalValue;
use std::collections::HashMap;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // email(s) → Bool
        "email" => {
            let s = one_str("validate.email", &args)?;
            Ok(EvalValue::Bool(is_email(&s)))
        }
        // url(s) → Bool
        "url" => {
            let s = one_str("validate.url", &args)?;
            Ok(EvalValue::Bool(s.starts_with("http://") || s.starts_with("https://")))
        }
        // requerido(valor) → Bool
        "requerido" | "required" => {
            let v = args.first().ok_or("validate.requerido requiere (valor)")?;
            Ok(EvalValue::Bool(!matches!(v, EvalValue::Null) && !to_str(v).is_empty()))
        }
        // longitud(s, min, max) → Bool
        "longitud" | "length" => {
            if args.len() < 3 { return Err("validate.longitud requiere (s, min, max)".into()); }
            let s   = to_str(&args[0]);
            let min = to_i64(&args[1])? as usize;
            let max = to_i64(&args[2])? as usize;
            let len = s.chars().count();
            Ok(EvalValue::Bool(len >= min && len <= max))
        }
        // numero(s) → Bool
        "numero" | "number" => {
            let s = one_str("validate.numero", &args)?;
            Ok(EvalValue::Bool(s.parse::<f64>().is_ok()))
        }
        // alfa(s) → Bool  — solo letras
        "alfa" | "alpha" => {
            let s = one_str("validate.alfa", &args)?;
            Ok(EvalValue::Bool(s.chars().all(|c| c.is_alphabetic())))
        }
        // alfanumerico(s) → Bool
        "alfanumerico" | "alphanumeric" => {
            let s = one_str("validate.alfanumerico", &args)?;
            Ok(EvalValue::Bool(s.chars().all(|c| c.is_alphanumeric())))
        }
        // todo(datos_dict, reglas_dict) → {valido, errores}
        // Reglas: "requerido|email|min:5|max:100|numero|alfa"
        "todo" | "all" => {
            if args.len() < 2 { return Err("validate.todo requiere (datos, reglas)".into()); }
            let datos = match &args[0] {
                EvalValue::Dict(m) => m.clone(),
                _ => return Err("validate.todo: datos debe ser un Dict".into()),
            };
            let reglas = match &args[1] {
                EvalValue::Dict(m) => m.clone(),
                _ => return Err("validate.todo: reglas debe ser un Dict".into()),
            };
            validate_all(datos, reglas)
        }
        f => Err(format!("validate.{}() no existe", f)),
    }
}

fn validate_all(
    datos: HashMap<String, EvalValue>,
    reglas: HashMap<String, EvalValue>,
) -> Result<EvalValue, String> {
    let mut errores = Vec::new();
    for (campo, regla) in &reglas {
        let valor     = datos.get(campo).cloned().unwrap_or(EvalValue::Null);
        let regla_str = to_str(regla);
        for r in regla_str.split('|') {
            let r  = r.trim();
            let ok = match r {
                "requerido" => !matches!(valor, EvalValue::Null) && !to_str(&valor).is_empty(),
                "email"     => is_email(&to_str(&valor)),
                "numero"    => to_str(&valor).parse::<f64>().is_ok(),
                "alfa"      => to_str(&valor).chars().all(|c| c.is_alphabetic()),
                "url"       => {
                    let s = to_str(&valor);
                    s.starts_with("http://") || s.starts_with("https://")
                }
                r if r.starts_with("min:") => {
                    let n: usize = r[4..].parse().unwrap_or(0);
                    to_str(&valor).chars().count() >= n
                }
                r if r.starts_with("max:") => {
                    let n: usize = r[4..].parse().unwrap_or(usize::MAX);
                    to_str(&valor).chars().count() <= n
                }
                _ => true,
            };
            if !ok {
                errores.push(EvalValue::Str(format!("{}: falla regla '{}'", campo, r)));
            }
        }
    }
    let valido = errores.is_empty();
    let mut result = HashMap::new();
    result.insert("valido".into(),  EvalValue::Bool(valido));
    result.insert("errores".into(), EvalValue::List(errores));
    Ok(EvalValue::Dict(result))
}

fn is_email(s: &str) -> bool {
    let parts: Vec<&str> = s.splitn(2, '@').collect();
    if parts.len() != 2 { return false; }
    let (local, domain) = (parts[0], parts[1]);
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

fn one_str(fn_name: &str, args: &[EvalValue]) -> Result<String, String> {
    if args.is_empty() { return Err(format!("{} requiere argumento", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("validate: esperaba número, recibió {}", other.type_name())),
    }
}

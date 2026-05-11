use crate::eval_value::EvalValue;
use comfy_table::{Table, ContentArrangement};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // tabla(encabezados, filas) → String  — tabla ASCII formateada
        // filas puede ser List<List> o List<Dict>
        "tabla" | "table" => {
            if args.len() < 2 { return Err("formato.tabla requiere (encabezados, filas)".into()); }
            let headers = to_str_list(&args[0])?;
            let rows    = match &args[1] {
                EvalValue::List(l) => l.clone(),
                _ => return Err("formato.tabla: filas debe ser una lista".into()),
            };
            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic)
                 .set_header(headers.iter().map(String::as_str).collect::<Vec<_>>());
            for row in &rows {
                match row {
                    EvalValue::List(cols) => {
                        table.add_row(cols.iter().map(|c| format!("{}", c)).collect::<Vec<_>>());
                    }
                    EvalValue::Dict(m) => {
                        let vals: Vec<String> = headers.iter()
                            .map(|h| m.get(h).map(|v| format!("{}", v)).unwrap_or_default())
                            .collect();
                        table.add_row(vals);
                    }
                    other => { table.add_row(vec![format!("{}", other)]); }
                }
            }
            Ok(EvalValue::Str(format!("{}", table)))
        }
        // separador(ancho, caracter?) → String  — línea horizontal
        "separador" | "divider" => {
            let ancho = to_i64(args.first().ok_or("formato.separador requiere (ancho)")?)?;
            let ch    = args.get(1).map(to_str_val).unwrap_or_else(|| "─".to_string());
            let ch    = ch.chars().next().unwrap_or('─');
            Ok(EvalValue::Str(std::iter::repeat(ch).take(ancho as usize).collect()))
        }
        // centrar(s, ancho) → String  — texto centrado con espacios
        "centrar" | "center" => {
            if args.len() < 2 { return Err("formato.centrar requiere (s, ancho)".into()); }
            let s     = to_str_val(&args[0]);
            let ancho = to_i64(&args[1])? as usize;
            let len   = s.chars().count();
            if len >= ancho {
                Ok(EvalValue::Str(s))
            } else {
                let pad = ancho - len;
                let left  = pad / 2;
                let right = pad - left;
                Ok(EvalValue::Str(format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))))
            }
        }
        f => Err(format!("formato.{}() no existe", f)),
    }
}

fn to_str_list(v: &EvalValue) -> Result<Vec<String>, String> {
    match v {
        EvalValue::List(l) => Ok(l.iter().map(to_str_val).collect()),
        _ => Err("formato: encabezados debe ser una lista".into()),
    }
}

fn to_str_val(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("formato: esperaba número, recibió {}", other.type_name())),
    }
}

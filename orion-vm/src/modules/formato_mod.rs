use crate::eval_value::EvalValue;
use comfy_table::{Table, ContentArrangement};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // table(encabezados, filas) → String  — tabla ASCII formateada
        // filas puede ser List<List> o List<Dict>
        "table" | "tabla" => {
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
        // divider(ancho, caracter?) → String  — línea horizontal
        "divider" | "separador" => {
            let ancho = to_i64(args.first().ok_or("formato.separador requiere (ancho)")?)?.max(0);
            let ch    = args.get(1).map(to_str_val).unwrap_or_else(|| "─".to_string());
            let ch    = ch.chars().next().unwrap_or('─');
            Ok(EvalValue::Str(std::iter::repeat(ch).take(ancho as usize).collect()))
        }
        // number(n, decimales=0, miles=",", decimal=".") → "1,487,000.50"
        // Estilo español: numero(n, 2, ".", ",") → "1.487.000,50"
        "number" | "numero" => {
            let n = to_f64(args.first().ok_or("formato.numero requiere (n)")?)?;
            let dec  = args.get(1).and_then(|v| to_i64(v).ok()).unwrap_or(0).max(0) as usize;
            let thou = args.get(2).map(to_str_val).unwrap_or_else(|| ",".to_string());
            let dsep = args.get(3).map(to_str_val).unwrap_or_else(|| ".".to_string());
            Ok(EvalValue::Str(format_number(n, dec, &thou, &dsep)))
        }
        // currency(n, simbolo="$", decimales=2) → "$1,487,000.00"
        // Símbolo alfabético va con espacio: moneda(n, "USD") → "USD 1,487,000.00"
        "currency" | "moneda" => {
            let n = to_f64(args.first().ok_or("formato.moneda requiere (n)")?)?;
            let sym = args.get(1).map(to_str_val).unwrap_or_else(|| "$".to_string());
            let dec = args.get(2).and_then(|v| to_i64(v).ok()).unwrap_or(2).max(0) as usize;
            let num = format_number(n, dec, ",", ".");
            let sep = if sym.chars().all(|c| c.is_alphabetic()) { " " } else { "" };
            Ok(EvalValue::Str(format!("{}{}{}", sym, sep, num)))
        }
        // percent(x, decimales=1) → 0.156 → "15.6%"
        "percent" | "porcentaje" => {
            let x = to_f64(args.first().ok_or("formato.porcentaje requiere (x)")?)?;
            let dec = args.get(1).and_then(|v| to_i64(v).ok()).unwrap_or(1).max(0) as usize;
            Ok(EvalValue::Str(format!("{:.*}%", dec, x * 100.0)))
        }
        // bytes(n) → tamaño humano en base 1024: "512 B", "1.5 KB", "2 MB"
        "bytes" => {
            let mut n = to_f64(args.first().ok_or("formato.bytes requiere (n)")?)?.max(0.0);
            let units = ["B", "KB", "MB", "GB", "TB", "PB"];
            let mut i = 0;
            while n >= 1024.0 && i < units.len() - 1 { n /= 1024.0; i += 1; }
            let s = if i == 0 || n.fract() < 0.05 {
                format!("{} {}", n.round() as i64, units[i])
            } else {
                format!("{:.1} {}", n, units[i])
            };
            Ok(EvalValue::Str(s))
        }
        // duration(segundos) → "1d 2h 3m 4s"; menos de 1s → "500ms"
        "duration" | "duracion" => {
            let secs = to_f64(args.first().ok_or("formato.duracion requiere (segundos)")?)?;
            if secs <= 0.0 { return Ok(EvalValue::Str("0s".into())); }
            if secs < 1.0 {
                return Ok(EvalValue::Str(format!("{}ms", (secs * 1000.0).round() as i64)));
            }
            let mut s = secs.round() as i64;
            let d = s / 86400; s %= 86400;
            let h = s / 3600;  s %= 3600;
            let m = s / 60;    s %= 60;
            let mut parts = Vec::new();
            if d > 0 { parts.push(format!("{}d", d)); }
            if h > 0 { parts.push(format!("{}h", h)); }
            if m > 0 { parts.push(format!("{}m", m)); }
            if s > 0 || parts.is_empty() { parts.push(format!("{}s", s)); }
            Ok(EvalValue::Str(parts.join(" ")))
        }
        // truncate(s, max) → corta a max caracteres agregando "…" si hizo falta
        "truncate" | "truncar" => {
            if args.len() < 2 { return Err("formato.truncar requiere (s, max)".into()); }
            let s   = to_str_val(&args[0]);
            let max = to_i64(&args[1])?.max(0) as usize;
            let out = if s.chars().count() <= max {
                s
            } else if max == 0 {
                String::new()
            } else {
                format!("{}…", s.chars().take(max - 1).collect::<String>())
            };
            Ok(EvalValue::Str(out))
        }
        // center(s, ancho) → String  — texto centrado con espacios
        "center" | "centrar" => {
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

fn to_f64(v: &EvalValue) -> Result<f64, String> {
    v.to_f64().map_err(|e| format!("formato: {}", e))
}

// Agrupa la parte entera en miles y aplica los separadores pedidos.
fn format_number(n: f64, dec: usize, thou: &str, dsep: &str) -> String {
    let neg = n < 0.0;
    let s = format!("{:.*}", dec, n.abs());
    let (int_part, frac_part) = match s.split_once('.') {
        Some((a, b)) => (a.to_string(), Some(b.to_string())),
        None => (s, None),
    };
    let chars: Vec<char> = int_part.chars().collect();
    let mut grouped = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i) % 3 == 0 { grouped.push_str(thou); }
        grouped.push(*c);
    }
    let mut out = String::new();
    if neg { out.push('-'); }
    out.push_str(&grouped);
    if let Some(f) = frac_part {
        out.push_str(dsep);
        out.push_str(&f);
    }
    out
}

use crate::eval_value::EvalValue;
use rand::Rng;
use rand::seq::SliceRandom;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let mut rng = rand::thread_rng();
    match function {
        // int(a, b) → entero aleatorio en [a, b]
        "int" => {
            if args.len() < 2 { return Err("random.int requiere (a, b)".into()); }
            let a = to_i64(&args[0])?;
            let b = to_i64(&args[1])?;
            if a > b { return Err("random.int: a debe ser <= b".into()); }
            Ok(EvalValue::Int(rng.gen_range(a..=b)))
        }
        // float() → float en [0, 1)
        "float" => {
            Ok(EvalValue::Float(rng.gen::<f64>()))
        }
        // between(a, b) → float en [a, b)
        "between" => {
            if args.len() < 2 { return Err("random.between requiere (a, b)".into()); }
            let a = to_f64(&args[0])?;
            let b = to_f64(&args[1])?;
            Ok(EvalValue::Float(rng.gen_range(a..b)))
        }
        // choice(list) → elemento aleatorio
        "choice" => {
            let v = one_arg("choice", args)?;
            match v {
                EvalValue::List(list) => {
                    if list.is_empty() { return Err("random.choice: lista vacía".into()); }
                    let idx = rng.gen_range(0..list.len());
                    Ok(list[idx].clone())
                }
                EvalValue::Str(s) => {
                    let chars: Vec<char> = s.chars().collect();
                    if chars.is_empty() { return Err("random.choice: string vacío".into()); }
                    let idx = rng.gen_range(0..chars.len());
                    Ok(EvalValue::Str(chars[idx].to_string()))
                }
                _ => Err("random.choice requiere una lista o string".into()),
            }
        }
        // shuffle(list) → lista mezclada
        "shuffle" => {
            let v = one_arg("shuffle", args)?;
            match v {
                EvalValue::List(mut list) => {
                    list.shuffle(&mut rng);
                    Ok(EvalValue::List(list))
                }
                _ => Err("random.shuffle requiere una lista".into()),
            }
        }
        // sample(list, n) → n elementos aleatorios sin repetición
        "sample" => {
            if args.len() < 2 { return Err("random.sample requiere (list, n)".into()); }
            let list = match &args[0] {
                EvalValue::List(v) => v.clone(),
                _ => return Err("random.sample requiere una lista".into()),
            };
            let n = to_i64(&args[1])? as usize;
            if n > list.len() { return Err("random.sample: n mayor que la lista".into()); }
            let mut indices: Vec<usize> = (0..list.len()).collect();
            indices.shuffle(&mut rng);
            let selected: Vec<EvalValue> = indices[..n].iter().map(|&i| list[i].clone()).collect();
            Ok(EvalValue::List(selected))
        }
        // uuidv4() → string UUID v4
        "uuidv4" => {
            let bytes: [u8; 16] = rng.gen();
            let uuid = format!(
                "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
                u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
                u16::from_be_bytes([bytes[4], bytes[5]]),
                u16::from_be_bytes([bytes[6], bytes[7]]) & 0x0fff,
                (u16::from_be_bytes([bytes[8], bytes[9]]) & 0x3fff) | 0x8000,
                {
                    let high = (bytes[10] as u64) << 32;
                    let rest = u32::from_be_bytes([bytes[11], bytes[12], bytes[13], bytes[14]]) as u64;
                    high | rest
                }
            );
            Ok(EvalValue::Str(uuid))
        }
        // bool() → true/false aleatorio
        "bool" => {
            Ok(EvalValue::Bool(rng.gen::<bool>()))
        }

        f => Err(format!("random.{}() no existe", f)),
    }
}

fn one_arg(fn_name: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.is_empty() {
        return Err(format!("random.{}() requiere 1 argumento", fn_name));
    }
    Ok(args.into_iter().next().unwrap())
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("random: esperaba número, recibió {}", other.type_name())),
    }
}

fn to_f64(v: &EvalValue) -> Result<f64, String> {
    match v {
        EvalValue::Float(f) => Ok(*f),
        EvalValue::Int(n)   => Ok(*n as f64),
        other => Err(format!("random: esperaba número, recibió {}", other.type_name())),
    }
}

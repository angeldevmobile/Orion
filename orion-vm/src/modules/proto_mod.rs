use crate::eval_value::EvalValue;
use std::collections::HashMap;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // encode(value) → List de Int (bytes MessagePack)
        "encode" => {
            if args.is_empty() { return Err("proto.encode requiere (value)".into()); }
            let bytes = to_msgpack(&args[0])?;
            Ok(EvalValue::List(bytes.into_iter().map(|b| EvalValue::Int(b as i64)).collect()))
        }

        // decode(bytes) → EvalValue
        "decode" => {
            if args.is_empty() { return Err("proto.decode requiere (bytes)".into()); }
            let bytes = list_to_bytes(&args[0])?;
            from_msgpack(&bytes)
        }

        // encode_b64(value) → Str base64
        "encode_b64" => {
            if args.is_empty() { return Err("proto.encode_b64 requiere (value)".into()); }
            let bytes = to_msgpack(&args[0])?;
            Ok(EvalValue::Str(BASE64.encode(&bytes)))
        }

        // decode_b64(s) → EvalValue
        "decode_b64" => {
            if args.is_empty() { return Err("proto.decode_b64 requiere (base64_str)".into()); }
            let s     = to_str(&args[0]);
            let bytes = BASE64.decode(s.trim())
                .map_err(|e| format!("proto.decode_b64: base64 inválido — {}", e))?;
            from_msgpack(&bytes)
        }

        // size(value) → Int (tamaño en bytes del encoding MessagePack)
        "size" => {
            if args.is_empty() { return Err("proto.size requiere (value)".into()); }
            let bytes = to_msgpack(&args[0])?;
            Ok(EvalValue::Int(bytes.len() as i64))
        }

        // json_size(value) → Int (tamaño en bytes como JSON, para comparar)
        "json_size" => {
            if args.is_empty() { return Err("proto.json_size requiere (value)".into()); }
            let json = eval_to_json_str(&args[0]);
            Ok(EvalValue::Int(json.len() as i64))
        }

        f => Err(format!("proto.{}() no existe", f)),
    }
}

//    MessagePack encoder nativo                                                 
// Implementación directa del formato MessagePack (spec: https://msgpack.org)
// sin dependencias externas: cubre nil, bool, int, float, str, array, map.

fn to_msgpack(v: &EvalValue) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    encode_value(v, &mut buf)?;
    Ok(buf)
}

fn encode_value(v: &EvalValue, buf: &mut Vec<u8>) -> Result<(), String> {
    match v {
        EvalValue::Null => {
            buf.push(0xc0); // nil
        }
        EvalValue::Bool(true)  => { buf.push(0xc3); } // true
        EvalValue::Bool(false) => { buf.push(0xc2); } // false

        EvalValue::Int(n) => encode_int(*n, buf),

        EvalValue::Float(f) => {
            buf.push(0xcb); // float64
            buf.extend_from_slice(&f.to_bits().to_be_bytes());
        }

        EvalValue::Str(s) => {
            encode_str(s, buf)?;
        }

        EvalValue::List(items) => {
            let len = items.len();
            if len <= 15 {
                buf.push(0x90 | len as u8);
            } else if len <= 0xFFFF {
                buf.push(0xdc);
                buf.extend_from_slice(&(len as u16).to_be_bytes());
            } else {
                buf.push(0xdd);
                buf.extend_from_slice(&(len as u32).to_be_bytes());
            }
            for item in items { encode_value(item, buf)?; }
        }

        EvalValue::Dict(m) => {
            let len = m.len();
            if len <= 15 {
                buf.push(0x80 | len as u8);
            } else if len <= 0xFFFF {
                buf.push(0xde);
                buf.extend_from_slice(&(len as u16).to_be_bytes());
            } else {
                buf.push(0xdf);
                buf.extend_from_slice(&(len as u32).to_be_bytes());
            }
            for (k, val) in m {
                encode_str(k, buf)?;
                encode_value(val, buf)?;
            }
        }

        // Tipos no serializables → null
        _ => { buf.push(0xc0); }
    }
    Ok(())
}

fn encode_int(n: i64, buf: &mut Vec<u8>) {
    if n >= 0 {
        let u = n as u64;
        if u <= 0x7f        { buf.push(u as u8); }
        else if u <= 0xff   { buf.push(0xcc); buf.push(u as u8); }
        else if u <= 0xffff { buf.push(0xcd); buf.extend_from_slice(&(u as u16).to_be_bytes()); }
        else if u <= 0xffff_ffff { buf.push(0xce); buf.extend_from_slice(&(u as u32).to_be_bytes()); }
        else                { buf.push(0xcf); buf.extend_from_slice(&u.to_be_bytes()); }
    } else {
        if n >= -32         { buf.push((n as i8) as u8); }          // negative fixint
        else if n >= -128   { buf.push(0xd0); buf.push(n as i8 as u8); }
        else if n >= -32768 { buf.push(0xd1); buf.extend_from_slice(&(n as i16).to_be_bytes()); }
        else if n >= -(1 << 31) { buf.push(0xd2); buf.extend_from_slice(&(n as i32).to_be_bytes()); }
        else                { buf.push(0xd3); buf.extend_from_slice(&n.to_be_bytes()); }
    }
}

fn encode_str(s: &str, buf: &mut Vec<u8>) -> Result<(), String> {
    let b = s.as_bytes();
    let len = b.len();
    if len <= 31 {
        buf.push(0xa0 | len as u8);
    } else if len <= 0xff {
        buf.push(0xd9); buf.push(len as u8);
    } else if len <= 0xffff {
        buf.push(0xda); buf.extend_from_slice(&(len as u16).to_be_bytes());
    } else if len <= 0xffff_ffff {
        buf.push(0xdb); buf.extend_from_slice(&(len as u32).to_be_bytes());
    } else {
        return Err("proto.encode: string demasiado largo".into());
    }
    buf.extend_from_slice(b);
    Ok(())
}

//    MessagePack decoder nativo                                                 

fn from_msgpack(bytes: &[u8]) -> Result<EvalValue, String> {
    let (val, _) = decode_value(bytes, 0)
        .ok_or_else(|| "proto.decode: bytes MessagePack inválidos".to_string())?;
    Ok(val)
}

fn decode_value(buf: &[u8], pos: usize) -> Option<(EvalValue, usize)> {
    let b = *buf.get(pos)?;

    // positive fixint 0x00–0x7f
    if b <= 0x7f { return Some((EvalValue::Int(b as i64), pos + 1)); }
    // fixmap 0x80–0x8f
    if b & 0xf0 == 0x80 { return decode_map(buf, pos + 1, (b & 0x0f) as usize); }
    // fixarray 0x90–0x9f
    if b & 0xf0 == 0x90 { return decode_array(buf, pos + 1, (b & 0x0f) as usize); }
    // fixstr 0xa0–0xbf
    if b & 0xe0 == 0xa0 {
        let len = (b & 0x1f) as usize;
        return decode_str_bytes(buf, pos + 1, len);
    }
    // negative fixint 0xe0–0xff
    if b >= 0xe0 { return Some((EvalValue::Int((b as i8) as i64), pos + 1)); }

    match b {
        0xc0 => Some((EvalValue::Null, pos + 1)),
        0xc2 => Some((EvalValue::Bool(false), pos + 1)),
        0xc3 => Some((EvalValue::Bool(true), pos + 1)),
        // uint
        0xcc => read_u8(buf, pos + 1).map(|(v, p)| (EvalValue::Int(v as i64), p)),
        0xcd => read_u16(buf, pos + 1).map(|(v, p)| (EvalValue::Int(v as i64), p)),
        0xce => read_u32(buf, pos + 1).map(|(v, p)| (EvalValue::Int(v as i64), p)),
        0xcf => read_u64(buf, pos + 1),
        // int
        0xd0 => read_i8(buf, pos + 1).map(|(v, p)| (EvalValue::Int(v as i64), p)),
        0xd1 => read_i16(buf, pos + 1).map(|(v, p)| (EvalValue::Int(v as i64), p)),
        0xd2 => read_i32(buf, pos + 1).map(|(v, p)| (EvalValue::Int(v as i64), p)),
        0xd3 => read_i64(buf, pos + 1),
        // float
        0xca => read_f32(buf, pos + 1),
        0xcb => read_f64(buf, pos + 1),
        // str
        0xd9 => { let (len, p) = read_u8(buf, pos + 1)?;  decode_str_bytes(buf, p, len as usize) }
        0xda => { let (len, p) = read_u16(buf, pos + 1)?; decode_str_bytes(buf, p, len as usize) }
        0xdb => { let (len, p) = read_u32(buf, pos + 1)?; decode_str_bytes(buf, p, len as usize) }
        // bin (treat as list of ints)
        0xc4 => { let (len, p) = read_u8(buf, pos + 1)?;  decode_bin(buf, p, len as usize) }
        0xc5 => { let (len, p) = read_u16(buf, pos + 1)?; decode_bin(buf, p, len as usize) }
        0xc6 => { let (len, p) = read_u32(buf, pos + 1)?; decode_bin(buf, p, len as usize) }
        // array
        0xdc => { let (len, p) = read_u16(buf, pos + 1)?; decode_array(buf, p, len as usize) }
        0xdd => { let (len, p) = read_u32(buf, pos + 1)?; decode_array(buf, p, len as usize) }
        // map
        0xde => { let (len, p) = read_u16(buf, pos + 1)?; decode_map(buf, p, len as usize) }
        0xdf => { let (len, p) = read_u32(buf, pos + 1)?; decode_map(buf, p, len as usize) }
        _ => None,
    }
}

fn decode_str_bytes(buf: &[u8], pos: usize, len: usize) -> Option<(EvalValue, usize)> {
    let end = pos + len;
    let s = std::str::from_utf8(buf.get(pos..end)?).ok()?;
    Some((EvalValue::Str(s.to_string()), end))
}

fn decode_bin(buf: &[u8], pos: usize, len: usize) -> Option<(EvalValue, usize)> {
    let end = pos + len;
    let bytes = buf.get(pos..end)?;
    let list  = bytes.iter().map(|b| EvalValue::Int(*b as i64)).collect();
    Some((EvalValue::List(list), end))
}

fn decode_array(buf: &[u8], mut pos: usize, len: usize) -> Option<(EvalValue, usize)> {
    let mut items = Vec::with_capacity(len);
    for _ in 0..len {
        let (v, p) = decode_value(buf, pos)?;
        items.push(v);
        pos = p;
    }
    Some((EvalValue::List(items), pos))
}

fn decode_map(buf: &[u8], mut pos: usize, len: usize) -> Option<(EvalValue, usize)> {
    let mut map = HashMap::new();
    for _ in 0..len {
        let (k, p) = decode_value(buf, pos)?;
        let (v, p) = decode_value(buf, p)?;
        let key = match k { EvalValue::Str(s) => s, other => format!("{}", other) };
        map.insert(key, v);
        pos = p;
    }
    Some((EvalValue::Dict(map), pos))
}

//    Read helpers                                                               

fn read_u8(buf: &[u8], pos: usize)  -> Option<(u8,  usize)> { Some((*buf.get(pos)?, pos + 1)) }
fn read_u16(buf: &[u8], pos: usize) -> Option<(u16, usize)> {
    Some((u16::from_be_bytes(buf.get(pos..pos+2)?.try_into().ok()?), pos + 2))
}
fn read_u32(buf: &[u8], pos: usize) -> Option<(u32, usize)> {
    Some((u32::from_be_bytes(buf.get(pos..pos+4)?.try_into().ok()?), pos + 4))
}
fn read_u64(buf: &[u8], pos: usize) -> Option<(EvalValue, usize)> {
    let v = u64::from_be_bytes(buf.get(pos..pos+8)?.try_into().ok()?);
    Some((EvalValue::Int(v as i64), pos + 8))
}
fn read_i8(buf: &[u8], pos: usize)  -> Option<(i8,  usize)> { Some((*buf.get(pos)? as i8, pos + 1)) }
fn read_i16(buf: &[u8], pos: usize) -> Option<(i16, usize)> {
    Some((i16::from_be_bytes(buf.get(pos..pos+2)?.try_into().ok()?), pos + 2))
}
fn read_i32(buf: &[u8], pos: usize) -> Option<(i32, usize)> {
    Some((i32::from_be_bytes(buf.get(pos..pos+4)?.try_into().ok()?), pos + 4))
}
fn read_i64(buf: &[u8], pos: usize) -> Option<(EvalValue, usize)> {
    let v = i64::from_be_bytes(buf.get(pos..pos+8)?.try_into().ok()?);
    Some((EvalValue::Int(v), pos + 8))
}
fn read_f32(buf: &[u8], pos: usize) -> Option<(EvalValue, usize)> {
    let bits = u32::from_be_bytes(buf.get(pos..pos+4)?.try_into().ok()?);
    Some((EvalValue::Float(f32::from_bits(bits) as f64), pos + 4))
}
fn read_f64(buf: &[u8], pos: usize) -> Option<(EvalValue, usize)> {
    let bits = u64::from_be_bytes(buf.get(pos..pos+8)?.try_into().ok()?);
    Some((EvalValue::Float(f64::from_bits(bits)), pos + 8))
}

//    Misc helpers                                                               

fn list_to_bytes(v: &EvalValue) -> Result<Vec<u8>, String> {
    match v {
        EvalValue::List(items) => items.iter().map(|item| {
            match item {
                EvalValue::Int(n) if *n >= 0 && *n <= 255 => Ok(*n as u8),
                _ => Err("proto.decode: cada byte debe ser un Int entre 0 y 255".into()),
            }
        }).collect(),
        EvalValue::Str(s) => BASE64.decode(s.trim())
            .map_err(|e| format!("proto.decode: string base64 inválido — {}", e)),
        _ => Err("proto.decode: argumento debe ser List de bytes o String base64".into()),
    }
}

fn eval_to_json_str(v: &EvalValue) -> String {
    match v {
        EvalValue::Dict(m) => {
            let pairs: Vec<String> = m.iter()
                .map(|(k, v)| format!("\"{}\":{}", k, eval_to_json_str(v)))
                .collect();
            format!("{{{}}}", pairs.join(","))
        }
        EvalValue::List(items) => {
            let elems: Vec<String> = items.iter().map(eval_to_json_str).collect();
            format!("[{}]", elems.join(","))
        }
        EvalValue::Str(s)   => format!("\"{}\"", s.replace('"', "\\\"")),
        EvalValue::Int(n)   => n.to_string(),
        EvalValue::Float(f) => f.to_string(),
        EvalValue::Bool(b)  => if *b { "true".into() } else { "false".into() },
        EvalValue::Null     => "null".into(),
        other               => format!("\"{}\"", other),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

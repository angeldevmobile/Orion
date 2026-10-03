use crate::eval_value::EvalValue;
use indexmap::IndexMap;
use lettre::{Message, SmtpTransport, Transport};
use lettre::transport::smtp::authentication::Credentials;
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // send(opciones) → Bool
        // Con un dict: servidor, puerto, seguridad ("tls" | "starttls" | "ninguna"),
        // usuario, clave, de, para (texto o lista), cc, bcc, asunto, texto, html,
        // adjuntos (rutas o {ruta|base64, nombre, tipo}) y timeout en segundos.
        // send(smtp, usuario, clave, de, para, asunto, cuerpo) → Bool (forma antigua)
        "send" | "enviar" => {
            if let Some(EvalValue::Dict(m)) = args.first() {
                return send_opts(m);
            }
            check_len(&args, 7, "mail.enviar requires (opciones) o (smtp, usuario, clave, de, para, asunto, cuerpo)")?;
            send_mail(&args, false)
        }
        // send_html(smtp, usuario, clave, de, para, asunto, html) → Bool
        "send_html" | "enviar_html" => {
            check_len(&args, 7, "mail.enviar_html requires (smtp, usuario, clave, de, para, asunto, html)")?;
            send_mail(&args, true)
        }
        f => Err(format!("mail.{}() does not exist", f)),
    }
}

fn send_mail(args: &[EvalValue], html: bool) -> Result<EvalValue, String> {
    let smtp    = to_str(&args[0]);
    let usuario = to_str(&args[1]);
    let clave   = to_str(&args[2]);
    let de      = to_str(&args[3]);
    let para    = to_str(&args[4]);
    let asunto  = to_str(&args[5]);
    let cuerpo  = to_str(&args[6]);

    let ct = if html { ContentType::TEXT_HTML } else { ContentType::TEXT_PLAIN };

    let email = Message::builder()
        .from(de.parse().map_err(|e| format!("mail: dirección 'de' inválida: {}", e))?)
        .to(para.parse().map_err(|e| format!("mail: dirección 'para' inválida: {}", e))?)
        .subject(asunto)
        .header(ct)
        .body(cuerpo)
        .map_err(|e| format!("mail: error construyendo mensaje: {}", e))?;

    let creds  = Credentials::new(usuario, clave);
    let mailer = SmtpTransport::relay(&smtp)
        .map_err(|e| format!("mail: could not connect a '{}': {}", smtp, e))?
        .credentials(creds)
        .build();

    mailer.send(&email).map_err(|e| format!("mail.enviar: {}", e))?;
    Ok(EvalValue::Bool(true))
}

//   mail.send(opciones)

/// El primer nombre que aparezca: cada opción vale en español y en inglés.
fn opt<'a>(m: &'a IndexMap<String, EvalValue>, nombres: &[&str]) -> Option<&'a EvalValue> {
    nombres.iter().find_map(|n| m.get(*n)).filter(|v| !matches!(v, EvalValue::Null))
}

fn opt_str(m: &IndexMap<String, EvalValue>, nombres: &[&str]) -> Option<String> {
    opt(m, nombres).map(to_str).filter(|s| !s.trim().is_empty())
}

/// Una dirección o una lista de direcciones.
fn direcciones(v: Option<&EvalValue>, que: &str) -> Result<Vec<Mailbox>, String> {
    let lista: Vec<String> = match v {
        None => vec![],
        Some(EvalValue::List(l)) => l.iter().map(to_str).collect(),
        Some(otro) => vec![to_str(otro)],
    };
    lista.iter()
        .map(|d| d.trim().parse::<Mailbox>().map_err(|e| format!("mail: dirección '{}' en '{}' no válida: {}", d, que, e)))
        .collect()
}

fn mime_de(nombre: &str) -> &'static str {
    let ext = nombre.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "pdf"  => "application/pdf",
        "png"  => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif"  => "image/gif",
        "svg"  => "image/svg+xml",
        "csv"  => "text/csv",
        "txt"  => "text/plain",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "zip"  => "application/zip",
        _      => "application/octet-stream",
    }
}

/// Un adjunto: una ruta, o {ruta | base64, nombre?, tipo?}.
fn adjunto(v: &EvalValue) -> Result<SinglePart, String> {
    let (bytes, nombre, tipo) = match v {
        EvalValue::Dict(m) => {
            let (bytes, por_defecto) = if let Some(ruta) = opt_str(m, &["ruta", "path"]) {
                let b = std::fs::read(&ruta).map_err(|e| format!("mail: no se pudo leer el adjunto '{}': {}", ruta, e))?;
                let nombre = std::path::Path::new(&ruta).file_name()
                    .map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "adjunto".into());
                (b, nombre)
            } else if let Some(b64) = opt_str(m, &["base64", "contenido_b64"]) {
                use base64::Engine as _;
                let b = base64::engine::general_purpose::STANDARD.decode(b64.trim())
                    .map_err(|e| format!("mail: el adjunto en base64 no es válido: {}", e))?;
                (b, "adjunto".to_string())
            } else {
                return Err("mail: un adjunto necesita 'ruta' o 'base64'".into());
            };
            let nombre = opt_str(m, &["nombre", "name", "filename"]).unwrap_or(por_defecto);
            let tipo = opt_str(m, &["tipo", "type", "content_type"]).unwrap_or_else(|| mime_de(&nombre).into());
            (bytes, nombre, tipo)
        }
        otro => {
            let ruta = to_str(otro);
            let b = std::fs::read(&ruta).map_err(|e| format!("mail: no se pudo leer el adjunto '{}': {}", ruta, e))?;
            let nombre = std::path::Path::new(&ruta).file_name()
                .map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "adjunto".into());
            let tipo = mime_de(&nombre).to_string();
            (b, nombre, tipo)
        }
    };
    let ct = ContentType::parse(&tipo).map_err(|_| format!("mail: tipo de adjunto no válido: '{}'", tipo))?;
    Ok(Attachment::new(nombre).body(bytes, ct))
}

fn send_opts(m: &IndexMap<String, EvalValue>) -> Result<EvalValue, String> {
    let servidor = opt_str(m, &["servidor", "host", "smtp"])
        .ok_or("mail.send: falta 'servidor' (el host SMTP)")?;
    let seguridad = opt_str(m, &["seguridad", "security"]).unwrap_or_else(|| "starttls".into()).to_lowercase();
    let puerto_por_defecto = match seguridad.as_str() {
        "tls" | "ssl" => 465,
        "starttls" => 587,
        "ninguna" | "none" => 25,
        otra => return Err(format!("mail.send: seguridad '{}' no válida; valen tls, starttls o ninguna", otra)),
    };
    let puerto = match opt(m, &["puerto", "port"]) {
        Some(EvalValue::Int(n)) => *n as u16,
        Some(EvalValue::Float(f)) => *f as u16,
        Some(otro) => to_str(otro).trim().parse().map_err(|_| "mail.send: el puerto tiene que ser un número")?,
        None => puerto_por_defecto,
    };
    let timeout = match opt(m, &["timeout"]) {
        Some(EvalValue::Int(n)) => *n as f64,
        Some(EvalValue::Float(f)) => *f,
        _ => 30.0,
    };

    let de = direcciones(opt(m, &["de", "from"]), "de")?;
    let de = de.into_iter().next().ok_or("mail.send: falta 'de'")?;
    let para = direcciones(opt(m, &["para", "to"]), "para")?;
    let cc = direcciones(opt(m, &["cc"]), "cc")?;
    let bcc = direcciones(opt(m, &["bcc", "cco"]), "bcc")?;
    if para.is_empty() && cc.is_empty() && bcc.is_empty() {
        return Err("mail.send: falta 'para'".into());
    }

    let mut b = Message::builder().from(de)
        .subject(opt_str(m, &["asunto", "subject"]).unwrap_or_default());
    for d in para { b = b.to(d); }
    for d in cc { b = b.cc(d); }
    for d in bcc { b = b.bcc(d); }
    if let Some(r) = opt_str(m, &["responder_a", "reply_to"]) {
        b = b.reply_to(r.parse().map_err(|e| format!("mail.send: 'responder_a' no válida: {}", e))?);
    }

    // Texto y HTML a la vez van como alternativas: el cliente elige.
    let texto = opt_str(m, &["texto", "text", "cuerpo"]);
    let html = opt_str(m, &["html"]);
    let cuerpo = match (texto, html) {
        (Some(t), Some(h)) => MultiPart::alternative_plain_html(t, h),
        (Some(t), None) => MultiPart::mixed().singlepart(SinglePart::plain(t)),
        (None, Some(h)) => MultiPart::mixed().singlepart(SinglePart::html(h)),
        (None, None) => MultiPart::mixed().singlepart(SinglePart::plain(String::new())),
    };
    let adjuntos: Vec<SinglePart> = match opt(m, &["adjuntos", "attachments"]) {
        None => vec![],
        Some(EvalValue::List(l)) => l.iter().map(adjunto).collect::<Result<_, _>>()?,
        Some(uno) => vec![adjunto(uno)?],
    };
    let mensaje = if adjuntos.is_empty() {
        cuerpo
    } else {
        adjuntos.into_iter().fold(MultiPart::mixed().multipart(cuerpo), |mp, a| mp.singlepart(a))
    };
    let email = b.multipart(mensaje).map_err(|e| format!("mail.send: no se pudo armar el mensaje: {}", e))?;

    let constructor = match seguridad.as_str() {
        "tls" | "ssl" => SmtpTransport::relay(&servidor),
        "starttls" => SmtpTransport::starttls_relay(&servidor),
        _ => Ok(SmtpTransport::builder_dangerous(&servidor)),
    }.map_err(|e| format!("mail.send: no se pudo preparar '{}': {}", servidor, e))?;
    let mut constructor = constructor.port(puerto)
        .timeout(Some(std::time::Duration::from_secs_f64(timeout.max(0.1))));
    if let Some(u) = opt_str(m, &["usuario", "user"]) {
        constructor = constructor.credentials(Credentials::new(u, opt_str(m, &["clave", "password"]).unwrap_or_default()));
    }
    constructor.build().send(&email).map_err(|e| format!("mail.send: {}", e))?;
    Ok(EvalValue::Bool(true))
}

fn check_len(args: &[EvalValue], n: usize, msg: &str) -> Result<(), String> {
    if args.len() < n { Err(msg.into()) } else { Ok(()) }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

use crate::eval_value::EvalValue;
use lettre::{Message, SmtpTransport, Transport};
use lettre::transport::smtp::authentication::Credentials;
use lettre::message::header::ContentType;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // enviar(smtp, usuario, clave, de, para, asunto, cuerpo) → Bool
        "enviar" | "send" => {
            check_len(&args, 7, "mail.enviar requiere (smtp, usuario, clave, de, para, asunto, cuerpo)")?;
            send_mail(&args, false)
        }
        // enviar_html(smtp, usuario, clave, de, para, asunto, html) → Bool
        "enviar_html" | "send_html" => {
            check_len(&args, 7, "mail.enviar_html requiere (smtp, usuario, clave, de, para, asunto, html)")?;
            send_mail(&args, true)
        }
        f => Err(format!("mail.{}() no existe", f)),
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
        .map_err(|e| format!("mail: no se pudo conectar a '{}': {}", smtp, e))?
        .credentials(creds)
        .build();

    mailer.send(&email).map_err(|e| format!("mail.enviar: {}", e))?;
    Ok(EvalValue::Bool(true))
}

fn check_len(args: &[EvalValue], n: usize, msg: &str) -> Result<(), String> {
    if args.len() < n { Err(msg.into()) } else { Ok(()) }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

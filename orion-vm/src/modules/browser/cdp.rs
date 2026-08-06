//! Transporte del Chrome DevTools Protocol.
//!
//! CDP es WebSocket + JSON, así que no hace falta ningún cliente externo: basta
//! el `tungstenite` síncrono que Orion ya usa en `ws`. Lo único que hay que
//! resolver de verdad es el multiplexado.
//!
//! Sobre un único socket viajan mezcladas las respuestas a las peticiones
//! (llevan `id`) y los eventos del navegador (llevan `method`). Si dos tareas de
//! Orion comparten un navegador, hace falta alguien que reparta:
//!
//! ```text
//!   tarea A  ── id=7 ──┐                     ┌─► responses[7] ─► despierta A
//!                      ├─►  socket CDP  ─────┤
//!   tarea B  ── id=8 ──┘     (1 hilo lector) └─► responses[8] ─► despierta B
//!                                            └─► events[] ─────► quien espere
//! ```
//!
//! Un hilo lector por conexión desencola mensajes y deja cada respuesta donde
//! su emisor la espera; el emisor duerme en una `Condvar` en vez de girar en
//! vacío. Es el mismo patrón de parking que usa `await` en `task_pool`, así que
//! no se introduce un segundo modelo de concurrencia junto al que ya existe.
//!
//! El socket se pone en modo no bloqueante: así el hilo lector retiene el
//! candado solo microsegundos por sondeo y los emisores no se quedan esperando
//! detrás de una lectura bloqueada.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

/// Límites de recursos de la conexión.
///
/// Llegan desde `open()` en vez de estar fijados aquí: el historial de eventos
/// es RAM y el sondeo es CPU, y las dos cosas las nota quien ejecuta.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Tope de eventos retenidos. Un navegador activo emite miles por minuto y
    /// nadie los consume todos: sin tope, una sesión larga se come la RAM en un
    /// historial que no sirve para nada. Se descartan los más antiguos.
    pub max_events: usize,
    /// Techo del sondeo del hilo lector. Parte de cero mientras hay peticiones
    /// en vuelo (para no añadir latencia) y se relaja hasta aquí en reposo.
    pub idle_poll:  Duration,
    /// Plazo para que un envío por el socket progrese.
    pub send:       Duration,
    /// Cuánto se espera a que la página termine de cambiar de documento antes
    /// de dar por perdida una evaluación. Ver `reintentable`.
    pub nav_settle: Duration,
    /// Pausa entre reintentos de una evaluación.
    pub retry:      Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_events: 512,
            idle_poll:  Duration::from_millis(5),
            send:       Duration::from_secs(5),
            nav_settle: Duration::from_secs(5),
            retry:      Duration::from_millis(50),
        }
    }
}

/// ¿Este fallo es "la página está cambiando de documento ahora mismo"?
///
/// Es el fallo intermitente clásico de la automatización web. Al pulsar
/// "Enviar", el navegador tira el documento actual y monta el siguiente; entre
/// una cosa y la otra no hay ningún contexto donde evaluar JavaScript, y todo
/// lo que llegue en ese hueco muere con uno de estos mensajes.
///
/// Lo que lo hace tan traicionero es que depende del tiempo: en local la página
/// nueva llega antes de la siguiente instrucción y no pasa nada, y en el
/// servidor —o el día que la red va peor— revienta. Es el motivo de fondo de
/// que un scraper "funcione en mi máquina".
///
/// Reintentar es seguro precisamente porque el contexto ya no existía: la
/// evaluación **no llegó a ejecutarse**, así que no hay ningún efecto a medias
/// que repetir. Solo se aplica a `Runtime.evaluate`; un evento de ratón sí se
/// duplicaría, y un clic de más no es un reintento, es otra acción.
fn reintentable(method: &str, error: &str) -> bool {
    method == "Runtime.evaluate"
        && (error.contains("Inspected target navigated or closed")
            || error.contains("Execution context was destroyed")
            || error.contains("Cannot find context with specified id"))
}

#[derive(Debug, Clone)]
pub struct Event {
    pub seq:     u64,
    pub method:  String,
    pub session: Option<String>,
    pub params:  serde_json::Value,
}

#[derive(Default)]
struct State {
    responses: HashMap<u64, serde_json::Value>,
    events:    Vec<Event>,
    next_seq:  u64,
    /// Se rellena si el socket muere: convierte una espera eterna en un error.
    dead:      Option<String>,
}

pub struct Conn {
    socket:  Mutex<Socket>,
    state:   Mutex<State>,
    /// Política de diálogos por pestaña. Vive fuera de `state` para que el
    /// hilo lector pueda consultarla sin tomar el candado del estado.
    dialogs: Mutex<HashMap<String, String>>,
    /// Dominios a los que se permite ir. Vacía significa sin restricción.
    allow:   Mutex<Vec<String>>,
    /// URLs que se han cortado por no estar en la lista.
    blocked: Mutex<Vec<String>>,
    limits:  Limits,
    cv:      Condvar,
    next_id: AtomicU64,
    pending: AtomicU64,
    closed:  AtomicBool,
}

impl Conn {
    /// Conecta al endpoint CDP y arranca el hilo lector.
    pub fn connect(url: &str, limits: Limits) -> Result<Arc<Conn>, String> {
        let (mut socket, _) = tungstenite::connect(url)
            .map_err(|e| format!("no se pudo conectar a CDP en {url}: {e}"))?;

        // No bloqueante: el lector sondea y suelta el candado enseguida, así que
        // un `call` concurrente no espera detrás de una lectura parada.
        match socket.get_mut() {
            MaybeTlsStream::Plain(s) => s.set_nonblocking(true)
                .map_err(|e| format!("no se pudo poner el socket CDP en no bloqueante: {e}"))?,
            // CDP siempre es ws:// contra localhost; si algún día no lo fuera,
            // se sigue funcionando (con el lector bloqueándose más tiempo).
            _ => {}
        }

        let conn = Arc::new(Conn {
            socket:  Mutex::new(socket),
            state:   Mutex::new(State::default()),
            dialogs: Mutex::new(HashMap::new()),
            allow:   Mutex::new(Vec::new()),
            blocked: Mutex::new(Vec::new()),
            limits,
            cv:      Condvar::new(),
            next_id: AtomicU64::new(1),
            pending: AtomicU64::new(0),
            closed:  AtomicBool::new(false),
        });

        let lector = Arc::clone(&conn);
        thread::Builder::new()
            .name("orion-cdp".into())
            .spawn(move || lector.read_loop())
            .map_err(|e| format!("no se pudo arrancar el lector CDP: {e}"))?;

        Ok(conn)
    }

    /// Bucle del hilo lector: reparte respuestas y eventos hasta que se cierra.
    fn read_loop(self: Arc<Self>) {
        let mut espera = Duration::from_millis(0);

        while !self.closed.load(Ordering::Relaxed) {
            let leido = { self.socket.lock().unwrap().read() };

            match leido {
                Ok(Message::Text(txt)) => {
                    espera = Duration::from_millis(0);
                    self.dispatch(&txt);
                }
                Ok(Message::Close(_)) => {
                    self.die("el navegador cerró la conexión CDP");
                    return;
                }
                // Ping/Pong/Binary: irrelevantes para CDP.
                Ok(_) => espera = Duration::from_millis(0),

                Err(tungstenite::Error::Io(e))
                    if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    // Con peticiones en vuelo no se duerme: la respuesta puede
                    // llegar en cualquier momento y la latencia se nota.
                    if self.pending.load(Ordering::Relaxed) > 0 {
                        thread::yield_now();
                        espera = Duration::from_millis(0);
                    } else {
                        if !espera.is_zero() { thread::sleep(espera); }
                        espera = (espera + Duration::from_millis(1)).min(self.limits.idle_poll);
                    }
                }
                Err(e) => {
                    self.die(&format!("conexión CDP perdida: {e}"));
                    return;
                }
            }
        }
    }

    /// Fija qué hacer con los `alert`/`confirm`/`prompt` de una pestaña.
    ///
    /// Se declara una vez y vale para siempre, en vez de registrar un handler
    /// antes de cada acción que pudiera abrir uno. Un diálogo nativo **congela**
    /// la página hasta que alguien responde: si nadie lo hace, el síntoma es que
    /// todo deja de responder sin ningún error, que es lo peor que puede pasar.
    pub fn set_dialog_policy(&self, session: &str, politica: Option<String>) {
        let mut p = self.dialogs.lock().unwrap();
        match politica {
            Some(v) => { p.insert(session.to_string(), v); }
            None    => { p.remove(session); }
        }
    }

    /// Fija a qué dominios puede ir esta conexión. Lista vacía: sin límite.
    pub fn set_allowlist(&self, lista: Vec<String>) {
        *self.allow.lock().unwrap() = lista;
    }

    /// ¿Hay lista blanca puesta?
    pub fn hay_allowlist(&self) -> bool {
        !self.allow.lock().unwrap().is_empty()
    }

    /// Qué se ha bloqueado, para poder contarlo en un error o un diagnóstico.
    pub fn bloqueadas(&self) -> Vec<String> {
        self.blocked.lock().unwrap().clone()
    }

    /// Deja pasar o corta una petición interceptada.
    ///
    /// Lo contesta el hilo lector por el mismo motivo que los diálogos: con
    /// `Fetch` activo el navegador **para cada petición** hasta que alguien
    /// responde, así que si esto esperase su turno detrás de otra llamada, la
    /// página entera se quedaría congelada.
    fn answer_fetch(&self, session: Option<&str>, request_id: &str, url: &str) {
        let lista = self.allow.lock().unwrap().clone();
        let ok = super::state::permitida(url, &lista);
        if !ok {
            let mut b = self.blocked.lock().unwrap();
            // Acotado: una página que insista en pedir a un dominio prohibido
            // no debe hacer crecer esta lista sin fin.
            if b.len() < self.limits.max_events {
                b.push(url.to_string());
            }
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut msg = serde_json::json!({
            "id": id,
            "method": if ok { "Fetch.continueRequest" } else { "Fetch.failRequest" },
            "params": if ok {
                serde_json::json!({ "requestId": request_id })
            } else {
                serde_json::json!({ "requestId": request_id, "errorReason": "BlockedByClient" })
            },
        });
        if let Some(s) = session {
            msg["sessionId"] = serde_json::Value::String(s.to_string());
        }
        let _ = self.send_text(msg.to_string());
    }

    /// Responde a un diálogo sin esperar confirmación.
    ///
    /// Lo llama el hilo lector, que no puede bloquearse esperando una respuesta:
    /// si lo hiciera, dejaría de repartir mensajes y la respuesta que espera
    /// nunca llegaría.
    fn answer_dialog(&self, session: Option<&str>, politica: &str) {
        let (accept, texto) = match politica.split_once(':') {
            Some(("answer" | "responder", t)) => (true, Some(t.to_string())),
            _ => (matches!(politica, "accept" | "aceptar" | "yes" | "ok"), None),
        };
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut params = serde_json::json!({ "accept": accept });
        if let Some(t) = texto {
            params["promptText"] = serde_json::Value::String(t);
        }
        let mut msg = serde_json::json!({
            "id": id, "method": "Page.handleJavaScriptDialog", "params": params,
        });
        if let Some(s) = session {
            msg["sessionId"] = serde_json::Value::String(s.to_string());
        }
        let _ = self.send_text(msg.to_string());
    }

    /// Coloca un mensaje entrante donde corresponda y despierta a quien espere.
    fn dispatch(&self, txt: &str) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(txt) else { return };

        // Los diálogos se atienden aquí y no en quien llamó: el que abre el
        // diálogo puede ser un temporizador de la página, sin ninguna llamada
        // de Orion en curso a la que colgarlo.
        // Con `Fetch` activo el navegador PARA cada petición hasta que se le
        // contesta. Si esto se atendiera desde quien llamó, la página se
        // quedaría congelada esperando a un programa que a su vez espera a la
        // página. Se responde aquí, igual que los diálogos.
        if v.get("method").and_then(|m| m.as_str()) == Some("Fetch.requestPaused") {
            let ses = v.get("sessionId").and_then(|s| s.as_str());
            let id  = v.get("params").and_then(|p| p.get("requestId"))
                       .and_then(|x| x.as_str()).unwrap_or("");
            let url = v.get("params").and_then(|p| p.get("request"))
                       .and_then(|r| r.get("url")).and_then(|x| x.as_str()).unwrap_or("");
            if !id.is_empty() {
                self.answer_fetch(ses, id, url);
            }
        }

        if v.get("method").and_then(|m| m.as_str()) == Some("Page.javascriptDialogOpening") {
            let ses = v.get("sessionId").and_then(|s| s.as_str());
            let pol = {
                let p = self.dialogs.lock().unwrap();
                ses.and_then(|s| p.get(s).cloned())
            };
            if let Some(pol) = pol {
                // El candado del estado NO está tomado todavía: enviar desde
                // aquí no puede cruzarse con `dispatch`.
                self.answer_dialog(ses, &pol);
            }
        }

        let mut st = self.state.lock().unwrap();

        if let Some(id) = v.get("id").and_then(|x| x.as_u64()) {
            st.responses.insert(id, v);
        } else if let Some(method) = v.get("method").and_then(|x| x.as_str()) {
            let seq = st.next_seq;
            st.next_seq += 1;
            let ev = Event {
                seq,
                method:  method.to_string(),
                session: v.get("sessionId").and_then(|s| s.as_str()).map(str::to_string),
                params:  v.get("params").cloned().unwrap_or(serde_json::Value::Null),
            };
            st.events.push(ev);
            if st.events.len() > self.limits.max_events {
                let sobran = st.events.len() - self.limits.max_events;
                st.events.drain(..sobran);
            }
        }
        drop(st);
        self.cv.notify_all();
    }

    /// Marca la conexión como muerta y despierta a todos los que esperaban:
    /// sin esto, una caída del navegador dejaría hilos dormidos para siempre.
    fn die(&self, motivo: &str) {
        self.closed.store(true, Ordering::SeqCst);
        self.state.lock().unwrap().dead = Some(motivo.to_string());
        self.cv.notify_all();
    }

    /// Número de secuencia actual: se toma antes de provocar algo para después
    /// esperar solo los eventos posteriores y no uno viejo que ya estaba ahí.
    pub fn event_mark(&self) -> u64 {
        self.state.lock().unwrap().next_seq
    }

    /// Envía un comando y espera su respuesta.
    ///
    /// `session` es el `sessionId` de la pestaña; sin él, el comando va dirigido
    /// al navegador entero.
    pub fn call(
        &self,
        method: &str,
        params: serde_json::Value,
        session: Option<&str>,
        timeout: Duration,
    ) -> Result<serde_json::Value, String> {
        let limite = Instant::now() + self.limits.nav_settle;
        loop {
            let r = self.call_once(method, params.clone(), session, timeout);

            match r {
                Err(e) if reintentable(method, &e) => {
                    if Instant::now() >= limite {
                        // El mensaje crudo de CDP no le dice nada a quien
                        // escribió el programa: habla de un "inspected target"
                        // que él nunca ha visto.
                        return Err(format!(
                            "la página siguió cambiando de documento durante {} ms y no se pudo leer.\n  \
                             Suele ser una redirección en cadena; sube el plazo con \
                             open({{ nav_settle: ms }}) si el sitio es así de lento.",
                            self.limits.nav_settle.as_millis()
                        ));
                    }
                    thread::sleep(self.limits.retry);
                }
                otro => return otro,
            }
        }
    }

    fn call_once(
        &self,
        method: &str,
        params: serde_json::Value,
        session: Option<&str>,
        timeout: Duration,
    ) -> Result<serde_json::Value, String> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(self.death_reason());
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut msg = serde_json::json!({ "id": id, "method": method, "params": params });
        if let Some(s) = session {
            msg["sessionId"] = serde_json::Value::String(s.to_string());
        }

        self.pending.fetch_add(1, Ordering::SeqCst);
        let enviado = self.send_text(msg.to_string());
        let resultado = match enviado {
            Ok(()) => self.await_response(id, method, timeout),
            Err(e) => Err(e),
        };
        self.pending.fetch_sub(1, Ordering::SeqCst);
        resultado
    }

    fn send_text(&self, texto: String) -> Result<(), String> {
        let mut sock = self.socket.lock().unwrap();
        // En un socket no bloqueante el envío puede quedarse a medias; para los
        // mensajes diminutos de CDP contra localhost es rarísimo, pero
        // reintentar es más barato que perseguir un fallo intermitente.
        let limite = Instant::now() + self.limits.send;
        loop {
            match sock.send(Message::Text(texto.clone())) {
                Ok(()) => return Ok(()),
                Err(tungstenite::Error::Io(e))
                    if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    if Instant::now() > limite {
                        return Err(format!("CDP: el envío no progresó en {:?}", self.limits.send));
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                Err(e) => return Err(format!("CDP: no se pudo enviar '{}': {e}", texto.len())),
            }
        }
    }

    fn await_response(
        &self,
        id: u64,
        method: &str,
        timeout: Duration,
    ) -> Result<serde_json::Value, String> {
        let limite = Instant::now() + timeout;
        let mut st = self.state.lock().unwrap();

        loop {
            if let Some(resp) = st.responses.remove(&id) {
                if let Some(err) = resp.get("error") {
                    let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("error CDP");
                    let detalle = err.get("data").and_then(|d| d.as_str()).unwrap_or("");
                    return Err(if detalle.is_empty() {
                        format!("{method}: {msg}")
                    } else {
                        format!("{method}: {msg} ({detalle})")
                    });
                }
                return Ok(resp.get("result").cloned().unwrap_or(serde_json::Value::Null));
            }
            if let Some(motivo) = &st.dead {
                return Err(motivo.clone());
            }
            let restante = limite.saturating_duration_since(Instant::now());
            if restante.is_zero() {
                return Err(format!("{method}: sin respuesta en {} ms", timeout.as_millis()));
            }
            let (guard, _) = self.cv.wait_timeout(st, restante).unwrap();
            st = guard;
        }
    }

    /// Espera un evento posterior a `desde`, opcionalmente de una pestaña
    /// concreta. Devuelve `Ok(None)` si vence el plazo.
    pub fn wait_event(
        &self,
        method: &str,
        session: Option<&str>,
        desde: u64,
        timeout: Duration,
    ) -> Result<Option<Event>, String> {
        self.wait_event_where(method, session, desde, timeout, |_| true)
    }

    /// Como `wait_event`, pero además el evento tiene que cumplir `cond`.
    ///
    /// Hace falta porque hay flujos que emiten el mismo método varias veces y
    /// solo interesa uno: una descarga manda un `downloadProgress` por cada
    /// trozo recibido, y quedarse con el primero daría por terminada una
    /// descarga que acaba de empezar. Con el predicado, la espera se acaba
    /// cuando el estado dice `completed` y no cuando el nombre coincide.
    pub fn wait_event_where(
        &self,
        method: &str,
        session: Option<&str>,
        desde: u64,
        timeout: Duration,
        cond: impl Fn(&Event) -> bool,
    ) -> Result<Option<Event>, String> {
        let limite = Instant::now() + timeout;
        let mut st = self.state.lock().unwrap();

        loop {
            if let Some(ev) = st.events.iter().find(|e| {
                e.seq >= desde
                    && e.method == method
                    && match (session, &e.session) {
                        (None, _) => true,
                        (Some(s), Some(es)) => s == es,
                        (Some(_), None) => false,
                    }
                    && cond(e)
            }) {
                return Ok(Some(ev.clone()));
            }
            if let Some(motivo) = &st.dead {
                return Err(motivo.clone());
            }
            let restante = limite.saturating_duration_since(Instant::now());
            if restante.is_zero() { return Ok(None); }
            let (guard, _) = self.cv.wait_timeout(st, restante).unwrap();
            st = guard;
        }
    }

    fn death_reason(&self) -> String {
        self.state.lock().unwrap().dead.clone()
            .unwrap_or_else(|| "la conexión CDP está cerrada".into())
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let _ = self.socket.lock().unwrap().close(None);
        self.cv.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El reparto de mensajes no necesita un navegador: se puede comprobar
    /// contra una conexión construida a mano.
    fn conn_de_prueba() -> Conn {
        // Un socket real no hace falta para probar `dispatch`; se usa un
        // listener local para obtener un stream válido y nunca se lee de él.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let cliente = std::thread::spawn(move || TcpStream::connect(addr).unwrap());
        let _servidor = listener.accept().unwrap();
        let stream = cliente.join().unwrap();
        stream.set_nonblocking(true).unwrap();

        Conn {
            socket:  Mutex::new(WebSocket::from_raw_socket(
                         MaybeTlsStream::Plain(stream),
                         tungstenite::protocol::Role::Client,
                         None)),
            state:   Mutex::new(State::default()),
            dialogs: Mutex::new(HashMap::new()),
            allow:   Mutex::new(Vec::new()),
            blocked: Mutex::new(Vec::new()),
            limits:  Limits::default(),
            cv:      Condvar::new(),
            next_id: AtomicU64::new(1),
            pending: AtomicU64::new(0),
            closed:  AtomicBool::new(false),
        }
    }

    #[test]
    fn una_respuesta_va_a_su_peticion() {
        let c = conn_de_prueba();
        c.dispatch(r#"{"id":7,"result":{"value":"siete"}}"#);
        c.dispatch(r#"{"id":8,"result":{"value":"ocho"}}"#);

        let r8 = c.await_response(8, "X", Duration::from_millis(50)).unwrap();
        assert_eq!(r8["value"], "ocho");
        // La del 7 sigue disponible: no se pisan entre sí.
        let r7 = c.await_response(7, "X", Duration::from_millis(50)).unwrap();
        assert_eq!(r7["value"], "siete");
    }

    #[test]
    fn un_error_cdp_se_convierte_en_error_de_orion() {
        let c = conn_de_prueba();
        c.dispatch(r#"{"id":1,"error":{"message":"Cannot find context","data":"id 42"}}"#);
        let e = c.await_response(1, "Runtime.evaluate", Duration::from_millis(50)).unwrap_err();
        assert!(e.contains("Runtime.evaluate"), "{e}");
        assert!(e.contains("Cannot find context"), "{e}");
        assert!(e.contains("id 42"), "{e}");
    }

    #[test]
    fn sin_respuesta_vence_el_plazo_en_vez_de_colgarse() {
        let c = conn_de_prueba();
        let inicio = Instant::now();
        let e = c.await_response(99, "Page.navigate", Duration::from_millis(60)).unwrap_err();
        assert!(e.contains("sin respuesta"), "{e}");
        assert!(inicio.elapsed() < Duration::from_secs(2), "tardó demasiado en rendirse");
    }

    #[test]
    fn los_eventos_se_filtran_por_metodo_y_sesion() {
        let c = conn_de_prueba();
        let marca = c.event_mark();
        c.dispatch(r#"{"method":"Page.loadEventFired","sessionId":"A","params":{}}"#);
        c.dispatch(r#"{"method":"Page.loadEventFired","sessionId":"B","params":{}}"#);

        let a = c.wait_event("Page.loadEventFired", Some("A"), marca, Duration::from_millis(50))
                 .unwrap().expect("evento de A");
        assert_eq!(a.session.as_deref(), Some("A"));

        // Una sesión que no emitió nada no debe recoger el evento de otra.
        let c2 = c.wait_event("Page.loadEventFired", Some("C"), marca, Duration::from_millis(30)).unwrap();
        assert!(c2.is_none(), "se coló un evento de otra pestaña");
    }

    #[test]
    fn la_marca_descarta_eventos_anteriores() {
        let c = conn_de_prueba();
        c.dispatch(r#"{"method":"Page.loadEventFired","params":{}}"#);
        // Marca tomada DESPUÉS del evento: no debe verlo.
        let marca = c.event_mark();
        let visto = c.wait_event("Page.loadEventFired", None, marca, Duration::from_millis(30)).unwrap();
        assert!(visto.is_none(), "se devolvió un evento anterior a la marca");
    }

    #[test]
    fn el_historial_de_eventos_esta_acotado() {
        let c = conn_de_prueba();
        for _ in 0..(Limits::default().max_events + 200) {
            c.dispatch(r#"{"method":"Network.dataReceived","params":{}}"#);
        }
        assert_eq!(c.state.lock().unwrap().events.len(), Limits::default().max_events);
    }

    #[test]
    fn se_reintenta_solo_lo_que_es_seguro_reintentar() {
        // El hueco entre documentos, en sus tres redacciones.
        for e in [
            "Runtime.evaluate: Inspected target navigated or closed",
            "Runtime.evaluate: Execution context was destroyed.",
            "Runtime.evaluate: Cannot find context with specified id",
        ] {
            assert!(reintentable("Runtime.evaluate", e), "debería reintentarse: {e}");
        }

        // Un error de verdad no se reintenta: repetirlo solo retrasa el
        // diagnóstico el tiempo del plazo entero.
        assert!(!reintentable("Runtime.evaluate", "Runtime.evaluate: SyntaxError"));

        // Y sobre todo: un clic NO se repite. Duplicarlo no es reintentar una
        // lectura, es comprar dos veces.
        assert!(!reintentable(
            "Input.dispatchMouseEvent",
            "Input.dispatchMouseEvent: Inspected target navigated or closed"
        ));
    }

    #[test]
    fn una_conexion_muerta_despierta_a_quien_espera() {
        let c = Arc::new(conn_de_prueba());
        let esperando = Arc::clone(&c);
        let h = thread::spawn(move || {
            esperando.await_response(1, "X", Duration::from_secs(30))
        });
        thread::sleep(Duration::from_millis(30));
        c.die("el navegador se cerró");

        let e = h.join().unwrap().unwrap_err();
        assert!(e.contains("se cerró"), "{e}");
    }
}

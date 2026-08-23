//! Pool de hilos cacheado para tareas `spawn` / `async fn`.
//!
//! Antes, cada `spawn` hacía `std::thread::spawn` directo: N tareas = N hilos de
//! SO, sin reutilización. Este pool reutiliza hilos ociosos y solo crea uno nuevo
//! cuando *todos* están ocupados, de modo que:
//!
//!   - Una ráfaga de miles de `spawn` cortos reutiliza un puñado de hilos.
//!   - Nunca hay deadlock: si una tarea hace `await` de otra y no queda ningún
//!     worker libre, el pool arranca uno nuevo bajo demanda (pool "cacheado",
//!     no acotado por un tope fijo). Esto es lo que permite anidar spawn/await.
//!   - Los hilos ociosos se reciclan tras `IDLE_TIMEOUT` para no acumular hilos
//!     dormidos indefinidamente.
//!
//! Toda la coordinación va bajo un único `Mutex<State>` + `Condvar`, así la
//! decisión "¿notifico a un ocioso o creo un worker?" es atómica respecto al
//! encolado y no puede perder un trabajo por una carrera.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

/// Tiempo que un worker ocioso espera trabajo antes de retirarse.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

type Job = Box<dyn FnOnce() + Send + 'static>;

struct State {
    queue: VecDeque<Job>,
    /// Workers ociosos ahora mismo aparcados en el Condvar.
    idle:  usize,
    /// Workers vivos en total (ociosos + ocupados).
    total: usize,
}

struct PoolInner {
    state: Mutex<State>,
    cvar:  Condvar,
}

static POOL: OnceLock<Arc<PoolInner>> = OnceLock::new();

fn pool() -> &'static Arc<PoolInner> {
    POOL.get_or_init(|| {
        Arc::new(PoolInner {
            state: Mutex::new(State { queue: VecDeque::new(), idle: 0, total: 0 }),
            cvar:  Condvar::new(),
        })
    })
}

/// Encola un trabajo. Si hay un worker ocioso, lo despierta; si no, crea uno.
/// La decisión se toma con el lock tomado, así que ningún trabajo se pierde.
pub fn submit<F: FnOnce() + Send + 'static>(job: F) {
    let p = pool();
    let start_worker = {
        let mut st = p.state.lock().unwrap();
        st.queue.push_back(Box::new(job));
        if st.idle == 0 {
            // Nadie libre para tomarlo → habrá que arrancar un worker.
            st.total += 1;
            true
        } else {
            // Hay al menos un ocioso: despertar a uno.
            false
        }
    };
    if start_worker {
        spawn_worker(Arc::clone(p));
    } else {
        p.cvar.notify_one();
    }
}

fn spawn_worker(p: Arc<PoolInner>) {
    // `total` ya fue incrementado por el llamador con el lock tomado.
    std::thread::Builder::new()
        .name("orion-task".into())
        .spawn(move || worker_loop(p))
        .expect("could not create the task thread");
}

fn worker_loop(p: Arc<PoolInner>) {
    let mut st = p.state.lock().unwrap();
    loop {
        if let Some(job) = st.queue.pop_front() {
            // Ejecutar el trabajo SIN el lock (puede a su vez hacer spawn/await).
            drop(st);
            job();
            st = p.state.lock().unwrap();
            continue;
        }

        // Cola vacía: aparcar como ocioso con timeout.
        st.idle += 1;
        let (guard, timeout) = p.cvar.wait_timeout(st, IDLE_TIMEOUT).unwrap();
        st = guard;
        st.idle -= 1;

        if timeout.timed_out() && st.queue.is_empty() {
            // Ocioso demasiado tiempo: este worker se retira.
            st.total -= 1;
            return;
        }
        // Si no, reintenta el pop (llegó trabajo o fue una notificación espuria).
    }
}

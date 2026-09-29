//! Pool de hilos para `spawn` / `async fn`: reutiliza hilos ociosos, crea uno si
//! todos están ocupados (así un `await` anidado nunca se bloquea) y recicla los
//! ociosos tras `IDLE_TIMEOUT`.

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
    }
}

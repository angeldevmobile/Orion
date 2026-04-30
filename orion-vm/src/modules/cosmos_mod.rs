/// Orion Cosmos — simulación gravitacional en Rust puro.
/// Cuerpos como Dicts, universo como Dict con lista de cuerpos.
use crate::eval_value::EvalValue;
use std::collections::HashMap;
use rand::Rng;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // body(name, mass, x, y, z, vx, vy, vz) → dict body
        "body" => {
            let name = if args.is_empty() { "body".into() } else { to_str(&args[0]) };
            let mass = if args.len() > 1 { to_f64(&args[1])? } else { 1.0e24 };
            let x  = if args.len() > 2 { to_f64(&args[2])? } else { 0.0 };
            let y  = if args.len() > 3 { to_f64(&args[3])? } else { 0.0 };
            let z  = if args.len() > 4 { to_f64(&args[4])? } else { 0.0 };
            let vx = if args.len() > 5 { to_f64(&args[5])? } else { 0.0 };
            let vy = if args.len() > 6 { to_f64(&args[6])? } else { 0.0 };
            let vz = if args.len() > 7 { to_f64(&args[7])? } else { 0.0 };
            Ok(make_body(&name, mass, [x,y,z], [vx,vy,vz]))
        }
        // random_star() → body con masa y posición aleatorias
        "random_star" | "star" => {
            let mut rng = rand::thread_rng();
            let name = format!("Star_{}", rng.gen_range(1000..9999));
            let mass = rng.gen_range(1e20..1e30);
            let pos  = [rng.gen_range(-1e5..1e5), rng.gen_range(-1e5..1e5), rng.gen_range(-1e5..1e5)];
            let vel  = [rng.gen_range(-10.0..10.0), rng.gen_range(-10.0..10.0), rng.gen_range(-10.0..10.0)];
            Ok(make_body(&name, mass, pos, vel))
        }
        // create(n?) → universo con n cuerpos aleatorios
        "create" | "universe" => {
            let n = if args.is_empty() { 5 } else { to_i64(&args[0])? as usize };
            let mut rng = rand::thread_rng();
            let bodies: Vec<EvalValue> = (0..n).map(|i| {
                let name = format!("Star_{}", i + 1);
                let mass = rng.gen_range(1e20..1e30);
                let pos  = [rng.gen_range(-1e5..1e5f64), rng.gen_range(-1e5..1e5), rng.gen_range(-1e5..1e5)];
                let vel  = [rng.gen_range(-10.0..10.0f64), rng.gen_range(-10.0..10.0), rng.gen_range(-10.0..10.0)];
                make_body(&name, mass, pos, vel)
            }).collect();
            let mut m = HashMap::new();
            m.insert("bodies".into(), EvalValue::List(bodies));
            m.insert("time".into(),   EvalValue::Float(0.0));
            Ok(EvalValue::Dict(m))
        }
        // step(universe, dt?) → universo actualizado
        "step" => {
            if args.is_empty() { return Err("cosmos.step requiere (universe, dt?)".into()); }
            let dt = if args.len() > 1 { to_f64(&args[1])? } else { 1.0 };
            let universe = args[0].clone();
            step_universe(universe, dt)
        }
        // run(universe, steps?, dt?) → universo final
        "run" => {
            if args.is_empty() { return Err("cosmos.run requiere (universe, steps?, dt?)".into()); }
            let mut universe = args[0].clone();
            let steps = if args.len() > 1 { to_i64(&args[1])? as usize } else { 10 };
            let dt    = if args.len() > 2 { to_f64(&args[2])? } else { 1.0 };
            for _ in 0..steps {
                universe = step_universe(universe, dt)?;
            }
            Ok(universe)
        }
        // summary(universe) → {time, bodies_count, total_energy}
        "summary" => {
            if args.is_empty() { return Err("cosmos.summary requiere (universe)".into()); }
            universe_summary(&args[0])
        }
        // gravity(b1, b2, G?) → fuerza [fx, fy, fz]
        "gravity" => {
            if args.len() < 2 { return Err("cosmos.gravity requiere (b1, b2, G?)".into()); }
            let g_const = if args.len() > 2 { to_f64(&args[2])? } else { 6.674e-11 };
            let b1 = parse_body(&args[0])?;
            let b2 = parse_body(&args[1])?;
            let force = compute_gravity(&b1, &b2, g_const);
            Ok(EvalValue::List(force.iter().map(|&f| EvalValue::Float(f)).collect()))
        }
        // energy(universe) → {kinetic, potential, total}
        "energy" => {
            if args.is_empty() { return Err("cosmos.energy requiere (universe)".into()); }
            universe_energy(&args[0])
        }
        // distance(b1, b2) → f64
        "distance" => {
            if args.len() < 2 { return Err("cosmos.distance requiere (b1, b2)".into()); }
            let b1 = parse_body(&args[0])?;
            let b2 = parse_body(&args[1])?;
            let d  = body_distance(&b1, &b2);
            Ok(EvalValue::Float(d))
        }
        // stardust(n?) → lista de n puntos 3D aleatorios
        "stardust" | "dust" => {
            let n = if args.is_empty() { 100 } else { to_i64(&args[0])? as usize };
            let mut rng = rand::thread_rng();
            let dust: Vec<EvalValue> = (0..n).map(|_| {
                EvalValue::List(vec![
                    EvalValue::Float(rng.gen_range(-1.0..1.0)),
                    EvalValue::Float(rng.gen_range(-1.0..1.0)),
                    EvalValue::Float(rng.gen_range(-1.0..1.0)),
                ])
            }).collect();
            Ok(EvalValue::List(dust))
        }

        f => Err(format!("cosmos.{}() no existe", f)),
    }
}

// ─── Cuerpo ───────────────────────────────────────────────────────────────────

struct Body { name: String, mass: f64, pos: [f64;3], vel: [f64;3] }

fn make_body(name: &str, mass: f64, pos: [f64;3], vel: [f64;3]) -> EvalValue {
    let mut m = HashMap::new();
    m.insert("name".into(), EvalValue::Str(name.to_string()));
    m.insert("mass".into(), EvalValue::Float(mass));
    m.insert("x".into(),  EvalValue::Float(pos[0]));
    m.insert("y".into(),  EvalValue::Float(pos[1]));
    m.insert("z".into(),  EvalValue::Float(pos[2]));
    m.insert("vx".into(), EvalValue::Float(vel[0]));
    m.insert("vy".into(), EvalValue::Float(vel[1]));
    m.insert("vz".into(), EvalValue::Float(vel[2]));
    EvalValue::Dict(m)
}

fn parse_body(v: &EvalValue) -> Result<Body, String> {
    let EvalValue::Dict(m) = v else { return Err("cosmos: se esperaba un body (dict)".into()); };
    Ok(Body {
        name: m.get("name").map(|x| format!("{}", x)).unwrap_or_default(),
        mass: to_f64(m.get("mass").ok_or("cosmos: body sin campo 'mass'")?)?,
        pos: [
            to_f64(m.get("x").unwrap_or(&EvalValue::Float(0.0)))?,
            to_f64(m.get("y").unwrap_or(&EvalValue::Float(0.0)))?,
            to_f64(m.get("z").unwrap_or(&EvalValue::Float(0.0)))?,
        ],
        vel: [
            to_f64(m.get("vx").unwrap_or(&EvalValue::Float(0.0)))?,
            to_f64(m.get("vy").unwrap_or(&EvalValue::Float(0.0)))?,
            to_f64(m.get("vz").unwrap_or(&EvalValue::Float(0.0)))?,
        ],
    })
}

fn body_to_eval(b: &Body) -> EvalValue {
    make_body(&b.name, b.mass, b.pos, b.vel)
}

// ─── Física ───────────────────────────────────────────────────────────────────

fn body_distance(b1: &Body, b2: &Body) -> f64 {
    let d: f64 = (0..3).map(|i| (b1.pos[i] - b2.pos[i]).powi(2)).sum();
    d.sqrt()
}

fn compute_gravity(b1: &Body, b2: &Body, g: f64) -> [f64; 3] {
    let dist = body_distance(b1, b2);
    if dist < 1e-10 { return [0.0; 3]; }
    let f = g * b1.mass * b2.mass / (dist * dist);
    let mut force = [0.0f64; 3];
    for i in 0..3 { force[i] = f * (b2.pos[i] - b1.pos[i]) / dist; }
    force
}

fn step_universe(universe: EvalValue, dt: f64) -> Result<EvalValue, String> {
    let EvalValue::Dict(mut uni_map) = universe else {
        return Err("cosmos.step: se esperaba un universo (dict)".into());
    };
    let bodies_val = uni_map.get("bodies").cloned().ok_or("cosmos.step: universo sin 'bodies'")?;
    let EvalValue::List(body_vals) = bodies_val else {
        return Err("cosmos.step: 'bodies' debe ser una lista".into());
    };

    let mut bodies: Vec<Body> = body_vals.iter().map(parse_body).collect::<Result<_, _>>()?;
    let n = bodies.len();

    // Calcular fuerzas
    let mut forces = vec![[0.0f64; 3]; n];
    for i in 0..n {
        for j in (i+1)..n {
            let f = compute_gravity(&bodies[i], &bodies[j], 6.674e-11);
            for k in 0..3 {
                forces[i][k] += f[k];
                forces[j][k] -= f[k];
            }
        }
    }

    // Actualizar velocidades y posiciones
    for i in 0..n {
        for k in 0..3 {
            bodies[i].vel[k] += forces[i][k] / bodies[i].mass * dt;
            bodies[i].pos[k] += bodies[i].vel[k] * dt;
        }
    }

    let time = to_f64(uni_map.get("time").unwrap_or(&EvalValue::Float(0.0)))? + dt;
    uni_map.insert("bodies".into(), EvalValue::List(bodies.iter().map(body_to_eval).collect()));
    uni_map.insert("time".into(), EvalValue::Float(time));
    Ok(EvalValue::Dict(uni_map))
}

fn universe_summary(universe: &EvalValue) -> Result<EvalValue, String> {
    let EvalValue::Dict(m) = universe else { return Err("cosmos.summary: se esperaba un universo".into()); };
    let count = match m.get("bodies") {
        Some(EvalValue::List(v)) => v.len() as i64,
        _ => 0,
    };
    let time = to_f64(m.get("time").unwrap_or(&EvalValue::Float(0.0)))?;
    let mut result = HashMap::new();
    result.insert("time".into(),    EvalValue::Float(time));
    result.insert("bodies".into(),  EvalValue::Int(count));
    Ok(EvalValue::Dict(result))
}

fn universe_energy(universe: &EvalValue) -> Result<EvalValue, String> {
    let EvalValue::Dict(m) = universe else { return Err("cosmos.energy: se esperaba un universo".into()); };
    let bodies_val = m.get("bodies").ok_or("cosmos.energy: universo sin 'bodies'")?;
    let EvalValue::List(body_vals) = bodies_val else { return Err("cosmos.energy: 'bodies' debe ser lista".into()); };
    let bodies: Vec<Body> = body_vals.iter().map(parse_body).collect::<Result<_, _>>()?;

    let kinetic: f64 = bodies.iter().map(|b| {
        0.5 * b.mass * b.vel.iter().map(|v| v * v).sum::<f64>()
    }).sum();

    let mut potential = 0.0f64;
    for i in 0..bodies.len() {
        for j in (i+1)..bodies.len() {
            let r = body_distance(&bodies[i], &bodies[j]);
            if r > 1e-10 { potential -= 6.674e-11 * bodies[i].mass * bodies[j].mass / r; }
        }
    }

    let mut res = HashMap::new();
    res.insert("kinetic".into(),   EvalValue::Float(kinetic));
    res.insert("potential".into(), EvalValue::Float(potential));
    res.insert("total".into(),     EvalValue::Float(kinetic + potential));
    Ok(EvalValue::Dict(res))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn to_f64(v: &EvalValue) -> Result<f64, String> {
    match v {
        EvalValue::Float(f) => Ok(*f),
        EvalValue::Int(n)   => Ok(*n as f64),
        other => Err(format!("cosmos: esperaba número, recibió {}", other.type_name())),
    }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("cosmos: esperaba entero, recibió {}", other.type_name())),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

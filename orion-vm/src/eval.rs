/// Evaluador de árbol (tree-walker) para AST Orion serializado como JSON.
///
/// Recibe el AST que genera parser.py (serializado con json.dumps) y lo evalúa
/// directamente sin compilar a bytecode. Reemplaza eval.py en la ruta de
/// ejecución de Rust.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use serde_json::Value as Json;

use crate::eval_value::EvalValue;
use crate::env::Env;
use crate::builtins::{call_builtin, cmp_values};

//   Control de flujo interno                          

#[derive(Debug)]
pub enum Flow {
    Normal,
    Return(EvalValue),
    Break,
    Continue,
}

//   Punto de entrada                              

/// Ejecuta un programa completo (lista de sentencias JSON).
pub fn run_program(stmts: &[Json]) -> Result<(), String> {
    let mut env = Env::new();
    match eval_body(stmts, &mut env)? {
        Flow::Normal | Flow::Return(_) => Ok(()),
        Flow::Break    => Err("'break' fuera de un bucle".into()),
        Flow::Continue => Err("'continue' fuera de un bucle".into()),
    }
}

//   Bloque de sentencias                            

pub fn eval_body(stmts: &[Json], env: &mut Env) -> Result<Flow, String> {
    for stmt in stmts {
        match eval_stmt(stmt, env)? {
            Flow::Normal => {}
            other => return Ok(other),
        }
    }
    Ok(Flow::Normal)
}

//   Sentencia                                 

pub fn eval_stmt(node: &Json, env: &mut Env) -> Result<Flow, String> {
    let arr = match node {
        Json::Array(a) => a,
        // Sentencias que son expresiones directas (ej: number literal suelto)
        other => {
            eval_expr(other, env)?;
            return Ok(Flow::Normal);
        }
    };

    if arr.is_empty() {
        return Ok(Flow::Normal);
    }

    let tag = arr[0].as_str().unwrap_or("");

    match tag {
        //   Asignación
        "ASSIGN" | "CONST" => {
            let name = str_field(arr, 1, tag)?;
            let val  = eval_expr(&arr[2], env)?;
            env.set(name, val);
            Ok(Flow::Normal)
        }

        //   Asignación tipada: x: int = expr
        "TYPED_ASSIGN" => {
            // ["TYPED_ASSIGN", name, type_hint, expr]
            let name      = str_field(arr, 1, tag)?;
            let type_hint = arr.get(2).and_then(|v| v.as_str()).unwrap_or("any");
            let val       = eval_expr(&arr[3], env)?;
            check_type(&val, type_hint, name)?;
            env.set(name, val);
            Ok(Flow::Normal)
        }

        //   Asignación múltiple: a, b = [1, 2]
        "MULTI_ASSIGN" => {
            // ["MULTI_ASSIGN", [name1, name2, ...], expr]
            let names = arr[1].as_array().ok_or("MULTI_ASSIGN: lista de nombres inválida")?;
            let val   = eval_expr(&arr[2], env)?;
            match val {
                EvalValue::List(items) => {
                    for (name_json, item) in names.iter().zip(items.into_iter()) {
                        let name = name_json.as_str().ok_or("MULTI_ASSIGN: nombre inválido")?;
                        env.set(name, item);
                    }
                }
                other => {
                    // Si solo hay una variable, asignarla directamente
                    if names.len() == 1 {
                        let name = names[0].as_str().ok_or("MULTI_ASSIGN: nombre inválido")?;
                        env.set(name, other);
                    } else {
                        return Err(format!(
                            "MULTI_ASSIGN: se esperaba una lista con {} elementos, recibió {}",
                            names.len(), other.type_name()
                        ));
                    }
                }
            }
            Ok(Flow::Normal)
        }

        //   Asignación de índice: lista[i] = val             
        "SET_INDEX" => {
            // ["SET_INDEX", obj_expr, idx_expr, val_expr]
            let var_name = ident_name(&arr[1])
                .ok_or("SET_INDEX: el objeto debe ser una variable")?;
            let idx = eval_expr(&arr[2], env)?;
            let val = eval_expr(&arr[3], env)?;
            let mut obj = env.get(var_name)
                .ok_or_else(|| format!("Variable '{}' no definida", var_name))?;
            match (&mut obj, &idx) {
                (EvalValue::List(v), EvalValue::Int(i)) => {
                    let i = norm_index(*i, v.len())?;
                    v[i] = val;
                }
                (EvalValue::Dict(m), EvalValue::Str(k)) => {
                    m.insert(k.clone(), val);
                }
                _ => return Err(format!("SET_INDEX: tipos incompatibles")),
            }
            env.set(var_name, obj);
            Ok(Flow::Normal)
        }

        //   Asignación de atributo: obj.campo = val            
        "SET_ATTR" => {
            // ["SET_ATTR", obj_expr, attr_name, val_expr]
            let var_name = ident_name(&arr[1])
                .ok_or("SET_ATTR: el objeto debe ser una variable")?;
            let attr = str_field(arr, 2, tag)?;
            let val  = eval_expr(&arr[3], env)?;
            let mut obj = env.get(var_name)
                .ok_or_else(|| format!("Variable '{}' no definida", var_name))?;
            match &mut obj {
                EvalValue::Dict(m)                 => { m.insert(attr.to_string(), val); }
                EvalValue::Instance { fields, .. } => { fields.insert(attr.to_string(), val); }
                _ => return Err(format!("SET_ATTR: no se puede asignar atributo a {}", obj.type_name())),
            }
            env.set(var_name, obj);
            Ok(Flow::Normal)
        }

        //   IF / IF_ELSIF                         
        "IF" => {
            // ["IF", cond, body_true, body_false]
            let cond = eval_expr(&arr[1], env)?;
            let body = if cond.is_truthy() { &arr[2] } else { &arr[3] };
            if let Some(stmts) = body.as_array() {
                if !stmts.is_empty() {
                    env.push();
                    let r = eval_body(stmts, env);
                    env.pop();
                    return r;
                }
            }
            Ok(Flow::Normal)
        }

        "IF_ELSIF" => {
            // ["IF_ELSIF", cond, body_true, [[cond2, body2], ...], body_false]
            let cond = eval_expr(&arr[1], env)?;
            if cond.is_truthy() {
                return eval_block_node(&arr[2], env);
            }
            if let Some(branches) = arr[3].as_array() {
                for branch in branches {
                    if let Some(br) = branch.as_array() {
                        if br.len() >= 2 && eval_expr(&br[0], env)?.is_truthy() {
                            return eval_block_node(&br[1], env);
                        }
                    }
                }
            }
            eval_block_node(&arr[4], env)
        }

        //   WHILE                             
        "WHILE" => {
            // ["WHILE", cond, body]
            let body = arr[2].as_array().ok_or("WHILE: body inválido")?;
            loop {
                let cond = eval_expr(&arr[1], env)?;
                if !cond.is_truthy() { break; }
                env.push();
                let fc = eval_body(body, env)?;
                env.pop();
                match fc {
                    Flow::Break       => break,
                    Flow::Return(_)   => return Ok(fc),
                    Flow::Continue | Flow::Normal => {}
                }
            }
            Ok(Flow::Normal)
        }

        //   FOR IN (iterable)                       
        "FOR_IN" => {
            // ["FOR_IN", var_name_or_list, iter_expr, body]
            let body  = arr[3].as_array().ok_or("FOR_IN: body inválido")?;
            let iter  = eval_expr(&arr[2], env)?;
            let items = to_iter(iter)?;

            match &arr[1] {
                // Una sola variable: for x in lista
                Json::String(var) => {
                    for item in items {
                        env.push();
                        env.set_local(var, item);
                        let fc = eval_body(body, env)?;
                        env.pop();
                        match fc {
                            Flow::Break     => break,
                            Flow::Return(_) => return Ok(fc),
                            _               => {}
                        }
                    }
                }
                // Múltiples variables: for k, v in dict.items() (destructuring)
                Json::Array(vars) => {
                    for item in items {
                        env.push();
                        match item {
                            EvalValue::List(parts) => {
                                for (v, p) in vars.iter().zip(parts.into_iter()) {
                                    if let Some(vname) = v.as_str() {
                                        env.set_local(vname, p);
                                    }
                                }
                            }
                            other => {
                                if let Some(vname) = vars[0].as_str() {
                                    env.set_local(vname, other);
                                }
                            }
                        }
                        let fc = eval_body(body, env)?;
                        env.pop();
                        match fc {
                            Flow::Break     => break,
                            Flow::Return(_) => return Ok(fc),
                            _               => {}
                        }
                    }
                }
                _ => return Err("FOR_IN: variable de iteración inválida".into()),
            }
            Ok(Flow::Normal)
        }

        //   FOR RANGE                           
        "FOR_RANGE" => {
            // ["FOR_RANGE", var_name, start_expr, end_expr, body]
            let var   = str_field(arr, 1, tag)?;
            let start = eval_expr(&arr[2], env)?.to_i64()?;
            let end   = eval_expr(&arr[3], env)?.to_i64()?;
            let body  = arr[4].as_array().ok_or("FOR_RANGE: body inválido")?;
            for i in start..end {
                env.push();
                env.set_local(var, EvalValue::Int(i));
                let fc = eval_body(body, env)?;
                env.pop();
                match fc {
                    Flow::Break     => break,
                    Flow::Return(_) => return Ok(fc),
                    _               => {}
                }
            }
            Ok(Flow::Normal)
        }

        //   RETURN / BREAK / CONTINUE                   
        "RETURN" => {
            let val = if arr.len() > 1 && !arr[1].is_null() {
                eval_expr(&arr[1], env)?
            } else {
                EvalValue::Null
            };
            Ok(Flow::Return(val))
        }
        "BREAK"    => Ok(Flow::Break),
        "CONTINUE" => Ok(Flow::Continue),

        //   Definición de función                     
        // "FN"     → tag del parser: ["FN", name, params, body, ret_type, kwargs, line]
        // "FN_DEF" → tag del bytecode compiler (forma alternativa)
        "FN_DEF" | "FN" => {
            let name   = str_field(arr, 1, tag)?;
            let params = parse_params(&arr[2])?;
            let body   = arr[3].as_array().ok_or("FN: body inválido")?.to_vec();
            let closure = env.snapshot();
            env.set(name, EvalValue::Function {
                name:     name.to_string(),
                params,
                body,
                closure,
                is_async: false,
            });
            Ok(Flow::Normal)
        }

        //   Función async: async fn nombre(params) { body }
        "ASYNC_FN" => {
            // ["ASYNC_FN", name, params, body]
            let name    = str_field(arr, 1, tag)?;
            let params  = parse_params(&arr[2])?;
            let body    = arr[3].as_array().ok_or("ASYNC_FN: body inválido")?.to_vec();
            let closure = env.snapshot();
            env.set(name, EvalValue::Function {
                name:     name.to_string(),
                params,
                body,
                closure,
                is_async: true,
            });
            Ok(Flow::Normal)
        }

        //   AWAIT statement: await expr
        "AWAIT_STMT" => {
            // ["AWAIT_STMT", expr]
            let val = eval_expr(&arr[1], env)?;
            resolve_future(val)?;
            Ok(Flow::Normal)
        }

        //   SPAWN: fire-and-forget de una función async
        "SPAWN" => {
            // ["SPAWN", call_expr]
            eval_expr(&arr[1], env)?;  // evaluar lanza el thread; descartamos el Future
            Ok(Flow::Normal)
        }

        //   SERVE_STMT: servidor HTTP integrado
        "SERVE_STMT" => {
            // ["SERVE_STMT", port_expr, handler_expr_or_fn]
            let port = eval_expr(&arr[1], env)?.to_i64()?;

            // El handler puede ser un nodo FN inline — si es así, registrarlo primero
            let handler = if arr[2].as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .map(|t| t == "FN" || t == "FN_DEF")
                .unwrap_or(false)
            {
                eval_stmt(&arr[2], env)?;
                let fn_name = arr[2].as_array().unwrap()[1].as_str().unwrap_or("");
                env.get(fn_name).ok_or_else(|| format!("Handler '{}' no definido", fn_name))?
            } else {
                eval_expr(&arr[2], env)?
            };

            serve_http(port, handler, env)
        }

        //   THINK: think <expr>  →  llama al modelo AI con el prompt
        "THINK" => {
            // ["THINK", expr]
            let prompt = eval_expr(&arr[1], env)?;
            match crate::ai::think(&format!("{}", prompt)) {
                Ok(resp) => println!("{}", resp),
                Err(e)   => eprintln!("[think error] {}", e),
            }
            Ok(Flow::Normal)
        }

        //   LEARN: learn <expr>  →  guarda texto en memoria AI de sesión
        "LEARN" => {
            // ["LEARN", expr]
            let text = eval_expr(&arr[1], env)?;
            println!("{}", crate::ai::learn(&format!("{}", text)));
            Ok(Flow::Normal)
        }

        //   SENSE: sense <expr>  →  consulta la memoria AI con una query
        "SENSE" => {
            // ["SENSE", expr]
            let query = eval_expr(&arr[1], env)?;
            match crate::ai::sense(&format!("{}", query)) {
                Ok(resp) => println!("{}", resp),
                Err(e)   => eprintln!("[sense error] {}", e),
            }
            Ok(Flow::Normal)
        }

        //   ATTEMPT / HANDLE                        
        "ATTEMPT" => {
            // ["ATTEMPT", body_stmts, handler | null]
            let body = arr[1].as_array().ok_or("ATTEMPT: body inválido")?;
            env.push();
            let result = eval_body(body, env);
            env.pop();
            match result {
                Ok(fc) => Ok(fc),
                Err(e) => {
                    if arr.len() > 2 && arr[2].is_array() {
                        let handler = arr[2].as_array().unwrap();
                        // handler = ["HANDLE", err_var, handle_body]
                        let err_var   = handler.get(1).and_then(|v| v.as_str()).unwrap_or("_error");
                        let hbody     = handler.get(2).and_then(|v| v.as_array())
                            .ok_or("HANDLE: body inválido")?;
                        env.push();
                        env.set_local(err_var, EvalValue::Str(e));
                        let fc = eval_body(hbody, env)?;
                        env.pop();
                        Ok(fc)
                    } else {
                        Ok(Flow::Normal) // sin handler, silenciar
                    }
                }
            }
        }

        //   ERROR_STMT: lanza error explícito               
        "ERROR_STMT" => {
            let msg = eval_expr(&arr[1], env)?;
            Err(format!("{}", msg))
        }

        //   EXPR (expresión como sentencia)                
        "EXPR" => {
            eval_expr(&arr[1], env)?;
            Ok(Flow::Normal)
        }

        //   CALL como sentencia (ej: show "hola")             
        "CALL" => {
            eval_expr(node, env)?;
            Ok(Flow::Normal)
        }

        //   MATCH: match expr { pattern: { body } ... else: { body } }
        "MATCH" => {
            // ["MATCH", expr, [[pattern, body_stmts], ...]]
            let val   = eval_expr(&arr[1], env)?;
            let cases = arr[2].as_array().ok_or("MATCH: cases inválidos")?;
            let mut else_body: Option<&[Json]> = None;

            for case in cases {
                let pair = case.as_array().ok_or("MATCH: case inválido")?;
                let pattern_json = &pair[0];
                let body = pair[1].as_array().ok_or("MATCH: body inválido")?;

                if pattern_json.as_str() == Some("else") {
                    else_body = Some(body);
                    continue;
                }
                let pat_val = eval_expr(pattern_json, env)?;
                if values_equal(&val, &pat_val) {
                    env.push();
                    let r = eval_body(body, env);
                    env.pop();
                    return r;
                }
            }
            if let Some(body) = else_body {
                env.push();
                let r = eval_body(body, env);
                env.pop();
                return r;
            }
            Ok(Flow::Normal)
        }

        //   SHAPE_DEF: shape Nombre { field: default  act nombre(params) { body } }
        "SHAPE_DEF" => {
            // ["SHAPE_DEF", name, field_defs, on_create, acts, using_list]
            let name = str_field(arr, 1, tag)?;

            // Campos con valores por defecto
            let field_defs = arr[2].as_array().ok_or("SHAPE_DEF: fields inválidos")?;
            let mut fields: Vec<(String, EvalValue)> = Vec::new();
            for fd in field_defs {
                if let Some(pair) = fd.as_array() {
                    let fname = pair[0].as_str().ok_or("SHAPE_DEF: field name inválido")?;
                    // [name, default] o [name, type_hint, default]
                    let def_idx = if pair.len() >= 3 { 2 } else { 1 };
                    let default = if pair.get(def_idx).map(|v| v.is_null()).unwrap_or(true) {
                        EvalValue::Null
                    } else {
                        eval_expr(&pair[def_idx], env)?
                    };
                    fields.push((fname.to_string(), default));
                }
            }

            // on_create: [params_array, body_array, param_types] | null
            let on_create = if arr[3].is_null() {
                None
            } else if let Some(oc) = arr[3].as_array() {
                let params = parse_params(&oc[0])?;
                let body   = oc[1].as_array().ok_or("on_create: body inválido")?.to_vec();
                Some((params, body))
            } else {
                None
            };

            // Acts: [[name, params, body, ret_type, param_types], ...]
            let mut acts: std::collections::HashMap<String, (Vec<String>, Vec<Json>)> =
                std::collections::HashMap::new();
            if let Some(acts_arr) = arr[4].as_array() {
                for act in acts_arr {
                    if let Some(a) = act.as_array() {
                        let aname  = a[0].as_str().ok_or("ACT: nombre inválido")?;
                        let params = parse_params(&a[1])?;
                        let body   = a[2].as_array().ok_or("ACT: body inválido")?.to_vec();
                        acts.insert(aname.to_string(), (params, body));
                    }
                }
            }

            env.set(name, EvalValue::Shape { name: name.to_string(), fields, on_create, acts });
            Ok(Flow::Normal)
        }

        //   USE (módulos externos → no soportado en Rust aún)
        "USE" => {
            let module = arr[1].as_str().unwrap_or("?").trim_matches('"');
            Err(format!(
                "Módulo '{}' no soportado en el evaluador Rust. \
                 Usa `python orion {}` para módulos externos.",
                module, module
            ))
        }

        //   Fallback: intenta como expresión                
        _ => {
            eval_expr(node, env)?;
            Ok(Flow::Normal)
        }
    }
}

//   Expresión                                 

pub fn eval_expr(node: &Json, env: &mut Env) -> Result<EvalValue, String> {
    match node {
        // Primitivos del AST
        Json::Null        => Ok(EvalValue::Null),
        Json::Bool(b)     => Ok(EvalValue::Bool(*b)),
        Json::Number(n)   => {
            if let Some(i) = n.as_i64() { Ok(EvalValue::Int(i)) }
            else if let Some(f) = n.as_f64() { Ok(EvalValue::Float(f)) }
            else { Err(format!("Número inválido: {}", n)) }
        }
        Json::String(s) => {
            // Strings literales del parser incluyen comillas: '"hello"'
            if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                Ok(EvalValue::Str(s[1..s.len()-1].to_string()))
            } else {
                Ok(EvalValue::Str(s.clone()))
            }
        }

        Json::Array(arr) => {
            if arr.is_empty() {
                return Ok(EvalValue::Null);
            }
            let tag = arr[0].as_str().unwrap_or("");

            match tag {
                //   Identificador                     
                "IDENT" => {
                    let name = arr[1].as_str().ok_or("IDENT: nombre inválido")?;
                    env.get(name)
                       .ok_or_else(|| format!("Variable '{}' no definida", name))
                }

                //   Operación binaria                   
                "BINARY_OP" => {
                    // ["BINARY_OP", op, left, right]
                    let op    = arr[1].as_str().ok_or("BINARY_OP: op inválido")?;
                    let left  = eval_expr(&arr[2], env)?;
                    let right = eval_expr(&arr[3], env)?;
                    apply_binary(op, left, right)
                }

                //   Operación unaria                    
                "UNARY_OP" => {
                    let op  = arr[1].as_str().ok_or("UNARY_OP: op inválido")?;
                    let val = eval_expr(&arr[2], env)?;
                    match op {
                        "-" | "NEG" => match val {
                            EvalValue::Int(n)   => Ok(EvalValue::Int(-n)),
                            EvalValue::Float(f) => Ok(EvalValue::Float(-f)),
                            other => Err(format!("Negación no aplica a {}", other.type_name())),
                        },
                        "!" | "NOT" | "not" => Ok(EvalValue::Bool(!val.is_truthy())),
                        _ => Err(format!("Operador unario desconocido: {}", op)),
                    }
                }

                //   Lista literal                     
                "LIST" => {
                    let items = arr[1].as_array().ok_or("LIST: items inválidos")?;
                    let vals: Vec<EvalValue> = items.iter()
                        .map(|item| eval_expr(item, env))
                        .collect::<Result<_,_>>()?;
                    Ok(EvalValue::List(vals))
                }

                //   Dict literal                      
                "DICT" => {
                    // ["DICT", [[key, val_expr], ...]]
                    let pairs = arr[1].as_array().ok_or("DICT: pairs inválidos")?;
                    let mut map = HashMap::new();
                    for pair in pairs {
                        if let Some(p) = pair.as_array() {
                            let key = p[0].as_str().ok_or("DICT: clave inválida")?;
                            let val = eval_expr(&p[1], env)?;
                            map.insert(key.to_string(), val);
                        }
                    }
                    Ok(EvalValue::Dict(map))
                }

                //   Acceso a índice: obj[idx]               
                "INDEX" => {
                    let obj = eval_expr(&arr[1], env)?;
                    let idx = eval_expr(&arr[2], env)?;
                    index_access(obj, idx)
                }

                //   Acceso a atributo: obj.campo              
                "ATTR_ACCESS" => {
                    let obj  = eval_expr(&arr[1], env)?;
                    let attr = arr[2].as_str().ok_or("ATTR_ACCESS: attr inválido")?;
                    attr_access(obj, attr)
                }

                //   Llamada a función
                "CALL" => {
                    let fn_ref   = &arr[1];
                    let args_raw = arr[2].as_array().ok_or("CALL: args inválidos")?;
                    let mut args: Vec<EvalValue> = args_raw.iter()
                        .map(|a| eval_expr(a, env))
                        .collect::<Result<_,_>>()?;

                    // kwargs: arr[3] es un objeto JSON {"name": expr, ...}
                    let kwargs = arr.get(3)
                        .and_then(|v| v.as_object())
                        .map(|obj| obj.iter()
                            .map(|(k, v)| eval_expr(v, env).map(|ev| (k.clone(), ev)))
                            .collect::<Result<Vec<_>, _>>())
                        .transpose()?
                        .unwrap_or_default();

                    // Resolver nombre de función
                    let fn_name = match fn_ref {
                        Json::String(s)    => Some(s.as_str()),
                        Json::Array(a) if a.first().and_then(|v| v.as_str()) == Some("IDENT")
                            => a.get(1).and_then(|v| v.as_str()),
                        _ => None,
                    };

                    if let Some(name) = fn_name {
                        // Buscar built-in (solo args posicionales)
                        let bi = call_builtin(name, args.clone());
                        match bi {
                            Ok(v)  => return Ok(v),
                            Err(e) if !e.starts_with("__not_found__") => return Err(e),
                            _ => {}
                        }
                        // Buscar en env
                        if let Some(fn_val) = env.get(name) {
                            // Instanciar shape si es un constructor
                            if let EvalValue::Shape { .. } = &fn_val {
                                return instantiate_shape(fn_val, args, env);
                            }
                            return call_function(fn_val, args, kwargs, env);
                        }
                        return Err(format!("Función '{}' no definida", name));
                    }

                    // Expresión compleja como función
                    let fn_val = eval_expr(fn_ref, env)?;
                    if let EvalValue::Shape { .. } = &fn_val {
                        return instantiate_shape(fn_val, args, env);
                    }
                    call_function(fn_val, args, kwargs, env)
                }

                //   Llamada a método: obj.method(args)
                "CALL_METHOD" => {
                    // ["CALL_METHOD", method_name, obj_expr, args, kwargs]
                    let method   = arr[1].as_str().ok_or("CALL_METHOD: método inválido")?;
                    let args_raw = arr[3].as_array().ok_or("CALL_METHOD: args inválidos")?;
                    let args: Vec<EvalValue> = args_raw.iter()
                        .map(|a| eval_expr(a, env))
                        .collect::<Result<_,_>>()?;

                    // Caso especial: append muta la lista en el env
                    if method == "append" || method == "push" {
                        if let Some(var_name) = ident_name(&arr[2]) {
                            let item = args.into_iter().next()
                                .ok_or("append() requiere un argumento")?;
                            let mut list = env.get(var_name)
                                .ok_or_else(|| format!("Variable '{}' no definida", var_name))?;
                            if let EvalValue::List(ref mut v) = list {
                                v.push(item);
                            } else {
                                return Err(format!("append() solo aplica a listas, no a {}", list.type_name()));
                            }
                            env.set(var_name, list);
                            return Ok(EvalValue::Null);
                        }
                    }

                    // Caso especial: acto de instancia — necesita write-back de "me"
                    if let Some(var_name) = ident_name(&arr[2]) {
                        if let Some(inst) = env.get(var_name) {
                            if let EvalValue::Instance { ref acts, .. } = inst {
                                if let Some((params, body)) = acts.get(method).cloned() {
                                    let body_clone = body.clone();
                                    let mut fn_env = Env::from_snapshot(env.snapshot());
                                    fn_env.set_local("me", inst.clone());
                                    for (p, a) in params.iter().zip(args.into_iter()) {
                                        fn_env.set_local(p, a);
                                    }
                                    let result = eval_body(&body_clone, &mut fn_env)?;
                                    // Write-back mutations de me al env original
                                    if let Some(updated) = fn_env.get("me") {
                                        env.set(var_name, updated);
                                    }
                                    return match result {
                                        Flow::Return(v) => Ok(v),
                                        _               => Ok(EvalValue::Null),
                                    };
                                }
                            }
                        }
                    }

                    let obj = eval_expr(&arr[2], env)?;
                    call_method(obj, method, args)
                }

                //   Lambda / función anónima                
                "LAMBDA" => {
                    // ["LAMBDA", params, body]
                    let params  = parse_params(&arr[1])?;
                    let body    = arr[2].as_array().ok_or("LAMBDA: body inválido")?.to_vec();
                    let closure = env.snapshot();
                    Ok(EvalValue::Function {
                        name:     "<lambda>".into(),
                        params,
                        body,
                        closure,
                        is_async: false,
                    })
                }

                "ANON_FN" | "FN_DEF" => {
                    // ["ANON_FN"/"FN_DEF", name, params, body]
                    let name    = arr[1].as_str().unwrap_or("<anon>");
                    let params  = parse_params(&arr[2])?;
                    let body    = arr[3].as_array().ok_or("FN_DEF expr: body inválido")?.to_vec();
                    let closure = env.snapshot();
                    Ok(EvalValue::Function {
                        name:     name.to_string(),
                        params,
                        body,
                        closure,
                        is_async: false,
                    })
                }

                //   Await como expresión: await fn_async()
                "AWAIT_EXPR" => {
                    // ["AWAIT_EXPR", expr]
                    let val = eval_expr(&arr[1], env)?;
                    resolve_future(val)
                }

                //   Acceso nulo-seguro: obj?.campo
                "NULL_SAFE_ACCESS" | "NULL_SAFE" => {
                    let obj = eval_expr(&arr[1], env)?;
                    if matches!(obj, EvalValue::Null) {
                        return Ok(EvalValue::Null);
                    }
                    let attr = arr[2].as_str().ok_or("NULL_SAFE: attr inválido")?;
                    attr_access(obj, attr)
                }

                //   Slicing: obj[start:end] o obj[start:end:step]
                "SLICE_ACCESS" => {
                    // ["SLICE_ACCESS", obj_expr, ["SLICE", start, end, step]]
                    let obj  = eval_expr(&arr[1], env)?;
                    let slice = arr[2].as_array().ok_or("SLICE_ACCESS: slice inválido")?;
                    // slice = ["SLICE", start_expr, end_expr, step_expr]
                    let start_expr = slice.get(1).unwrap_or(&serde_json::Value::Null);
                    let end_expr   = slice.get(2).unwrap_or(&serde_json::Value::Null);

                    let start = if start_expr.is_null() { None }
                                else { Some(eval_expr(start_expr, env)?.to_i64()?) };
                    let end   = if end_expr.is_null() { None }
                                else { Some(eval_expr(end_expr, env)?.to_i64()?) };

                    slice_access(obj, start, end)
                }

                //   Check de instancia/tipo: expr is "type"
                "IS_CHECK" => {
                    // ["IS_CHECK", expr, type_name_str]
                    let val       = eval_expr(&arr[1], env)?;
                    let type_name = arr[2].as_str().ok_or("IS_CHECK: nombre de tipo inválido")?;
                    Ok(EvalValue::Bool(is_type_match(&val, type_name)))
                }

                //   Fallback: tratar como sentencia-expresión       
                _ => {
                    // Podría ser un ASSIGN u otro statement dentro de una expresión
                    match eval_stmt(node, env)? {
                        Flow::Return(v) => Ok(v),
                        _               => Ok(EvalValue::Null),
                    }
                }
            }
        }

        _ => Ok(EvalValue::Null),
    }
}

//   Operaciones binarias                            

fn apply_binary(op: &str, left: EvalValue, right: EvalValue) -> Result<EvalValue, String> {
    // Cortocircuito lógico
    match op {
        "&&" | "and" => return Ok(EvalValue::Bool(left.is_truthy() && right.is_truthy())),
        "||" | "or"  => return Ok(EvalValue::Bool(left.is_truthy() || right.is_truthy())),
        _ => {}
    }

    // Null-safe equality
    if matches!((&left, &right), (EvalValue::Null, _) | (_, EvalValue::Null)) {
        return match op {
            "==" => Ok(EvalValue::Bool(matches!((&left, &right), (EvalValue::Null, EvalValue::Null)))),
            "!=" => Ok(EvalValue::Bool(!matches!((&left, &right), (EvalValue::Null, EvalValue::Null)))),
            _    => Ok(EvalValue::Bool(false)),
        };
    }

    // Concatenación de strings con +
    if op == "+" {
        match (&left, &right) {
            (EvalValue::Str(a), _) =>
                return Ok(EvalValue::Str(format!("{}{}", a, right))),
            (_, EvalValue::Str(b)) =>
                return Ok(EvalValue::Str(format!("{}{}", left, b))),
            (EvalValue::List(a), EvalValue::List(b)) => {
                let mut v = a.clone();
                v.extend(b.iter().cloned());
                return Ok(EvalValue::List(v));
            }
            _ => {}
        }
    }

    // Comparaciones
    if matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=") {
        let ord = cmp_values(&left, &right)?;
        return Ok(EvalValue::Bool(match op {
            "==" => ord == 0,
            "!=" => ord != 0,
            "<"  => ord < 0,
            "<=" => ord <= 0,
            ">"  => ord > 0,
            ">=" => ord >= 0,
            _    => false,
        }));
    }

    // Aritmética numérica
    let use_float = matches!((&left, &right), (EvalValue::Float(_), _) | (_, EvalValue::Float(_)));
    if use_float {
        let l = left.to_f64()?;
        let r = right.to_f64()?;
        return match op {
            "+"  => Ok(EvalValue::Float(l + r)),
            "-"  => Ok(EvalValue::Float(l - r)),
            "*"  => Ok(EvalValue::Float(l * r)),
            "/"  => {
                if r == 0.0 { Err("División por cero".into()) }
                else { Ok(EvalValue::Float(l / r)) }
            }
            "%"  => Ok(EvalValue::Float(l % r)),
            "**" => Ok(EvalValue::Float(l.powf(r))),
            _    => Err(format!("Operador '{}' no soportado para float", op)),
        };
    }

    let l = left.to_i64()?;
    let r = right.to_i64()?;
    match op {
        "+"  => Ok(EvalValue::Int(l + r)),
        "-"  => Ok(EvalValue::Int(l - r)),
        "*"  => Ok(EvalValue::Int(l * r)),
        "/"  => {
            if r == 0 { Err("División por cero".into()) }
            else { Ok(EvalValue::Int(l / r)) }
        }
        "%"  => Ok(EvalValue::Int(l % r)),
        "**" => Ok(EvalValue::Int(l.pow(r as u32))),
        _    => Err(format!("Operador '{}' no conocido", op)),
    }
}

//   Llamada a función valor                          

fn instantiate_shape(shape: EvalValue, args: Vec<EvalValue>, env: &mut Env) -> Result<EvalValue, String> {
    match shape {
        EvalValue::Shape { name, fields, on_create, acts } => {
            // Valores por defecto de campos
            let mut instance_fields: HashMap<String, EvalValue> =
                fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

            if let Some((params, body)) = on_create {
                // Ejecutar on_create con "me" apuntando a la instancia inicial
                let mut fn_env = Env::from_snapshot(env.snapshot());
                let me = EvalValue::Instance {
                    shape_name: name.clone(),
                    fields:     instance_fields.clone(),
                    acts:       acts.clone(),
                };
                fn_env.set_local("me", me);
                for (p, a) in params.iter().zip(args.into_iter()) {
                    fn_env.set_local(p, a);
                }
                eval_body(&body, &mut fn_env)?;
                // Escribir de vuelta los campos desde "me"
                if let Some(EvalValue::Instance { fields: mf, .. }) = fn_env.get("me") {
                    instance_fields = mf;
                } else {
                    // Fallback: leer campos individuales
                    for (k, _) in &fields {
                        if let Some(v) = fn_env.get(k) {
                            instance_fields.insert(k.clone(), v);
                        }
                    }
                }
            } else {
                // Sin on_create: asignación posicional
                for ((k, _), a) in fields.iter().zip(args.into_iter()) {
                    instance_fields.insert(k.clone(), a);
                }
            }

            Ok(EvalValue::Instance { shape_name: name, fields: instance_fields, acts })
        }
        _ => Err("instantiate_shape: no es un Shape".into()),
    }
}

fn call_function(
    fn_val: EvalValue,
    args: Vec<EvalValue>,
    kwargs: Vec<(String, EvalValue)>,
    _caller_env: &mut Env,
) -> Result<EvalValue, String> {
    let fn_copy = fn_val.clone();
    match fn_val {
        EvalValue::Function { name, params, body, closure, is_async } => {
            // Construir argumentos combinando posicionales + kwargs
            let kw_map: HashMap<String, EvalValue> =
                kwargs.into_iter().collect();

            let mut bound: Vec<EvalValue> = Vec::with_capacity(params.len());
            let mut pos_iter = args.into_iter();
            for p in &params {
                if let Some(val) = pos_iter.next() {
                    bound.push(val);
                } else if let Some(val) = kw_map.get(p.as_str()) {
                    bound.push(val.clone());
                } else {
                    return Err(format!(
                        "Función '{}': argumento '{}' no fue proporcionado", name, p
                    ));
                }
            }

            if is_async {
                // Lanzar en thread y devolver Future
                let state: Arc<(Mutex<Option<Result<EvalValue, String>>>, Condvar)> =
                    Arc::new((Mutex::new(None), Condvar::new()));
                let state_clone = Arc::clone(&state);

                let mut fn_env = Env::from_snapshot(closure);
                if !name.starts_with('<') { fn_env.set_local(&name, fn_copy); }
                for (p, a) in params.iter().zip(bound.into_iter()) {
                    fn_env.set_local(p, a);
                }

                thread::spawn(move || {
                    let result = eval_body(&body, &mut fn_env)
                        .map(|fc| match fc { Flow::Return(v) => v, _ => EvalValue::Null });
                    let (lock, cvar) = &*state_clone;
                    *lock.lock().unwrap() = Some(result);
                    cvar.notify_one();
                });

                return Ok(EvalValue::Future(state));
            }

            // Función sincrónica normal
            let mut fn_env = Env::from_snapshot(closure);
            if !name.starts_with('<') { fn_env.set_local(&name, fn_copy); }
            for (p, a) in params.iter().zip(bound.into_iter()) {
                fn_env.set_local(p, a);
            }
            match eval_body(&body, &mut fn_env)? {
                Flow::Return(v) => Ok(v),
                _               => Ok(EvalValue::Null),
            }
        }
        other => Err(format!("'{}' no es una función", other.type_name())),
    }
}

//   Métodos de valores                             

fn call_method(obj: EvalValue, method: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match (&obj, method) {
        //   String methods                         
        (EvalValue::Str(s), "upper")  => Ok(EvalValue::Str(s.to_uppercase())),
        (EvalValue::Str(s), "lower")  => Ok(EvalValue::Str(s.to_lowercase())),
        (EvalValue::Str(s), "strip")  => Ok(EvalValue::Str(s.trim().to_string())),
        (EvalValue::Str(s), "len")    => Ok(EvalValue::Int(s.chars().count() as i64)),
        (EvalValue::Str(s), "reverse") =>
            Ok(EvalValue::Str(s.chars().rev().collect())),
        (EvalValue::Str(s), "split") => {
            let sep = args.first().map(|a| format!("{}", a)).unwrap_or_else(|| " ".into());
            Ok(EvalValue::List(s.split(&*sep).map(|p| EvalValue::Str(p.to_string())).collect()))
        }
        (EvalValue::Str(s), "contains") | (EvalValue::Str(s), "includes") => {
            let sub = args.first().map(|a| format!("{}", a)).unwrap_or_default();
            Ok(EvalValue::Bool(s.contains(&*sub)))
        }
        (EvalValue::Str(s), "starts_with") | (EvalValue::Str(s), "startsWith") => {
            let pre = args.first().map(|a| format!("{}", a)).unwrap_or_default();
            Ok(EvalValue::Bool(s.starts_with(&*pre)))
        }
        (EvalValue::Str(s), "ends_with") | (EvalValue::Str(s), "endsWith") => {
            let suf = args.first().map(|a| format!("{}", a)).unwrap_or_default();
            Ok(EvalValue::Bool(s.ends_with(&*suf)))
        }
        (EvalValue::Str(s), "replace") => {
            if args.len() < 2 { return Err("replace() requiere 2 argumentos".into()); }
            let from = format!("{}", args[0]);
            let to   = format!("{}", args[1]);
            Ok(EvalValue::Str(s.replace(&*from, &*to)))
        }
        (EvalValue::Str(s), "trim")  => Ok(EvalValue::Str(s.trim().to_string())),
        (EvalValue::Str(s), "chars") =>
            Ok(EvalValue::List(s.chars().map(|c| EvalValue::Str(c.to_string())).collect())),
        (EvalValue::Str(s), "to_int") | (EvalValue::Str(s), "toInt") =>
            s.trim().parse::<i64>().map(EvalValue::Int)
                .map_err(|_| format!("No se puede convertir '{}' a int", s)),
        (EvalValue::Str(s), "to_float") | (EvalValue::Str(s), "toFloat") =>
            s.trim().parse::<f64>().map(EvalValue::Float)
                .map_err(|_| format!("No se puede convertir '{}' a float", s)),

        //   List methods                          
        (EvalValue::List(v), "len")    => Ok(EvalValue::Int(v.len() as i64)),
        (EvalValue::List(v), "first")  => Ok(v.first().cloned().unwrap_or(EvalValue::Null)),
        (EvalValue::List(v), "last")   => Ok(v.last().cloned().unwrap_or(EvalValue::Null)),
        (EvalValue::List(v), "reverse") => {
            let mut r = v.clone(); r.reverse();
            Ok(EvalValue::List(r))
        }
        (EvalValue::List(v), "contains") | (EvalValue::List(v), "includes") => {
            let item = args.first().cloned().unwrap_or(EvalValue::Null);
            Ok(EvalValue::Bool(v.iter().any(|x| values_equal(x, &item))))
        }
        (EvalValue::List(v), "join") => {
            let sep = args.first().map(|a| format!("{}", a)).unwrap_or_default();
            let parts: Vec<String> = v.iter().map(|x| format!("{}", x)).collect();
            Ok(EvalValue::Str(parts.join(&sep)))
        }
        (EvalValue::List(v), "slice") => {
            let start = args.get(0).and_then(|a| a.to_i64().ok()).unwrap_or(0) as usize;
            let end   = args.get(1).and_then(|a| a.to_i64().ok())
                .map(|n| n as usize).unwrap_or(v.len());
            Ok(EvalValue::List(v[start.min(v.len())..end.min(v.len())].to_vec()))
        }

        //   Dict methods                          
        (EvalValue::Dict(m), "keys")   => Ok(EvalValue::List(m.keys().map(|k| EvalValue::Str(k.clone())).collect())),
        (EvalValue::Dict(m), "values") => Ok(EvalValue::List(m.values().cloned().collect())),
        (EvalValue::Dict(m), "len")    => Ok(EvalValue::Int(m.len() as i64)),
        (EvalValue::Dict(m), "get") => {
            let key     = args.first().map(|a| format!("{}", a)).unwrap_or_default();
            let default = args.get(1).cloned().unwrap_or(EvalValue::Null);
            Ok(m.get(&key).cloned().unwrap_or(default))
        }
        (EvalValue::Dict(m), "contains") | (EvalValue::Dict(m), "has") => {
            let key = args.first().map(|a| format!("{}", a)).unwrap_or_default();
            Ok(EvalValue::Bool(m.contains_key(&key)))
        }

        _ => Err(format!(
            "Método '{}' no disponible para {}",
            method, obj.type_name()
        )),
    }
}

//   Accesos                                  

fn index_access(obj: EvalValue, idx: EvalValue) -> Result<EvalValue, String> {
    match (&obj, &idx) {
        (EvalValue::List(v), EvalValue::Int(i)) => {
            let i = norm_index(*i, v.len())?;
            Ok(v[i].clone())
        }
        (EvalValue::Dict(m), EvalValue::Str(k)) => {
            m.get(k).cloned().ok_or_else(|| format!("Clave '{}' no encontrada", k))
        }
        (EvalValue::Str(s), EvalValue::Int(i)) => {
            let chars: Vec<char> = s.chars().collect();
            let i = norm_index(*i, chars.len())?;
            Ok(EvalValue::Str(chars[i].to_string()))
        }
        _ => Err(format!(
            "No se puede indexar {} con {}",
            obj.type_name(), idx.type_name()
        )),
    }
}

fn attr_access(obj: EvalValue, attr: &str) -> Result<EvalValue, String> {
    match &obj {
        EvalValue::Dict(m) => m.get(attr).cloned()
            .ok_or_else(|| format!("Clave '{}' no encontrada", attr)),
        EvalValue::Instance { fields, .. } => fields.get(attr).cloned()
            .ok_or_else(|| format!("Campo '{}' no encontrado en instancia", attr)),
        EvalValue::List(_) | EvalValue::Str(_) => {
            call_method(obj, attr, vec![])
        }
        _ => Err(format!(
            "No se puede acceder '{}' en {}",
            attr, obj.type_name()
        )),
    }
}

//   Utilidades                                 

fn eval_block_node(node: &Json, env: &mut Env) -> Result<Flow, String> {
    match node.as_array() {
        Some(stmts) if !stmts.is_empty() => {
            env.push();
            let r = eval_body(stmts, env);
            env.pop();
            r
        }
        _ => Ok(Flow::Normal),
    }
}

fn to_iter(val: EvalValue) -> Result<Vec<EvalValue>, String> {
    match val {
        EvalValue::List(v) => Ok(v),
        EvalValue::Str(s)  => Ok(s.chars().map(|c| EvalValue::Str(c.to_string())).collect()),
        EvalValue::Dict(m) => Ok(m.into_keys().map(EvalValue::Str).collect()),
        other => Err(format!("'{}' no es iterable", other.type_name())),
    }
}

fn parse_params(node: &Json) -> Result<Vec<String>, String> {
    node.as_array()
        .ok_or("Parámetros inválidos")?
        .iter()
        .map(|p| p.as_str().map(str::to_string).ok_or_else(|| "Parámetro inválido".into()))
        .collect()
}

fn str_field<'a>(arr: &'a [Json], idx: usize, ctx: &str) -> Result<&'a str, String> {
    arr.get(idx)
       .and_then(|v| v.as_str())
       .ok_or_else(|| format!("{}: campo {} debe ser string", ctx, idx))
}

fn ident_name(node: &Json) -> Option<&str> {
    node.as_array()
        .filter(|a| a.first().and_then(|v| v.as_str()) == Some("IDENT"))
        .and_then(|a| a.get(1))
        .and_then(|v| v.as_str())
}

fn norm_index(i: i64, len: usize) -> Result<usize, String> {
    let idx = if i < 0 { len as i64 + i } else { i } as usize;
    if idx >= len {
        Err(format!("Índice {} fuera de rango (len={})", i, len))
    } else {
        Ok(idx)
    }
}

fn values_equal(a: &EvalValue, b: &EvalValue) -> bool {
    match (a, b) {
        (EvalValue::Int(x),   EvalValue::Int(y))   => x == y,
        (EvalValue::Float(x), EvalValue::Float(y)) => x == y,
        (EvalValue::Str(x),   EvalValue::Str(y))   => x == y,
        (EvalValue::Bool(x),  EvalValue::Bool(y))  => x == y,
        (EvalValue::Null,     EvalValue::Null)      => true,
        _ => false,
    }
}

fn is_type_match(val: &EvalValue, type_name: &str) -> bool {
    // Primero verificar si es instancia de un shape con ese nombre
    if let EvalValue::Instance { shape_name, .. } = val {
        if shape_name == type_name { return true; }
    }
    let normalized = match type_name {
        "str" | "string"  => "string",
        "integer"         => "int",
        "boolean"         => "bool",
        "number"          => "number",
        other             => other,
    };
    match (val, normalized) {
        (EvalValue::Int(_),      "int")      => true,
        (EvalValue::Float(_),    "float")    => true,
        (EvalValue::Int(_),      "number")   => true,
        (EvalValue::Float(_),    "number")   => true,
        (EvalValue::Str(_),      "string")   => true,
        (EvalValue::Bool(_),     "bool")     => true,
        (EvalValue::List(_),     "list")     => true,
        (EvalValue::Dict(_),     "dict")     => true,
        (EvalValue::Null,        "null")     => true,
        (EvalValue::Instance{..},"instance") => true,
        _ => false,
    }
}

fn check_type(val: &EvalValue, type_hint: &str, name: &str) -> Result<(), String> {
    if type_hint == "any" || type_hint == "void" {
        return Ok(());
    }
    if !is_type_match(val, type_hint) {
        return Err(format!(
            "Error de tipo en '{}': se esperaba '{}', se recibió '{}'",
            name, type_hint, val.type_name()
        ));
    }
    Ok(())
}

fn slice_access(obj: EvalValue, start: Option<i64>, end: Option<i64>) -> Result<EvalValue, String> {
    match obj {
        EvalValue::List(v) => {
            let len = v.len() as i64;
            let s = resolve_slice_idx(start.unwrap_or(0), len);
            let e = resolve_slice_idx(end.unwrap_or(len), len);
            let s = s.min(v.len());
            let e = e.min(v.len());
            let e = e.max(s);
            Ok(EvalValue::List(v[s..e].to_vec()))
        }
        EvalValue::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            let len = chars.len() as i64;
            let si = resolve_slice_idx(start.unwrap_or(0), len);
            let ei = resolve_slice_idx(end.unwrap_or(len), len);
            let si = si.min(chars.len());
            let ei = ei.min(chars.len()).max(si);
            Ok(EvalValue::Str(chars[si..ei].iter().collect()))
        }
        other => Err(format!("No se puede hacer slice de {}", other.type_name())),
    }
}

fn resolve_slice_idx(i: i64, len: i64) -> usize {
    if i < 0 { (len + i).max(0) as usize }
    else      { i as usize }
}

/// Bloquea hasta que un Future se resuelve y devuelve su valor.
fn resolve_future(val: EvalValue) -> Result<EvalValue, String> {
    match val {
        EvalValue::Future(state) => {
            let (lock, cvar) = &*state;
            let mut guard = lock.lock().unwrap();
            while guard.is_none() {
                guard = cvar.wait(guard).unwrap();
            }
            guard.take().unwrap()
        }
        other => Ok(other), // valor ya resuelto, devolverlo directamente
    }
}

/// Servidor HTTP bloqueante usando tiny_http.
fn serve_http(port: i64, handler: EvalValue, env: &mut Env) -> Result<Flow, String> {

    let addr = format!("0.0.0.0:{}", port);
    let server = tiny_http::Server::http(&addr)
        .map_err(|e| format!("No se pudo iniciar servidor en {}: {}", addr, e))?;

    eprintln!("[Orion] Servidor HTTP en http://localhost:{} (Ctrl+C para detener)", port);

    loop {
        let mut request = match server.recv() {
            Ok(r)  => r,
            Err(_) => break,
        };

        let method = request.method().to_string();
        let url    = request.url().to_string();

        let (path, query) = match url.find('?') {
            Some(pos) => (url[..pos].to_string(), url[pos + 1..].to_string()),
            None      => (url.clone(), String::new()),
        };

        // Query params → Dict
        let mut params: HashMap<String, EvalValue> = HashMap::new();
        for pair in query.split('&').filter(|s| !s.is_empty()) {
            if let Some(eq) = pair.find('=') {
                params.insert(pair[..eq].to_string(),
                              EvalValue::Str(pair[eq + 1..].to_string()));
            }
        }

        // Leer body
        let mut body_bytes = Vec::new();
        request.as_reader().read_to_end(&mut body_bytes).ok();
        let body_str = String::from_utf8_lossy(&body_bytes).to_string();

        // Dict de request
        let mut req: HashMap<String, EvalValue> = HashMap::new();
        req.insert("path".into(),   EvalValue::Str(path));
        req.insert("method".into(), EvalValue::Str(method));
        req.insert("body".into(),   EvalValue::Str(body_str));
        req.insert("params".into(), EvalValue::Dict(params));

        // Llamar handler
        let resp_val = match call_function(handler.clone(), vec![EvalValue::Dict(req)], vec![], env) {
            Ok(v)  => v,
            Err(e) => {
                let mut err: HashMap<String, EvalValue> = HashMap::new();
                err.insert("status".into(), EvalValue::Int(500));
                err.insert("body".into(),   EvalValue::Str(e));
                EvalValue::Dict(err)
            }
        };

        // Parsear respuesta
        let (status_code, resp_body, content_type) = match &resp_val {
            EvalValue::Dict(m) => {
                let sc = m.get("status").and_then(|v| v.to_i64().ok()).unwrap_or(200) as u16;
                let bd = m.get("body").map(|v| format!("{}", v)).unwrap_or_default();
                let ct = m.get("content_type").map(|v| format!("{}", v))
                    .unwrap_or_else(|| "text/plain; charset=utf-8".into());
                (sc, bd, ct)
            }
            other => (200, format!("{}", other), "text/plain; charset=utf-8".into()),
        };

        let response = tiny_http::Response::from_string(resp_body)
            .with_status_code(tiny_http::StatusCode(status_code))
            .with_header(
                tiny_http::Header::from_bytes("Content-Type", content_type.as_bytes())
                    .unwrap_or_else(|_| {
                        tiny_http::Header::from_bytes("Content-Type", b"text/plain").unwrap()
                    }),
            );

        request.respond(response).ok();
    }

    Ok(Flow::Normal)
}

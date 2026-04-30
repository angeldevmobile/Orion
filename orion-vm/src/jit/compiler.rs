//! Compilador Bytecode → Cranelift JIT — Fase 5
//!
//! Novedades:
//!   - f64 nativo (LoadFloat, fadd/fsub/fmul/fdiv/fpow, fcmp)
//!   - Funciones JIT-to-JIT compiladas y cacheadas (HashMap<String, FuncId>)
//!   - Return con valor
//!   - Inferencia de tipos de variables (I64 / F64)
//!   - API `run_program(&OrionBytecode)` además de `run(&[Instruction])`

use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::{
    condcodes::{FloatCC, IntCC},
    types, AbiParam, InstBuilder,
};
use cranelift_codegen::settings;
use cranelift_codegen::settings::Configurable;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, FuncId, Linkage, Module};

use crate::bytecode::OrionBytecode;
use crate::instruction::Instruction;

//   Tipo de valor en la pila de compilación                   

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JType {
    Int,
    Float,
    Bool,
    Str,
    Unknown,
}

impl JType {
    fn cl_type(self) -> cranelift_codegen::ir::Type {
        match self {
            JType::Float => types::F64,
            _ => types::I64,
        }
    }
}

//   Inferencia de tipos de variables                      

/// Primera pasada: determina si cada variable es F64 o I64.
/// Float gana sobre cualquier otro tipo (decisión conservadora segura).
fn infer_var_types(instructions: &[Instruction]) -> HashMap<String, JType> {
    let mut var_types: HashMap<String, JType> = HashMap::new();
    let mut stack: Vec<JType> = Vec::new();

    fn merge(a: JType, b: JType) -> JType {
        if a == JType::Float || b == JType::Float {
            JType::Float
        } else if a == JType::Str || b == JType::Str {
            JType::Str
        } else if a == JType::Int || b == JType::Int {
            JType::Int
        } else if a == JType::Bool || b == JType::Bool {
            JType::Bool
        } else {
            JType::Unknown
        }
    }

    for instr in instructions {
        match instr {
            Instruction::LoadInt(_)   => stack.push(JType::Int),
            Instruction::LoadFloat(_) => stack.push(JType::Float),
            Instruction::LoadBool(_)  => stack.push(JType::Bool),
            Instruction::LoadStr(_)   => stack.push(JType::Str),
            Instruction::LoadNull     => stack.push(JType::Int),
            Instruction::LoadVar(n)   => {
                let t = var_types.get(n).copied().unwrap_or(JType::Unknown);
                stack.push(t);
            }
            Instruction::StoreVar(n) | Instruction::StoreConst(n) => {
                let t = stack.pop().unwrap_or(JType::Unknown);
                let existing = var_types.get(n).copied().unwrap_or(JType::Unknown);
                var_types.insert(n.clone(), merge(existing, t));
            }
            Instruction::Add | Instruction::Sub | Instruction::Mul
            | Instruction::Div | Instruction::Pow => {
                let b = stack.pop().unwrap_or(JType::Int);
                let a = stack.pop().unwrap_or(JType::Int);
                let r = if a == JType::Float || b == JType::Float {
                    JType::Float
                } else {
                    JType::Int
                };
                stack.push(r);
            }
            Instruction::Mod => {
                stack.pop(); stack.pop();
                stack.push(JType::Int);
            }
            Instruction::Neg => {
                let t = stack.pop().unwrap_or(JType::Int);
                stack.push(t);
            }
            Instruction::Eq | Instruction::NotEq | Instruction::Lt
            | Instruction::LtEq | Instruction::Gt | Instruction::GtEq
            | Instruction::And | Instruction::Or => {
                stack.pop(); stack.pop();
                stack.push(JType::Bool);
            }
            Instruction::Not => {
                stack.pop();
                stack.push(JType::Bool);
            }
            Instruction::Call(_, n) => {
                for _ in 0..*n { stack.pop(); }
                stack.push(JType::Int); // conservador
            }
            Instruction::Pop => { stack.pop(); }
            Instruction::Dup => {
                if let Some(&t) = stack.last() { stack.push(t); }
            }
            Instruction::Show => { stack.pop(); }
            _ => {}
        }
    }
    var_types
}

//   Elegibilidad                                

fn is_eligible(instr: &Instruction) -> bool {
    matches!(
        instr,
        Instruction::LoadInt(_)
            | Instruction::LoadFloat(_)
            | Instruction::LoadBool(_)
            | Instruction::LoadNull
            | Instruction::LoadStr(_)
            | Instruction::StoreVar(_)
            | Instruction::StoreConst(_)
            | Instruction::LoadVar(_)
            | Instruction::Add
            | Instruction::Sub
            | Instruction::Mul
            | Instruction::Div
            | Instruction::Mod
            | Instruction::Pow
            | Instruction::Neg
            | Instruction::Eq
            | Instruction::NotEq
            | Instruction::Lt
            | Instruction::LtEq
            | Instruction::Gt
            | Instruction::GtEq
            | Instruction::And
            | Instruction::Or
            | Instruction::Not
            | Instruction::Jump(_)
            | Instruction::JumpIfFalse(_)
            | Instruction::JumpIfTrue(_)
            | Instruction::Show
            | Instruction::Pop
            | Instruction::Dup
            | Instruction::Call(_, _)
            | Instruction::MakeFunction(_, _, _)
            | Instruction::Return
            | Instruction::Halt
    )
}

//   Análisis de bloques básicos                         

fn find_block_starts(instructions: &[Instruction]) -> HashSet<usize> {
    let mut starts = HashSet::new();
    starts.insert(0);
    starts.insert(instructions.len()); // centinela

    for (i, instr) in instructions.iter().enumerate() {
        match instr {
            Instruction::Jump(t) => {
                starts.insert(*t);
                starts.insert(i + 1);
            }
            Instruction::JumpIfFalse(t) | Instruction::JumpIfTrue(t) => {
                starts.insert(*t);
                starts.insert(i + 1);
            }
            _ => {}
        }
    }
    starts
}

//   IDs de runtime                               

#[derive(Clone)]
struct RuntimeIds {
    show_int:   FuncId,
    show_float: FuncId,
    show_bool:  FuncId,
    show_str:   FuncId,
    div_int:    FuncId,
    mod_int:    FuncId,
    pow_int:    FuncId,
    pow_f64:    FuncId,
}

//   Compilador JIT                               

pub struct JitCompiler {
    module: JITModule,
    fn_counter: usize,
    /// Cache de FuncId de funciones de usuario ya declaradas.
    fn_cache: HashMap<String, FuncId>,
    /// Mantiene las cadenas C vivas mientras el módulo JIT existe.
    string_storage: Vec<Vec<u8>>,
    /// IDs de funciones de runtime (inicializados en ensure_runtime).
    rt: Option<RuntimeIds>,
}

impl JitCompiler {
    pub fn new() -> Result<Self, String> {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();
        flag_builder.set("opt_level", "speed").unwrap();

        let isa_builder = cranelift_native::builder()
            .map_err(|msg| format!("ISA nativa no disponible: {msg}"))?;

        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .map_err(|e| format!("Error construyendo ISA: {e}"))?;

        let mut jit_builder = JITBuilder::with_isa(isa, default_libcall_names());

        jit_builder.symbol("orion_rt_show_int",   super::runtime::orion_rt_show_int   as *const u8);
        jit_builder.symbol("orion_rt_show_float", super::runtime::orion_rt_show_float as *const u8);
        jit_builder.symbol("orion_rt_show_bool",  super::runtime::orion_rt_show_bool  as *const u8);
        jit_builder.symbol("orion_rt_show_str",   super::runtime::orion_rt_show_str   as *const u8);
        jit_builder.symbol("orion_rt_div_int",    super::runtime::orion_rt_div_int    as *const u8);
        jit_builder.symbol("orion_rt_mod_int",    super::runtime::orion_rt_mod_int    as *const u8);
        jit_builder.symbol("orion_rt_pow_int",    super::runtime::orion_rt_pow_int    as *const u8);
        jit_builder.symbol("orion_rt_pow_f64",    super::runtime::orion_rt_pow_f64    as *const u8);

        let module = JITModule::new(jit_builder);
        Ok(JitCompiler {
            module,
            fn_counter: 0,
            fn_cache: HashMap::new(),
            string_storage: Vec::new(),
            rt: None,
        })
    }

    /// Declara las funciones de runtime en el módulo (idempotente).
    fn ensure_runtime(&mut self) -> Result<(), String> {
        if self.rt.is_some() {
            return Ok(());
        }

        macro_rules! decl_rt {
            ($name:literal, [$($p:expr),*], [$($r:expr),*]) => {{
                let mut sig = self.module.make_signature();
                $(sig.params.push(AbiParam::new($p));)*
                $(sig.returns.push(AbiParam::new($r));)*
                self.module
                    .declare_function($name, Linkage::Import, &sig)
                    .map_err(|e| e.to_string())?
            }};
        }

        let show_int   = decl_rt!("orion_rt_show_int",   [types::I64], []);
        let show_float = decl_rt!("orion_rt_show_float", [types::F64], []);
        let show_bool  = decl_rt!("orion_rt_show_bool",  [types::I64], []);
        let show_str   = decl_rt!("orion_rt_show_str",   [types::I64], []);
        let div_int    = decl_rt!("orion_rt_div_int",    [types::I64, types::I64], [types::I64]);
        let mod_int    = decl_rt!("orion_rt_mod_int",    [types::I64, types::I64], [types::I64]);
        let pow_int    = decl_rt!("orion_rt_pow_int",    [types::I64, types::I64], [types::I64]);
        let pow_f64    = decl_rt!("orion_rt_pow_f64",    [types::F64, types::F64], [types::F64]);

        self.rt = Some(RuntimeIds {
            show_int, show_float, show_bool, show_str,
            div_int, mod_int, pow_int, pow_f64,
        });
        Ok(())
    }

    //   API pública                               

    /// Compila y ejecuta un programa completo (main + todas las funciones).
    ///
    /// Devuelve `Ok(true)` si compiló y ejecutó con JIT.
    /// Devuelve `Ok(false)` si hay instrucciones no soportadas → usar intérprete.
    /// Devuelve `Err` para errores de compilación JIT reales.
    pub fn run_program(&mut self, bc: &OrionBytecode) -> Result<bool, String> {
        // Elegibilidad global
        for instr in &bc.main {
            if !is_eligible(instr) { return Ok(false); }
        }
        for fdef in bc.functions.values() {
            for instr in &fdef.body {
                if !is_eligible(instr) { return Ok(false); }
            }
        }

        if bc.main.is_empty() {
            return Ok(true);
        }

        self.ensure_runtime()?;

        // 1. Declarar (pero no definir aún) todas las funciones de usuario
        //    para que Call en main y otras funciones puedan referenciarlas.
        let fn_names: Vec<String> = bc.functions.keys().cloned().collect();
        for name in &fn_names {
            let fdef = &bc.functions[name];
            let n_params = fdef.params.len();
            let mut sig = self.module.make_signature();
            for _ in 0..n_params {
                sig.params.push(AbiParam::new(types::I64));
            }
            sig.returns.push(AbiParam::new(types::I64));
            let fid = self
                .module
                .declare_function(name, Linkage::Local, &sig)
                .map_err(|e| e.to_string())?;
            self.fn_cache.insert(name.clone(), fid);
        }

        // 2. Declarar la función main
        let main_name = format!("orion_jit_main_{}", self.fn_counter);
        self.fn_counter += 1;
        let main_sig = self.module.make_signature();
        let main_id = self
            .module
            .declare_function(&main_name, Linkage::Local, &main_sig)
            .map_err(|e| e.to_string())?;

        // 3. Definir los cuerpos de todas las funciones de usuario
        for name in &fn_names {
            let fdef = bc.functions[name].clone();
            let fid = self.fn_cache[name];
            let n_params = fdef.params.len();

            let mut fn_sig = self.module.make_signature();
            for _ in 0..n_params {
                fn_sig.params.push(AbiParam::new(types::I64));
            }
            fn_sig.returns.push(AbiParam::new(types::I64));

            let mut ctx = self.module.make_context();
            ctx.func.signature = fn_sig;
            self.fill_function_body(&fdef.body, &fdef.params, &mut ctx, false)?;
            self.module
                .define_function(fid, &mut ctx)
                .map_err(|e| format!("JIT define fn '{name}': {e}"))?;
            self.module.clear_context(&mut ctx);
        }

        // 4. Definir el cuerpo de main
        let mut ctx = self.module.make_context();
        ctx.func.signature = main_sig;
        self.fill_function_body(&bc.main, &[], &mut ctx, true)?;
        self.module
            .define_function(main_id, &mut ctx)
            .map_err(|e| format!("JIT define main: {e}"))?;
        self.module.clear_context(&mut ctx);

        // 5. Compilar todo de una vez
        self.module
            .finalize_definitions()
            .map_err(|e| format!("JIT finalize: {e}"))?;

        // 6. Ejecutar main
        let code_ptr = self.module.get_finalized_function(main_id);
        // SAFETY: firma `() -> ()`, generada por Cranelift, válida mientras module vive.
        unsafe {
            let f: extern "C" fn() = std::mem::transmute(code_ptr);
            f();
        }

        Ok(true)
    }

    /// API de compatibilidad: solo instrucciones de main, sin funciones.
    pub fn run(&mut self, instructions: &[Instruction]) -> Result<bool, String> {
        use indexmap::IndexMap;
        let dummy = OrionBytecode {
            main: instructions.to_vec(),
            lines: vec![],
            functions: IndexMap::new(),
            shapes: IndexMap::new(),
        };
        self.run_program(&dummy)
    }

    //   Generación de IR                            

    /// Construye el cuerpo Cranelift IR en `ctx.func`.
    ///
    /// `params` son los nombres de los parámetros de la función (vacío para main).
    /// `is_main` controla el tipo de Return (sin valor vs. con I64).
    fn fill_function_body(
        &mut self,
        instructions: &[Instruction],
        params: &[String],
        ctx: &mut cranelift_codegen::Context,
        is_main: bool,
    ) -> Result<(), String> {
        //   Paso 0: inferir tipos de variables                 
        let var_types = infer_var_types(instructions);

        //   Paso 1: análisis de bloques básicos                 
        let block_starts = find_block_starts(instructions);
        let mut sorted_starts: Vec<usize> = block_starts.iter().cloned().collect();
        sorted_starts.sort_unstable();

        //   Paso 2: declarar func-refs ANTES de crear el builder        
        // (declare_func_in_func necesita &mut Function, que el builder también toma)

        // Extraer IDs en variables locales para evitar borrows múltiples de self.
        let rt = self.rt.as_ref().expect("runtime debe estar inicializado").clone();
        let cached_fns: Vec<(String, FuncId)> = self.fn_cache
            .iter()
            .map(|(k, &v)| (k.clone(), v))
            .collect();

        let show_int_ref   = self.module.declare_func_in_func(rt.show_int,   &mut ctx.func);
        let show_float_ref = self.module.declare_func_in_func(rt.show_float, &mut ctx.func);
        let show_bool_ref  = self.module.declare_func_in_func(rt.show_bool,  &mut ctx.func);
        let show_str_ref   = self.module.declare_func_in_func(rt.show_str,   &mut ctx.func);
        let div_ref        = self.module.declare_func_in_func(rt.div_int,    &mut ctx.func);
        let mod_ref        = self.module.declare_func_in_func(rt.mod_int,    &mut ctx.func);
        let pow_int_ref    = self.module.declare_func_in_func(rt.pow_int,    &mut ctx.func);
        let pow_f64_ref    = self.module.declare_func_in_func(rt.pow_f64,    &mut ctx.func);

        let mut user_fn_refs: HashMap<String, cranelift_codegen::ir::FuncRef> = HashMap::new();
        for (fname, fid) in &cached_fns {
            let fref = self.module.declare_func_in_func(*fid, &mut ctx.func);
            user_fn_refs.insert(fname.clone(), fref);
        }

        //   Paso 3: construir bloques y variables                

        let mut fb_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);

        // Crear bloques Cranelift para cada inicio de BB
        let mut block_map: HashMap<usize, cranelift_codegen::ir::Block> = HashMap::new();
        for &idx in &sorted_starts {
            block_map.insert(idx, builder.create_block());
        }

        // Recopilar todos los nombres de variables usados
        let mut var_names: Vec<String> = Vec::new();
        for instr in instructions {
            match instr {
                Instruction::StoreVar(n)
                | Instruction::StoreConst(n)
                | Instruction::LoadVar(n) => {
                    if !var_names.contains(n) {
                        var_names.push(n.clone());
                    }
                }
                _ => {}
            }
        }
        for p in params {
            if !var_names.contains(p) {
                var_names.push(p.clone());
            }
        }

        // Declarar variables Cranelift con el tipo inferido
        let mut var_table: HashMap<String, (Variable, JType)> = HashMap::new();
        for (vid, name) in var_names.iter().enumerate() {
            let jt = var_types.get(name).copied().unwrap_or(JType::Int);
            let cl_ty = jt.cl_type();
            let v = Variable::from_u32(vid as u32);
            builder.declare_var(v, cl_ty);
            var_table.insert(name.clone(), (v, jt));
        }

        //   Bloque de entrada                          

        let entry_block = block_map[&0];

        // Para funciones con parámetros: pasar los args a través del entry block
        if !params.is_empty() {
            builder.append_block_params_for_function_params(entry_block);
        }
        builder.switch_to_block(entry_block);

        // Inicializar todas las variables a 0/0.0 (excepto los parámetros)
        for (name, (v, jt)) in &var_table {
            if !params.contains(name) {
                let zero = match jt {
                    JType::Float => builder.ins().f64const(0.0),
                    _ => builder.ins().iconst(types::I64, 0),
                };
                builder.def_var(*v, zero);
            }
        }

        // Definir variables de parámetros desde los block params del entry block
        if !params.is_empty() {
            let block_params: Vec<cranelift_codegen::ir::Value> =
                builder.block_params(entry_block).to_vec();
            for (i, pname) in params.iter().enumerate() {
                if i < block_params.len() {
                    if let Some((var, _)) = var_table.get(pname) {
                        builder.def_var(*var, block_params[i]);
                    }
                }
            }
        }

        //   Paso 4: compilar instrucciones                   

        let mut stack: Vec<(cranelift_codegen::ir::Value, JType)> = Vec::new();
        let mut terminated = false;

        for (i, instr) in instructions.iter().enumerate() {
            // Cambio de bloque básico
            if i > 0 && block_starts.contains(&i) {
                let next_block = block_map[&i];
                if !terminated {
                    builder.ins().jump(next_block, &[]);
                }
                builder.switch_to_block(next_block);
                terminated = false;
                stack.clear();
            }

            if terminated { continue; }

            match instr {
                //   Literales                         
                Instruction::LoadInt(n) => {
                    stack.push((builder.ins().iconst(types::I64, *n), JType::Int));
                }
                Instruction::LoadFloat(f) => {
                    stack.push((builder.ins().f64const(*f), JType::Float));
                }
                Instruction::LoadBool(b) => {
                    let v = builder.ins().iconst(types::I64, if *b { 1 } else { 0 });
                    stack.push((v, JType::Bool));
                }
                Instruction::LoadNull => {
                    stack.push((builder.ins().iconst(types::I64, 0), JType::Int));
                }
                Instruction::LoadStr(s) => {
                    let mut bytes = s.as_bytes().to_vec();
                    bytes.push(0u8);
                    let raw_ptr = bytes.as_ptr() as u64 as i64;
                    self.string_storage.push(bytes);
                    stack.push((builder.ins().iconst(types::I64, raw_ptr), JType::Str));
                }

                //   Variables                          
                Instruction::StoreVar(name) | Instruction::StoreConst(name) => {
                    let (val, ty) = stack.pop().ok_or("StoreVar: pila vacía")?;
                    if let Some(&(var, var_ty)) = var_table.get(name) {
                        let coerced = coerce_value(&mut builder, val, ty, var_ty);
                        builder.def_var(var, coerced);
                    }
                }
                Instruction::LoadVar(name) => {
                    let &(var, ty) = var_table
                        .get(name)
                        .ok_or_else(|| format!("JIT: variable no declarada '{name}'"))?;
                    stack.push((builder.use_var(var), ty));
                }

                //   Aritmética                         
                Instruction::Add => {
                    let (b, tb) = stack.pop().ok_or("Add: pila vacía")?;
                    let (a, ta) = stack.pop().ok_or("Add: pila vacía")?;
                    let (av, bv, tr) = numeric_pair(&mut builder, a, ta, b, tb);
                    let r = if tr == JType::Float {
                        builder.ins().fadd(av, bv)
                    } else {
                        builder.ins().iadd(av, bv)
                    };
                    stack.push((r, tr));
                }
                Instruction::Sub => {
                    let (b, tb) = stack.pop().ok_or("Sub: pila vacía")?;
                    let (a, ta) = stack.pop().ok_or("Sub: pila vacía")?;
                    let (av, bv, tr) = numeric_pair(&mut builder, a, ta, b, tb);
                    let r = if tr == JType::Float {
                        builder.ins().fsub(av, bv)
                    } else {
                        builder.ins().isub(av, bv)
                    };
                    stack.push((r, tr));
                }
                Instruction::Mul => {
                    let (b, tb) = stack.pop().ok_or("Mul: pila vacía")?;
                    let (a, ta) = stack.pop().ok_or("Mul: pila vacía")?;
                    let (av, bv, tr) = numeric_pair(&mut builder, a, ta, b, tb);
                    let r = if tr == JType::Float {
                        builder.ins().fmul(av, bv)
                    } else {
                        builder.ins().imul(av, bv)
                    };
                    stack.push((r, tr));
                }
                Instruction::Div => {
                    let (b, tb) = stack.pop().ok_or("Div: pila vacía")?;
                    let (a, ta) = stack.pop().ok_or("Div: pila vacía")?;
                    if ta == JType::Float || tb == JType::Float {
                        let (av, bv, _) = numeric_pair(&mut builder, a, ta, b, tb);
                        stack.push((builder.ins().fdiv(av, bv), JType::Float));
                    } else {
                        let call = builder.ins().call(div_ref, &[a, b]);
                        stack.push((builder.inst_results(call)[0], JType::Int));
                    }
                }
                Instruction::Mod => {
                    let (b, _) = stack.pop().ok_or("Mod: pila vacía")?;
                    let (a, _) = stack.pop().ok_or("Mod: pila vacía")?;
                    let call = builder.ins().call(mod_ref, &[a, b]);
                    stack.push((builder.inst_results(call)[0], JType::Int));
                }
                Instruction::Pow => {
                    let (b, tb) = stack.pop().ok_or("Pow: pila vacía")?;
                    let (a, ta) = stack.pop().ok_or("Pow: pila vacía")?;
                    if ta == JType::Float || tb == JType::Float {
                        let (av, bv, _) = numeric_pair(&mut builder, a, ta, b, tb);
                        let call = builder.ins().call(pow_f64_ref, &[av, bv]);
                        stack.push((builder.inst_results(call)[0], JType::Float));
                    } else {
                        let call = builder.ins().call(pow_int_ref, &[a, b]);
                        stack.push((builder.inst_results(call)[0], JType::Int));
                    }
                }
                Instruction::Neg => {
                    let (a, ta) = stack.pop().ok_or("Neg: pila vacía")?;
                    let r = if ta == JType::Float {
                        builder.ins().fneg(a)
                    } else {
                        builder.ins().ineg(a)
                    };
                    stack.push((r, ta));
                }

                //   Comparaciones                        
                Instruction::Eq => {
                    let (b, tb) = stack.pop().ok_or("Eq: pila vacía")?;
                    let (a, ta) = stack.pop().ok_or("Eq: pila vacía")?;
                    let c = if ta == JType::Float || tb == JType::Float {
                        let (av, bv, _) = numeric_pair(&mut builder, a, ta, b, tb);
                        builder.ins().fcmp(FloatCC::Equal, av, bv)
                    } else {
                        builder.ins().icmp(IntCC::Equal, a, b)
                    };
                    stack.push((builder.ins().uextend(types::I64, c), JType::Bool));
                }
                Instruction::NotEq => {
                    let (b, tb) = stack.pop().ok_or("NotEq: pila vacía")?;
                    let (a, ta) = stack.pop().ok_or("NotEq: pila vacía")?;
                    let c = if ta == JType::Float || tb == JType::Float {
                        let (av, bv, _) = numeric_pair(&mut builder, a, ta, b, tb);
                        builder.ins().fcmp(FloatCC::NotEqual, av, bv)
                    } else {
                        builder.ins().icmp(IntCC::NotEqual, a, b)
                    };
                    stack.push((builder.ins().uextend(types::I64, c), JType::Bool));
                }
                Instruction::Lt => {
                    let (b, tb) = stack.pop().ok_or("Lt: pila vacía")?;
                    let (a, ta) = stack.pop().ok_or("Lt: pila vacía")?;
                    let c = if ta == JType::Float || tb == JType::Float {
                        let (av, bv, _) = numeric_pair(&mut builder, a, ta, b, tb);
                        builder.ins().fcmp(FloatCC::LessThan, av, bv)
                    } else {
                        builder.ins().icmp(IntCC::SignedLessThan, a, b)
                    };
                    stack.push((builder.ins().uextend(types::I64, c), JType::Bool));
                }
                Instruction::LtEq => {
                    let (b, tb) = stack.pop().ok_or("LtEq: pila vacía")?;
                    let (a, ta) = stack.pop().ok_or("LtEq: pila vacía")?;
                    let c = if ta == JType::Float || tb == JType::Float {
                        let (av, bv, _) = numeric_pair(&mut builder, a, ta, b, tb);
                        builder.ins().fcmp(FloatCC::LessThanOrEqual, av, bv)
                    } else {
                        builder.ins().icmp(IntCC::SignedLessThanOrEqual, a, b)
                    };
                    stack.push((builder.ins().uextend(types::I64, c), JType::Bool));
                }
                Instruction::Gt => {
                    let (b, tb) = stack.pop().ok_or("Gt: pila vacía")?;
                    let (a, ta) = stack.pop().ok_or("Gt: pila vacía")?;
                    let c = if ta == JType::Float || tb == JType::Float {
                        let (av, bv, _) = numeric_pair(&mut builder, a, ta, b, tb);
                        builder.ins().fcmp(FloatCC::GreaterThan, av, bv)
                    } else {
                        builder.ins().icmp(IntCC::SignedGreaterThan, a, b)
                    };
                    stack.push((builder.ins().uextend(types::I64, c), JType::Bool));
                }
                Instruction::GtEq => {
                    let (b, tb) = stack.pop().ok_or("GtEq: pila vacía")?;
                    let (a, ta) = stack.pop().ok_or("GtEq: pila vacía")?;
                    let c = if ta == JType::Float || tb == JType::Float {
                        let (av, bv, _) = numeric_pair(&mut builder, a, ta, b, tb);
                        builder.ins().fcmp(FloatCC::GreaterThanOrEqual, av, bv)
                    } else {
                        builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, a, b)
                    };
                    stack.push((builder.ins().uextend(types::I64, c), JType::Bool));
                }

                //   Lógica                           
                Instruction::And => {
                    let (b, _) = stack.pop().ok_or("And: pila vacía")?;
                    let (a, _) = stack.pop().ok_or("And: pila vacía")?;
                    let z = builder.ins().iconst(types::I64, 0);
                    let a1 = builder.ins().icmp(IntCC::NotEqual, a, z);
                    let b1 = builder.ins().icmp(IntCC::NotEqual, b, z);
                    let ae = builder.ins().uextend(types::I64, a1);
                    let be = builder.ins().uextend(types::I64, b1);
                    stack.push((builder.ins().band(ae, be), JType::Bool));
                }
                Instruction::Or => {
                    let (b, _) = stack.pop().ok_or("Or: pila vacía")?;
                    let (a, _) = stack.pop().ok_or("Or: pila vacía")?;
                    let z = builder.ins().iconst(types::I64, 0);
                    let a1 = builder.ins().icmp(IntCC::NotEqual, a, z);
                    let b1 = builder.ins().icmp(IntCC::NotEqual, b, z);
                    let ae = builder.ins().uextend(types::I64, a1);
                    let be = builder.ins().uextend(types::I64, b1);
                    stack.push((builder.ins().bor(ae, be), JType::Bool));
                }
                Instruction::Not => {
                    let (a, _) = stack.pop().ok_or("Not: pila vacía")?;
                    let z = builder.ins().iconst(types::I64, 0);
                    let c = builder.ins().icmp(IntCC::Equal, a, z);
                    stack.push((builder.ins().uextend(types::I64, c), JType::Bool));
                }

                //   Control de flujo                      
                Instruction::Jump(target) => {
                    let t_block = *block_map
                        .get(target)
                        .ok_or_else(|| format!("Jump: bloque {target} no encontrado"))?;
                    builder.ins().jump(t_block, &[]);
                    terminated = true;
                }
                Instruction::JumpIfFalse(target) => {
                    let (cond, _) = stack.pop().ok_or("JumpIfFalse: pila vacía")?;
                    let false_block = *block_map
                        .get(target)
                        .ok_or_else(|| format!("JumpIfFalse: {target} no encontrado"))?;
                    let true_block = *block_map
                        .get(&(i + 1))
                        .ok_or_else(|| format!("JumpIfFalse: {} no encontrado", i + 1))?;
                    builder.ins().brif(cond, true_block, &[], false_block, &[]);
                    terminated = true;
                }
                Instruction::JumpIfTrue(target) => {
                    let (cond, _) = stack.pop().ok_or("JumpIfTrue: pila vacía")?;
                    let true_block = *block_map
                        .get(target)
                        .ok_or_else(|| format!("JumpIfTrue: {target} no encontrado"))?;
                    let false_block = *block_map
                        .get(&(i + 1))
                        .ok_or_else(|| format!("JumpIfTrue: {} no encontrado", i + 1))?;
                    builder.ins().brif(cond, true_block, &[], false_block, &[]);
                    terminated = true;
                }

                //   Funciones                          
                Instruction::MakeFunction(_, _, _) => {
                    // No-op en JIT: las funciones ya fueron compiladas antes de main.
                }
                Instruction::Call(fname, n_args) => {
                    let n = *n_args as usize;
                    // Pop args en orden de pila y revertir para orden correcto
                    let mut args: Vec<(cranelift_codegen::ir::Value, JType)> = Vec::new();
                    for _ in 0..n {
                        args.push(stack.pop().ok_or("Call: pila vacía")?);
                    }
                    args.reverse();

                    if let Some(&fref) = user_fn_refs.get(fname) {
                        // Coercionar a I64 (Phase 5: params de funciones son I64)
                        let coerced: Vec<cranelift_codegen::ir::Value> = args
                            .iter()
                            .map(|&(v, t)| coerce_value(&mut builder, v, t, JType::Int))
                            .collect();
                        let call = builder.ins().call(fref, &coerced);
                        stack.push((builder.inst_results(call)[0], JType::Int));
                    } else {
                        return Err(format!("JIT: función '{fname}' no encontrada en cache"));
                    }
                }
                Instruction::Return => {
                    if is_main {
                        builder.ins().return_(&[]);
                    } else {
                        let ret_val = stack.pop().map(|(v, t)| {
                            coerce_value(&mut builder, v, t, JType::Int)
                        }).unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        builder.ins().return_(&[ret_val]);
                    }
                    terminated = true;
                }

                //   I/O                             
                Instruction::Show => {
                    let (val, ty) = stack.pop().ok_or("Show: pila vacía")?;
                    match ty {
                        JType::Float => { builder.ins().call(show_float_ref, &[val]); }
                        JType::Bool  => { builder.ins().call(show_bool_ref,  &[val]); }
                        JType::Str   => { builder.ins().call(show_str_ref,   &[val]); }
                        _            => { builder.ins().call(show_int_ref,   &[val]); }
                    }
                }

                //   Pila                            
                Instruction::Pop => { stack.pop(); }
                Instruction::Dup => {
                    let top = stack.last().cloned().ok_or("Dup: pila vacía")?;
                    stack.push(top);
                }

                //   Terminadores                        
                Instruction::Halt => {
                    if is_main {
                        builder.ins().return_(&[]);
                    } else {
                        let zero = builder.ins().iconst(types::I64, 0);
                        builder.ins().return_(&[zero]);
                    }
                    terminated = true;
                }

                other => {
                    return Err(format!("JIT: instrucción inesperada: {other:?}"));
                }
            }
        }

        // Return implícito al final
        if !terminated {
            if is_main {
                builder.ins().return_(&[]);
            } else {
                let ret_val = stack.pop().map(|(v, t)| {
                    coerce_value(&mut builder, v, t, JType::Int)
                }).unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                builder.ins().return_(&[ret_val]);
            }
        }

        builder.seal_all_blocks();
        builder.finalize();
        Ok(())
    }
}

//   Helpers                                   

/// Convierte un valor de `from_ty` a `to_ty` insertando una instrucción Cranelift si es necesario.
fn coerce_value(
    builder: &mut FunctionBuilder,
    val: cranelift_codegen::ir::Value,
    from_ty: JType,
    to_ty: JType,
) -> cranelift_codegen::ir::Value {
    match (from_ty, to_ty) {
        (JType::Float, JType::Int)
        | (JType::Float, JType::Bool)
        | (JType::Float, JType::Unknown) => builder.ins().fcvt_to_sint_sat(types::I64, val),
        (JType::Int, JType::Float)
        | (JType::Bool, JType::Float)
        | (JType::Unknown, JType::Float) => builder.ins().fcvt_from_sint(types::F64, val),
        _ => val,
    }
}

/// Normaliza dos operandos al tipo resultado correcto.
/// Si alguno es Float, ambos se convierten a F64 y el resultado es Float.
fn numeric_pair(
    builder: &mut FunctionBuilder,
    a: cranelift_codegen::ir::Value,
    ta: JType,
    b: cranelift_codegen::ir::Value,
    tb: JType,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value, JType) {
    if ta == JType::Float || tb == JType::Float {
        let av = coerce_value(builder, a, ta, JType::Float);
        let bv = coerce_value(builder, b, tb, JType::Float);
        (av, bv, JType::Float)
    } else {
        (a, b, JType::Int)
    }
}

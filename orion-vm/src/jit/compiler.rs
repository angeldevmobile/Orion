//! Compilador Bytecode → Cranelift JIT — Fase JIT-1: Valores Unificados
//!
//! Todos los valores son punteros a OrionVal en heap (pasados como i64).
//! Cada operación delega a una función de runtime (rt_add, rt_eq, etc.).
//! Fase JIT-2 añadirá MakeList, MakeDict, GetIndex, SetIndex.

use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::{types, AbiParam, InstBuilder};
use cranelift_codegen::settings;
use cranelift_codegen::settings::Configurable;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, FuncId, Linkage, Module};

use crate::bytecode::OrionBytecode;
use crate::instruction::Instruction;

// ─── IDs de runtime ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct RuntimeIds {
    // Constructores
    make_null:       FuncId,
    make_int:        FuncId,
    make_float_bits: FuncId,
    make_bool:       FuncId,
    make_str:        FuncId,
    // I/O y control
    show:            FuncId,
    is_truthy:       FuncId,
    // Aritmética
    add: FuncId, sub: FuncId, mul: FuncId,
    div: FuncId, rt_mod: FuncId, pow: FuncId, neg: FuncId,
    // Comparación
    eq: FuncId, neq: FuncId,
    lt: FuncId, lteq: FuncId, gt: FuncId, gteq: FuncId,
    // Lógica
    and: FuncId, or: FuncId, not: FuncId,
}

// ─── Análisis de bloques básicos ─────────────────────────────────────────────

fn find_block_starts(instructions: &[Instruction]) -> HashSet<usize> {
    let mut starts = HashSet::new();
    starts.insert(0);
    starts.insert(instructions.len());
    for (i, instr) in instructions.iter().enumerate() {
        match instr {
            Instruction::Jump(t) => { starts.insert(*t); starts.insert(i + 1); }
            Instruction::JumpIfFalse(t) | Instruction::JumpIfTrue(t) => {
                starts.insert(*t); starts.insert(i + 1);
            }
            _ => {}
        }
    }
    starts
}

// ─── Elegibilidad ────────────────────────────────────────────────────────────

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

// ─── Compilador JIT ──────────────────────────────────────────────────────────

pub struct JitCompiler {
    module:         JITModule,
    fn_counter:     usize,
    fn_cache:       HashMap<String, FuncId>,
    string_storage: Vec<Vec<u8>>,
    rt:             Option<RuntimeIds>,
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

        macro_rules! sym {
            ($name:literal, $fn:expr) => {
                jit_builder.symbol($name, $fn as *const u8);
            };
        }
        sym!("rt_make_null",       super::runtime::rt_make_null);
        sym!("rt_make_int",        super::runtime::rt_make_int);
        sym!("rt_make_float_bits", super::runtime::rt_make_float_bits);
        sym!("rt_make_bool",       super::runtime::rt_make_bool);
        sym!("rt_make_str",        super::runtime::rt_make_str);
        sym!("rt_show",            super::runtime::rt_show);
        sym!("rt_is_truthy",       super::runtime::rt_is_truthy);
        sym!("rt_add",             super::runtime::rt_add);
        sym!("rt_sub",             super::runtime::rt_sub);
        sym!("rt_mul",             super::runtime::rt_mul);
        sym!("rt_div",             super::runtime::rt_div);
        sym!("rt_mod",             super::runtime::rt_mod);
        sym!("rt_pow",             super::runtime::rt_pow);
        sym!("rt_neg",             super::runtime::rt_neg);
        sym!("rt_eq",              super::runtime::rt_eq);
        sym!("rt_neq",             super::runtime::rt_neq);
        sym!("rt_lt",              super::runtime::rt_lt);
        sym!("rt_lteq",            super::runtime::rt_lteq);
        sym!("rt_gt",              super::runtime::rt_gt);
        sym!("rt_gteq",            super::runtime::rt_gteq);
        sym!("rt_and",             super::runtime::rt_and);
        sym!("rt_or",              super::runtime::rt_or);
        sym!("rt_not",             super::runtime::rt_not);

        let module = JITModule::new(jit_builder);
        Ok(JitCompiler { module, fn_counter: 0, fn_cache: HashMap::new(), string_storage: Vec::new(), rt: None })
    }

    fn ensure_runtime(&mut self) -> Result<(), String> {
        if self.rt.is_some() { return Ok(()); }

        let i = types::I64;
        macro_rules! decl {
            ($name:literal, [$($p:expr),*], [$($r:expr),*]) => {{
                let mut sig = self.module.make_signature();
                $(sig.params.push(AbiParam::new($p));)*
                $(sig.returns.push(AbiParam::new($r));)*
                self.module.declare_function($name, Linkage::Import, &sig)
                    .map_err(|e| e.to_string())?
            }};
        }

        let make_null       = decl!("rt_make_null",       [],       [i]);
        let make_int        = decl!("rt_make_int",        [i],      [i]);
        let make_float_bits = decl!("rt_make_float_bits", [i],      [i]);
        let make_bool       = decl!("rt_make_bool",       [i],      [i]);
        let make_str        = decl!("rt_make_str",        [i],      [i]);
        let show            = decl!("rt_show",            [i],      []);
        let is_truthy       = decl!("rt_is_truthy",       [i],      [i]);
        let add             = decl!("rt_add",             [i, i],   [i]);
        let sub             = decl!("rt_sub",             [i, i],   [i]);
        let mul             = decl!("rt_mul",             [i, i],   [i]);
        let div             = decl!("rt_div",             [i, i],   [i]);
        let rt_mod          = decl!("rt_mod",             [i, i],   [i]);
        let pow             = decl!("rt_pow",             [i, i],   [i]);
        let neg             = decl!("rt_neg",             [i],      [i]);
        let eq              = decl!("rt_eq",              [i, i],   [i]);
        let neq             = decl!("rt_neq",             [i, i],   [i]);
        let lt              = decl!("rt_lt",              [i, i],   [i]);
        let lteq            = decl!("rt_lteq",            [i, i],   [i]);
        let gt              = decl!("rt_gt",              [i, i],   [i]);
        let gteq            = decl!("rt_gteq",            [i, i],   [i]);
        let and             = decl!("rt_and",             [i, i],   [i]);
        let or              = decl!("rt_or",              [i, i],   [i]);
        let not             = decl!("rt_not",             [i],      [i]);

        self.rt = Some(RuntimeIds {
            make_null, make_int, make_float_bits, make_bool, make_str,
            show, is_truthy,
            add, sub, mul, div, rt_mod, pow, neg,
            eq, neq, lt, lteq, gt, gteq,
            and, or, not,
        });
        Ok(())
    }

    // ─── API pública ─────────────────────────────────────────────────────────

    /// Compila y ejecuta un programa completo (main + funciones).
    ///
    /// - `Ok(true)`  → JIT compiló y ejecutó con éxito.
    /// - `Ok(false)` → instrucciones no elegibles → usar intérprete.
    /// - `Err(msg)`  → error real de compilación JIT.
    pub fn run_program(&mut self, bc: &OrionBytecode) -> Result<bool, String> {
        for instr in &bc.main {
            if !is_eligible(instr) { return Ok(false); }
        }
        for fdef in bc.functions.values() {
            for instr in &fdef.body {
                if !is_eligible(instr) { return Ok(false); }
            }
        }
        if bc.main.is_empty() { return Ok(true); }

        self.ensure_runtime()?;

        // 1. Declarar todas las funciones de usuario (sin definir aún)
        let fn_names: Vec<String> = bc.functions.keys().cloned().collect();
        for name in &fn_names {
            let n_params = bc.functions[name].params.len();
            let mut sig = self.module.make_signature();
            for _ in 0..n_params { sig.params.push(AbiParam::new(types::I64)); }
            sig.returns.push(AbiParam::new(types::I64));
            let fid = self.module.declare_function(name, Linkage::Local, &sig)
                .map_err(|e| e.to_string())?;
            self.fn_cache.insert(name.clone(), fid);
        }

        // 2. Declarar main
        let main_name = format!("orion_jit_main_{}", self.fn_counter);
        self.fn_counter += 1;
        let main_sig = self.module.make_signature();
        let main_id = self.module.declare_function(&main_name, Linkage::Local, &main_sig)
            .map_err(|e| e.to_string())?;

        // 3. Definir cuerpos de funciones de usuario
        for name in &fn_names {
            let fdef = bc.functions[name].clone();
            let fid = self.fn_cache[name];
            let n_params = fdef.params.len();

            let mut fn_sig = self.module.make_signature();
            for _ in 0..n_params { fn_sig.params.push(AbiParam::new(types::I64)); }
            fn_sig.returns.push(AbiParam::new(types::I64));

            let mut ctx = self.module.make_context();
            ctx.func.signature = fn_sig;
            self.fill_function_body(&fdef.body, &fdef.params, &mut ctx, false)?;
            self.module.define_function(fid, &mut ctx)
                .map_err(|e| format!("JIT define fn '{name}': {e}"))?;
            self.module.clear_context(&mut ctx);
        }

        // 4. Definir cuerpo de main
        let mut ctx = self.module.make_context();
        ctx.func.signature = main_sig;
        self.fill_function_body(&bc.main, &[], &mut ctx, true)?;
        self.module.define_function(main_id, &mut ctx)
            .map_err(|e| format!("JIT define main: {e}"))?;
        self.module.clear_context(&mut ctx);

        // 5. Compilar y ejecutar
        self.module.finalize_definitions()
            .map_err(|e| format!("JIT finalize: {e}"))?;

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

    // ─── Generación de IR ────────────────────────────────────────────────────

    fn fill_function_body(
        &mut self,
        instructions: &[Instruction],
        params: &[String],
        ctx: &mut cranelift_codegen::Context,
        is_main: bool,
    ) -> Result<(), String> {
        let block_starts = find_block_starts(instructions);
        let mut sorted_starts: Vec<usize> = block_starts.iter().cloned().collect();
        sorted_starts.sort_unstable();

        let rt = self.rt.as_ref().expect("runtime debe estar inicializado").clone();
        let cached_fns: Vec<(String, FuncId)> = self.fn_cache
            .iter().map(|(k, &v)| (k.clone(), v)).collect();

        // Declarar todas las func-refs ANTES de crear el builder
        let make_null_ref   = self.module.declare_func_in_func(rt.make_null,       &mut ctx.func);
        let make_int_ref    = self.module.declare_func_in_func(rt.make_int,        &mut ctx.func);
        let make_fbits_ref  = self.module.declare_func_in_func(rt.make_float_bits, &mut ctx.func);
        let make_bool_ref   = self.module.declare_func_in_func(rt.make_bool,       &mut ctx.func);
        let make_str_ref    = self.module.declare_func_in_func(rt.make_str,        &mut ctx.func);
        let show_ref        = self.module.declare_func_in_func(rt.show,            &mut ctx.func);
        let is_truthy_ref   = self.module.declare_func_in_func(rt.is_truthy,       &mut ctx.func);
        let add_ref         = self.module.declare_func_in_func(rt.add,             &mut ctx.func);
        let sub_ref         = self.module.declare_func_in_func(rt.sub,             &mut ctx.func);
        let mul_ref         = self.module.declare_func_in_func(rt.mul,             &mut ctx.func);
        let div_ref         = self.module.declare_func_in_func(rt.div,             &mut ctx.func);
        let mod_ref         = self.module.declare_func_in_func(rt.rt_mod,          &mut ctx.func);
        let pow_ref         = self.module.declare_func_in_func(rt.pow,             &mut ctx.func);
        let neg_ref         = self.module.declare_func_in_func(rt.neg,             &mut ctx.func);
        let eq_ref          = self.module.declare_func_in_func(rt.eq,              &mut ctx.func);
        let neq_ref         = self.module.declare_func_in_func(rt.neq,             &mut ctx.func);
        let lt_ref          = self.module.declare_func_in_func(rt.lt,              &mut ctx.func);
        let lteq_ref        = self.module.declare_func_in_func(rt.lteq,            &mut ctx.func);
        let gt_ref          = self.module.declare_func_in_func(rt.gt,              &mut ctx.func);
        let gteq_ref        = self.module.declare_func_in_func(rt.gteq,            &mut ctx.func);
        let and_ref         = self.module.declare_func_in_func(rt.and,             &mut ctx.func);
        let or_ref          = self.module.declare_func_in_func(rt.or,              &mut ctx.func);
        let not_ref         = self.module.declare_func_in_func(rt.not,             &mut ctx.func);

        let mut user_fn_refs: HashMap<String, cranelift_codegen::ir::FuncRef> = HashMap::new();
        for (fname, fid) in &cached_fns {
            let fref = self.module.declare_func_in_func(*fid, &mut ctx.func);
            user_fn_refs.insert(fname.clone(), fref);
        }

        // Construir bloques
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);

        let mut block_map: HashMap<usize, cranelift_codegen::ir::Block> = HashMap::new();
        for &idx in &sorted_starts {
            block_map.insert(idx, builder.create_block());
        }

        // Recopilar nombres de variables
        let mut var_names: Vec<String> = Vec::new();
        for instr in instructions {
            match instr {
                Instruction::StoreVar(n) | Instruction::StoreConst(n) | Instruction::LoadVar(n) => {
                    if !var_names.contains(n) { var_names.push(n.clone()); }
                }
                _ => {}
            }
        }
        for p in params {
            if !var_names.contains(p) { var_names.push(p.clone()); }
        }

        // Declarar variables Cranelift (todas i64 = puntero a OrionVal)
        let mut var_table: HashMap<String, Variable> = HashMap::new();
        for (vid, name) in var_names.iter().enumerate() {
            let v = Variable::from_u32(vid as u32);
            builder.declare_var(v, types::I64);
            var_table.insert(name.clone(), v);
        }

        // Bloque de entrada
        let entry_block = block_map[&0];
        if !params.is_empty() {
            builder.append_block_params_for_function_params(entry_block);
        }
        builder.switch_to_block(entry_block);

        // Inicializar todas las variables a null
        for (name, &v) in &var_table {
            if !params.contains(name) {
                let call = builder.ins().call(make_null_ref, &[]);
                builder.def_var(v, builder.inst_results(call)[0]);
            }
        }

        // Bind de parámetros
        if !params.is_empty() {
            let bparams: Vec<cranelift_codegen::ir::Value> =
                builder.block_params(entry_block).to_vec();
            for (i, pname) in params.iter().enumerate() {
                if i < bparams.len() {
                    if let Some(&var) = var_table.get(pname) {
                        builder.def_var(var, bparams[i]);
                    }
                }
            }
        }

        // Compilar instrucciones
        let mut stack: Vec<cranelift_codegen::ir::Value> = Vec::new();
        let mut terminated = false;

        // Macro para llamadas binarias frecuentes
        macro_rules! binop {
            ($fref:expr) => {{
                let b = stack.pop().ok_or(concat!(stringify!($fref), ": pila vacía"))?;
                let a = stack.pop().ok_or(concat!(stringify!($fref), ": pila vacía"))?;
                let call = builder.ins().call($fref, &[a, b]);
                stack.push(builder.inst_results(call)[0]);
            }};
        }
        macro_rules! unop {
            ($fref:expr) => {{
                let a = stack.pop().ok_or(concat!(stringify!($fref), ": pila vacía"))?;
                let call = builder.ins().call($fref, &[a]);
                stack.push(builder.inst_results(call)[0]);
            }};
        }

        for (i, instr) in instructions.iter().enumerate() {
            // Cambio de bloque básico
            if i > 0 && block_starts.contains(&i) {
                let next_block = block_map[&i];
                if !terminated { builder.ins().jump(next_block, &[]); }
                builder.switch_to_block(next_block);
                terminated = false;
                stack.clear();
            }
            if terminated { continue; }

            match instr {
                // ── Literales ────────────────────────────────────────────────
                Instruction::LoadNull => {
                    let call = builder.ins().call(make_null_ref, &[]);
                    stack.push(builder.inst_results(call)[0]);
                }
                Instruction::LoadInt(n) => {
                    let nv = builder.ins().iconst(types::I64, *n);
                    let call = builder.ins().call(make_int_ref, &[nv]);
                    stack.push(builder.inst_results(call)[0]);
                }
                Instruction::LoadFloat(f) => {
                    let bits = builder.ins().iconst(types::I64, f.to_bits() as i64);
                    let call = builder.ins().call(make_fbits_ref, &[bits]);
                    stack.push(builder.inst_results(call)[0]);
                }
                Instruction::LoadBool(b) => {
                    let bv = builder.ins().iconst(types::I64, if *b { 1 } else { 0 });
                    let call = builder.ins().call(make_bool_ref, &[bv]);
                    stack.push(builder.inst_results(call)[0]);
                }
                Instruction::LoadStr(s) => {
                    let mut bytes = s.as_bytes().to_vec();
                    bytes.push(0u8);
                    let raw = bytes.as_ptr() as i64;
                    self.string_storage.push(bytes);
                    let ptr = builder.ins().iconst(types::I64, raw);
                    let call = builder.ins().call(make_str_ref, &[ptr]);
                    stack.push(builder.inst_results(call)[0]);
                }

                // ── Variables ────────────────────────────────────────────────
                Instruction::LoadVar(name) => {
                    let &var = var_table.get(name)
                        .ok_or_else(|| format!("JIT: variable '{name}' no declarada"))?;
                    stack.push(builder.use_var(var));
                }
                Instruction::StoreVar(name) | Instruction::StoreConst(name) => {
                    let val = stack.pop().ok_or("StoreVar: pila vacía")?;
                    if let Some(&var) = var_table.get(name) {
                        builder.def_var(var, val);
                    }
                }

                // ── Aritmética ───────────────────────────────────────────────
                Instruction::Add => { binop!(add_ref); }
                Instruction::Sub => { binop!(sub_ref); }
                Instruction::Mul => { binop!(mul_ref); }
                Instruction::Div => { binop!(div_ref); }
                Instruction::Mod => { binop!(mod_ref); }
                Instruction::Pow => { binop!(pow_ref); }
                Instruction::Neg => { unop!(neg_ref); }

                // ── Comparación ──────────────────────────────────────────────
                Instruction::Eq    => { binop!(eq_ref);   }
                Instruction::NotEq => { binop!(neq_ref);  }
                Instruction::Lt    => { binop!(lt_ref);   }
                Instruction::LtEq  => { binop!(lteq_ref); }
                Instruction::Gt    => { binop!(gt_ref);   }
                Instruction::GtEq  => { binop!(gteq_ref); }

                // ── Lógica ───────────────────────────────────────────────────
                Instruction::And => { binop!(and_ref); }
                Instruction::Or  => { binop!(or_ref);  }
                Instruction::Not => { unop!(not_ref);  }

                // ── Control de flujo ─────────────────────────────────────────
                Instruction::Jump(target) => {
                    let tb = *block_map.get(target)
                        .ok_or_else(|| format!("Jump: bloque {target} no encontrado"))?;
                    builder.ins().jump(tb, &[]);
                    terminated = true;
                }
                Instruction::JumpIfFalse(target) => {
                    let val = stack.pop().ok_or("JumpIfFalse: pila vacía")?;
                    let cond_call = builder.ins().call(is_truthy_ref, &[val]);
                    let cond = builder.inst_results(cond_call)[0];
                    let false_block = *block_map.get(target)
                        .ok_or_else(|| format!("JumpIfFalse: {target} no encontrado"))?;
                    let true_block  = *block_map.get(&(i + 1))
                        .ok_or_else(|| format!("JumpIfFalse: {} no encontrado", i + 1))?;
                    builder.ins().brif(cond, true_block, &[], false_block, &[]);
                    terminated = true;
                }
                Instruction::JumpIfTrue(target) => {
                    let val = stack.pop().ok_or("JumpIfTrue: pila vacía")?;
                    let cond_call = builder.ins().call(is_truthy_ref, &[val]);
                    let cond = builder.inst_results(cond_call)[0];
                    let true_block  = *block_map.get(target)
                        .ok_or_else(|| format!("JumpIfTrue: {target} no encontrado"))?;
                    let false_block = *block_map.get(&(i + 1))
                        .ok_or_else(|| format!("JumpIfTrue: {} no encontrado", i + 1))?;
                    builder.ins().brif(cond, true_block, &[], false_block, &[]);
                    terminated = true;
                }

                // ── Funciones ────────────────────────────────────────────────
                Instruction::MakeFunction(_, _, _) => { /* no-op: ya compilado */ }
                Instruction::Call(fname, n_args) => {
                    let n = *n_args as usize;
                    let mut args: Vec<cranelift_codegen::ir::Value> = (0..n)
                        .map(|_| stack.pop().ok_or("Call: pila vacía"))
                        .collect::<Result<_, _>>()?;
                    args.reverse();
                    let fref = *user_fn_refs.get(fname)
                        .ok_or_else(|| format!("JIT: función '{fname}' no encontrada"))?;
                    let call = builder.ins().call(fref, &args);
                    stack.push(builder.inst_results(call)[0]);
                }
                Instruction::Return => {
                    if is_main {
                        builder.ins().return_(&[]);
                    } else {
                        let ret = if let Some(v) = stack.pop() {
                            v
                        } else {
                            let c = builder.ins().call(make_null_ref, &[]);
                            builder.inst_results(c)[0]
                        };
                        builder.ins().return_(&[ret]);
                    }
                    terminated = true;
                }

                // ── I/O ──────────────────────────────────────────────────────
                Instruction::Show => {
                    let val = stack.pop().ok_or("Show: pila vacía")?;
                    builder.ins().call(show_ref, &[val]);
                }

                // ── Stack ────────────────────────────────────────────────────
                Instruction::Pop => { stack.pop(); }
                Instruction::Dup => {
                    let top = stack.last().cloned().ok_or("Dup: pila vacía")?;
                    stack.push(top);
                }

                // ── Terminadores ─────────────────────────────────────────────
                Instruction::Halt => {
                    if is_main {
                        builder.ins().return_(&[]);
                    } else {
                        let c = builder.ins().call(make_null_ref, &[]);
                        let nv = builder.inst_results(c)[0];
                        builder.ins().return_(&[nv]);
                    }
                    terminated = true;
                }

                other => {
                    return Err(format!("JIT: instrucción no soportada en esta fase: {other:?}"));
                }
            }
        }

        // Return implícito al final de bloque
        if !terminated {
            if is_main {
                builder.ins().return_(&[]);
            } else {
                let c = builder.ins().call(make_null_ref, &[]);
                let nv = builder.inst_results(c)[0];
                builder.ins().return_(&[nv]);
            }
        }

        builder.seal_all_blocks();
        builder.finalize();
        Ok(())
    }
}

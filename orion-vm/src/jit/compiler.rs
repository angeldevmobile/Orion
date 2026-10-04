//! Bytecode → Cranelift (JIT y AOT). Los valores son i64 con la codificación de
//! `runtime::alloc_val`: la aritmética y las comparaciones entre enteros o
//! decimales van en línea (módulo `inline`); el resto llama al runtime.

use std::collections::HashSet;
use indexmap::IndexMap as HashMap;

use cranelift_codegen::ir::{types, AbiParam, InstBuilder, Value};
use cranelift_codegen::settings;
use cranelift_codegen::settings::Configurable;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, DataDescription, DataId, FuncId, Linkage, Module};

use crate::bytecode::OrionBytecode;
use crate::instruction::Instruction;

//     IDs de runtime                                                           

#[derive(Clone)]
struct RuntimeIds {
    // Constructores escalares
    make_null:       FuncId,
    make_int:        FuncId,
    make_str:        FuncId,
    // Colecciones — JIT-2
    push_arg:        FuncId,
    make_list_n:     FuncId,
    make_dict_n:     FuncId,
    get_index:       FuncId,
    set_index:       FuncId,
    // Manejo de errores — JIT-3
    set_error:       FuncId,
    take_error:      FuncId,
    raise_exit:      FuncId,
    error_pending:   FuncId,
    // I/O nativo — JIT-4
    read_input:         FuncId,
    read_input_choices: FuncId,
    read_file:          FuncId,
    // Globales: el nivel superior publica al asignar, las funciones leen lo que
    // no es suyo. Sin esto una función no ve nada definido fuera de ella.
    store_global:       FuncId,
    load_global:        FuncId,
    write_file:         FuncId,
    read_env:           FuncId,
    use_module:         FuncId,
    // OOP — JIT-5
    create_instance:    FuncId,  // rt_create_instance_and_init(name_ptr, n_args) -> i64
    get_attr:           FuncId,  // rt_get_attr(obj, name_ptr) -> i64
    set_attr:           FuncId,  // rt_set_attr(obj, name_ptr, val)
    is_instance:        FuncId,  // rt_is_instance(obj, name_ptr) -> i64
    get_self:           FuncId,  // rt_get_current_self() -> i64
    push_self:          FuncId,  // rt_push_self(inst)
    pop_self:           FuncId,  // rt_pop_self()
    get_self_field:     FuncId,  // rt_get_self_field(name_ptr) -> i64
    set_self_field:     FuncId,  // rt_set_self_field(name_ptr, val)
    call_method:        FuncId,  // rt_call_method(obj, name_ptr, n_args) -> i64
    // Builtins vía puente VM — rt_call_builtin(name_ptr, n_args) -> i64
    call_builtin:       FuncId,
    // JIT-6: Closures y Async
    make_closure:    FuncId,  // rt_make_closure(fn_name_ptr) -> i64
    call_async:      FuncId,  // rt_call_async(fn_name_ptr, n_args) -> i64
    spawn:           FuncId,  // rt_spawn: igual, para una tarea que nadie espera
    rt_await:        FuncId,  // rt_await(task) -> i64
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

//     Análisis de bloques básicos                                              

fn find_block_starts(instructions: &[Instruction]) -> HashSet<usize> {
    let mut starts = HashSet::new();
    starts.insert(0);
    starts.insert(instructions.len());
    for (i, instr) in instructions.iter().enumerate() {
        match instr {
            Instruction::Jump(t) => { starts.insert(*t); starts.insert(i + 1); }
            Instruction::JumpIfFalse(t) | Instruction::JumpIfTrue(t)
            | Instruction::JumpIfFalseOrPop(t) | Instruction::JumpIfTrueOrPop(t) => {
                starts.insert(*t); starts.insert(i + 1);
            }
            // JIT-3: el handler y el bloque post-attempt son targets de salto
            Instruction::BeginAttempt(h) => {
                starts.insert(*h);    // bloque del handler
                starts.insert(i + 1); // cuerpo del attempt
            }
            Instruction::EndAttempt(e) => {
                starts.insert(*e);    // bloque final (post-handler)
                starts.insert(i + 1); // primer instrucción del handler body
            }
            // Raise es una terminación implícita de bloque
            Instruction::Raise => { starts.insert(i + 1); }
            _ => {}
        }
    }
    starts
}

//     Elegibilidad                                                             

/// Nombres propios de un cuerpo: parámetros, campos (en un act) y lo que asigna.
fn locales_de(instructions: &[Instruction], params: &[String], fields: &[String]) -> HashSet<String> {
    instructions.iter()
        .filter_map(|i| match i {
            Instruction::StoreVar(n) | Instruction::StoreConst(n) => Some(n.clone()),
            Instruction::UseModule(_, alias, _) => Some(alias.clone()),
            _ => None,
        })
        .chain(params.iter().cloned())
        .chain(fields.iter().cloned())
        .collect()
}

/// Lo que un cuerpo lee sin ser suyo: lo busca en la tabla de globales.
fn globales_leidas(instructions: &[Instruction], params: &[String], fields: &[String]) -> Vec<String> {
    let locales = locales_de(instructions, params, fields);
    instructions.iter()
        .filter_map(|i| match i {
            Instruction::LoadVar(n) if !locales.contains(n) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

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
            | Instruction::JumpIfFalseOrPop(_)
            | Instruction::JumpIfTrueOrPop(_)
            | Instruction::ToBool
            | Instruction::Show
            | Instruction::Pop
            | Instruction::Dup
            | Instruction::Call(_, _)
            | Instruction::MakeFunction(_, _, _)
            | Instruction::Return
            | Instruction::Halt
            // JIT-2: colecciones
            | Instruction::MakeList(_)
            | Instruction::MakeDict(_)
            | Instruction::GetIndex
            | Instruction::SetIndex
            // JIT-3: manejo de errores
            | Instruction::BeginAttempt(_)
            | Instruction::EndAttempt(_)
            | Instruction::Raise
            // JIT-4: I/O nativo y módulos
            | Instruction::ReadInput { .. }
            | Instruction::ReadFile(_)
            | Instruction::WriteFile(_)
            | Instruction::ReadEnv(_)
            | Instruction::UseModule(_, _, _)
            // JIT-5: OOP
            | Instruction::DefineShape(_)
            | Instruction::GetAttr(_)
            | Instruction::SetAttr(_)
            | Instruction::IsInstance(_)
            | Instruction::PushSelf
            | Instruction::CallMethod(_, _)
            // JIT-6: Closures y Async
            | Instruction::MakeClosure(_)
            | Instruction::CallAsync(_, _)
            | Instruction::Await
    )
}

//     Compilador JIT                                                           

/// Generador Cranelift: `JITModule` compila en memoria y `ObjectModule` emite un
/// objeto para enlazar (AOT). Solo cambia cómo se emiten las cadenas.
pub struct CodeGen<M: Module> {
    module:         M,
    fn_counter:     usize,
    fn_cache:       HashMap<String, FuncId>,
    /// Modo JIT: mantiene vivos los buffers a los que apunta el código emitido.
    string_storage: Vec<Vec<u8>>,
    /// Modo AOT: cadenas ya emitidas como datos del objeto, deduplicadas.
    str_data:       HashMap<String, DataId>,
    /// Si se emite a un objeto en vez de a memoria ejecutable.
    aot:            bool,
    rt:             Option<RuntimeIds>,
    /// Variables de main que alguna función o act lee: solo esas se copian
    /// a la tabla de globales; las demás viven en registros.
    publicadas:     HashSet<String>,
}

/// El compilador JIT es el generador sobre el backend en memoria.
pub type JitCompiler = CodeGen<JITModule>;

/// Resultado de compilar: el JIT resuelve los `FuncId` a direcciones y el AOT
/// los referencia por símbolo.
pub struct CompiledProgram {
    /// `None` solo para el programa vacío, que no tiene nada que ejecutar.
    pub main:      Option<FuncId>,
    /// Funciones de usuario: (nombre, id). Necesarias para closures y async.
    pub functions: Vec<(String, FuncId)>,
    /// Acts de shapes: (shape, act, id).
    pub methods:   Vec<(String, String, FuncId)>,
    /// Shapes declaradas: (nombre, campos, padres).
    pub shapes:    Vec<(String, Vec<String>, Vec<String>)>,
}

impl CompiledProgram {
    fn empty() -> Self {
        CompiledProgram {
            main: None, functions: Vec::new(),
            methods: Vec::new(), shapes: Vec::new(),
        }
    }
}

impl CodeGen<JITModule> {
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
        sym!("rt_make_str",        super::runtime::rt_make_str);
        sym!("rt_push_arg",        super::runtime::rt_push_arg);
        sym!("rt_make_list_n",     super::runtime::rt_make_list_n);
        sym!("rt_make_dict_n",     super::runtime::rt_make_dict_n);
        sym!("rt_get_index",       super::runtime::rt_get_index);
        sym!("rt_set_index",       super::runtime::rt_set_index);
        sym!("rt_set_error",           super::runtime::rt_set_error);
        sym!("rt_take_error",          super::runtime::rt_take_error);
        sym!("rt_error_pending",       super::runtime::rt_error_pending);
        sym!("rt_raise_exit",          super::runtime::rt_raise_exit);
        sym!("rt_read_input",              super::runtime::rt_read_input);
        sym!("rt_read_input_choices",      super::runtime::rt_read_input_choices);
        sym!("rt_read_file",               super::runtime::rt_read_file);
        sym!("rt_write_file",              super::runtime::rt_write_file);
        sym!("rt_read_env",                super::runtime::rt_read_env);
        sym!("rt_use_module",              super::runtime::rt_use_module);
        sym!("rt_store_global",            super::runtime::rt_store_global);
        sym!("rt_load_global",             super::runtime::rt_load_global);
        sym!("rt_create_instance_and_init",super::runtime_oop::rt_create_instance_and_init);
        sym!("rt_get_attr",                super::runtime_oop::rt_get_attr);
        sym!("rt_set_attr",                super::runtime_oop::rt_set_attr);
        sym!("rt_is_instance",             super::runtime_oop::rt_is_instance);
        sym!("rt_get_current_self",        super::runtime_oop::rt_get_current_self);
        sym!("rt_push_self",               super::runtime_oop::rt_push_self);
        sym!("rt_pop_self",                super::runtime_oop::rt_pop_self);
        sym!("rt_get_self_field",          super::runtime_oop::rt_get_self_field);
        sym!("rt_set_self_field",          super::runtime_oop::rt_set_self_field);
        sym!("rt_call_method",             super::runtime_oop::rt_call_method);
        sym!("rt_call_builtin",            super::bridge::rt_call_builtin);
        sym!("rt_make_closure",            super::runtime::rt_make_closure);
        sym!("rt_call_async",              super::runtime::rt_call_async);
        sym!("rt_spawn",                   super::runtime::rt_spawn);
        sym!("rt_await",                   super::runtime::rt_await);
        sym!("rt_show",                    super::runtime::rt_show);
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
        Ok(CodeGen {
            module, fn_counter: 0, fn_cache: HashMap::new(),
            string_storage: Vec::new(), str_data: HashMap::new(),
            aot: false, rt: None, publicadas: HashSet::new(),
        })
    }

    /// Compila y ejecuta un programa completo (main + funciones).
    pub fn run_program(&mut self, bc: &OrionBytecode) -> Result<bool, String> {
        let prog = match self.compile_program(bc)? {
            Some(p) => p,
            None => return Ok(false),
        };

        self.module.finalize_definitions()
            .map_err(|e| format!("JIT finalize: {e}"))?;

        for (name, fields, parents) in &prog.shapes {
            super::runtime_oop::register_shape_info(name, fields.clone(), parents.clone());
        }
        // Punteros reales para CallAsync / MakeClosure y para el dispatch de acts.
        for (name, fid) in &prog.functions {
            let fn_ptr = self.module.get_finalized_function(*fid) as i64;
            super::runtime::register_jit_fn(name, fn_ptr);
        }
        for (shape, act, fid) in &prog.methods {
            let fn_ptr = self.module.get_finalized_function(*fid) as i64;
            super::runtime_oop::register_method(shape, act, fn_ptr);
        }

        let Some(main_id) = prog.main else { return Ok(true) };
        let code_ptr = self.module.get_finalized_function(main_id);
        unsafe {
            let f: extern "C" fn() = std::mem::transmute(code_ptr);
            f();
        }
        Ok(true)
    }
}

impl<M: Module> CodeGen<M> {
    pub fn new_aot(module: M) -> Self {
        CodeGen {
            module, fn_counter: 0, fn_cache: HashMap::new(),
            string_storage: Vec::new(), str_data: HashMap::new(),
            aot: true, rt: None, publicadas: HashSet::new(),
        }
    }

    pub fn module_mut(&mut self) -> &mut M {
        &mut self.module
    }

    pub fn into_module(self) -> M {
        self.module
    }

    /// Emite literales NUL-terminados como datos del objeto y devuelve sus ids.
    pub fn emit_literals(
        &mut self,
        lits: &HashMap<String, String>,
    ) -> Result<HashMap<String, DataId>, String> {
        let mut out = HashMap::new();
        for (key, text) in lits {
            let sym = format!("orion_lit_{}", self.str_data.len() + out.len());
            let id = self.module
                .declare_data(&sym, Linkage::Local, false, false)
                .map_err(|e| format!("declare_data '{key}': {e}"))?;
            let mut desc = DataDescription::new();
            let mut bytes = text.as_bytes().to_vec();
            bytes.push(0u8);
            desc.define(bytes.into_boxed_slice());
            self.module.define_data(id, &desc)
                .map_err(|e| format!("define_data '{key}': {e}"))?;
            out.insert(key.clone(), id);
        }
        Ok(out)
    }

    /// Puntero a un literal C (NUL-terminado) utilizable desde el código emitido.
    fn cstr_ptr(&mut self, builder: &mut FunctionBuilder, s: &str) -> Value {
        if !self.aot {
            let mut bytes = s.as_bytes().to_vec();
            bytes.push(0u8);
            let raw = bytes.as_ptr() as i64;
            self.string_storage.push(bytes);
            return builder.ins().iconst(types::I64, raw);
        }

        let data_id = match self.str_data.get(s) {
            Some(&id) => id,
            None => {
                let sym = format!("orion_str_{}", self.str_data.len());
                let id = self.module
                    .declare_data(&sym, Linkage::Local, false, false)
                    .expect("declare_data de literal");
                let mut desc = DataDescription::new();
                let mut bytes = s.as_bytes().to_vec();
                bytes.push(0u8);
                desc.define(bytes.into_boxed_slice());
                self.module.define_data(id, &desc).expect("define_data de literal");
                self.str_data.insert(s.to_string(), id);
                id
            }
        };
        let gv = self.module.declare_data_in_func(data_id, builder.func);
        builder.ins().symbol_value(types::I64, gv)
    }

    fn ensure_runtime(&mut self) -> Result<(), String> {
        if self.rt.is_some() { return Ok(()); }

        let i = types::I64;
        macro_rules! decl {
            ($name:literal, [$($p:expr),*], [$($r:expr),*]) => {{
                #[allow(unused_mut)]
                let mut sig = self.module.make_signature();
                $(sig.params.push(AbiParam::new($p));)*
                $(sig.returns.push(AbiParam::new($r));)*
                self.module.declare_function($name, Linkage::Import, &sig)
                    .map_err(|e| e.to_string())?
            }};
        }

        let make_null       = decl!("rt_make_null",       [],          [i]);
        let make_int        = decl!("rt_make_int",        [i],         [i]);
        let make_str        = decl!("rt_make_str",        [i],         [i]);
        let push_arg        = decl!("rt_push_arg",        [i],         []);
        let make_list_n     = decl!("rt_make_list_n",     [i],         [i]);
        let make_dict_n     = decl!("rt_make_dict_n",     [i],         [i]);
        let get_index       = decl!("rt_get_index",       [i, i],      [i]);
        let set_index       = decl!("rt_set_index",       [i, i, i],   [i]);
        let set_error          = decl!("rt_set_error",          [i],         []);
        let take_error         = decl!("rt_take_error",         [],          [i]);
        let raise_exit         = decl!("rt_raise_exit",         [i],         []);
        let error_pending      = decl!("rt_error_pending",      [],          [i]);
        let read_input         = decl!("rt_read_input",              [i, i],    [i]);
        let read_input_choices = decl!("rt_read_input_choices",      [i, i, i], [i]);
        let read_file          = decl!("rt_read_file",               [i, i],    [i]);
        let write_file         = decl!("rt_write_file",              [i, i, i], []);
        let read_env           = decl!("rt_read_env",                [i, i],    [i]);
        let use_module         = decl!("rt_use_module",              [i],       [i]);
        // Globales: el nivel superior publica, las funciones leen. Ver el
        // comentario de `rt_store_global` en runtime.rs.
        let store_global       = decl!("rt_store_global",            [i, i],    []);
        let load_global        = decl!("rt_load_global",             [i],       [i]);
        let create_instance    = decl!("rt_create_instance_and_init",[i, i],    [i]);
        let get_attr           = decl!("rt_get_attr",                [i, i],    [i]);
        let set_attr           = decl!("rt_set_attr",                [i, i, i], []);
        let is_instance        = decl!("rt_is_instance",             [i, i],    [i]);
        let get_self           = decl!("rt_get_current_self",        [],        [i]);
        let push_self          = decl!("rt_push_self",               [i],       []);
        let pop_self           = decl!("rt_pop_self",                [],        []);
        let get_self_field     = decl!("rt_get_self_field",          [i],       [i]);
        let set_self_field     = decl!("rt_set_self_field",          [i, i],    []);
        let call_method        = decl!("rt_call_method",             [i, i, i], [i]);
        let call_builtin       = decl!("rt_call_builtin",            [i, i],    [i]);
        let make_closure       = decl!("rt_make_closure",            [i],       [i]);
        let call_async         = decl!("rt_call_async",              [i, i],    [i]);
        let spawn              = decl!("rt_spawn",                   [i, i],    [i]);
        let rt_await           = decl!("rt_await",                   [i],       [i]);
        let show               = decl!("rt_show",                    [i],       []);
        let is_truthy       = decl!("rt_is_truthy",       [i],         [i]);
        let add             = decl!("rt_add",             [i, i],      [i]);
        let sub             = decl!("rt_sub",             [i, i],      [i]);
        let mul             = decl!("rt_mul",             [i, i],      [i]);
        let div             = decl!("rt_div",             [i, i],      [i]);
        let rt_mod          = decl!("rt_mod",             [i, i],      [i]);
        let pow             = decl!("rt_pow",             [i, i],      [i]);
        let neg             = decl!("rt_neg",             [i],         [i]);
        let eq              = decl!("rt_eq",              [i, i],      [i]);
        let neq             = decl!("rt_neq",             [i, i],      [i]);
        let lt              = decl!("rt_lt",              [i, i],      [i]);
        let lteq            = decl!("rt_lteq",            [i, i],      [i]);
        let gt              = decl!("rt_gt",              [i, i],      [i]);
        let gteq            = decl!("rt_gteq",            [i, i],      [i]);
        let and             = decl!("rt_and",             [i, i],      [i]);
        let or              = decl!("rt_or",              [i, i],      [i]);
        let not             = decl!("rt_not",             [i],         [i]);

        self.rt = Some(RuntimeIds {
            make_null, make_int, make_str,
            push_arg, make_list_n, make_dict_n, get_index, set_index,
            set_error, take_error, raise_exit, error_pending,
            read_input, read_input_choices, read_file, write_file, read_env, use_module,
            store_global, load_global,
            create_instance, get_attr, set_attr, is_instance,
            get_self, push_self, pop_self, get_self_field, set_self_field, call_method,
            call_builtin,
            make_closure, call_async, spawn, rt_await,
            show, is_truthy,
            add, sub, mul, div, rt_mod, pow, neg,
            eq, neq, lt, lteq, gt, gteq,
            and, or, not,
        });
        Ok(())
    }

    //     API pública
    pub fn compile_program(&mut self, bc: &OrionBytecode) -> Result<Option<CompiledProgram>, String> {
        let callable: HashSet<&str> = bc.functions.keys()
            .chain(bc.shapes.keys())
            .map(|s| s.as_str())
            .collect();
        let eligible = |instr: &Instruction| -> bool {
            match instr {
                // Un Call resuelve a: función de usuario, shape, o builtin puenteable.
                Instruction::Call(name, argc) => {
                    if let Some(f) = bc.functions.get(name.as_str()) {
                        *argc as usize == f.params.len()
                    } else if bc.shapes.contains_key(name.as_str()) {
                        true
                    } else {
                        super::bridge::is_jit_builtin(name.as_str())
                    }
                }
                // Async solo sobre funciones de usuario (no builtins).
                Instruction::CallAsync(name, _) => callable.contains(name.as_str()),
                other => is_eligible(other),
            }
        };

        // Elegibilidad
        for instr in &bc.main {
            if !eligible(instr) { return Ok(None); }
        }
        for fdef in bc.functions.values() {
            for instr in fdef.body.iter() {
                if !eligible(instr) { return Ok(None); }
            }
        }
        for shape in bc.shapes.values() {
            let check_act = |body: &[Instruction]| body.iter().all(&eligible);
            if let Some(oc) = &shape.on_create {
                if !check_act(&oc.body) { return Ok(None); }
            }
            for act in shape.acts.values() {
                if !check_act(&act.body) { return Ok(None); }
            }
        }
        if bc.main.is_empty() { return Ok(Some(CompiledProgram::empty())); }

        self.ensure_runtime()?;

        // Conjunto de nombres de shapes para dispatch en Call
        let shape_names: HashSet<String> = bc.shapes.keys().cloned().collect();

        // JIT-5: registrar info de shapes en TLS (antes de ejecutar)
        for (sname, sdef) in &bc.shapes {
            let fields: Vec<String> = sdef.fields.iter().map(|f| f.name.clone()).collect();
            let parents = sdef.using.clone();
            super::runtime_oop::register_shape_info(sname, fields, parents);
        }

        // 1. Declarar funciones de usuario
        let fn_names: Vec<String> = bc.functions.keys().cloned().collect();
        for name in &fn_names {
            let n_params = bc.functions[name].params.len();
            let mut sig = self.module.make_signature();
            for _ in 0..n_params { sig.params.push(AbiParam::new(types::I64)); }
            sig.returns.push(AbiParam::new(types::I64));
            let simbolo = if self.aot { format!("orion_fn_{name}") } else { name.clone() };
            let fid = self.module.declare_function(&simbolo, Linkage::Local, &sig)
                .map_err(|e| e.to_string())?;
            self.fn_cache.insert(name.clone(), fid);
        }

        struct ActEntry { jit_name: String, shape: String, act: String, params: Vec<String>, body: Vec<Instruction> }
        let mut act_entries: Vec<ActEntry> = Vec::new();
        for (sname, sdef) in &bc.shapes {
            let field_names: Vec<String> = sdef.fields.iter().map(|f| f.name.clone()).collect();
            // on_create
            if let Some(oc) = &sdef.on_create {
                let jit_name = format!("shape__{}__on_create", sname);
                let mut sig = self.module.make_signature();
                for _ in &oc.params { sig.params.push(AbiParam::new(types::I64)); }
                sig.returns.push(AbiParam::new(types::I64));
                let fid = self.module.declare_function(&jit_name, Linkage::Local, &sig)
                    .map_err(|e| e.to_string())?;
                self.fn_cache.insert(jit_name.clone(), fid);
                act_entries.push(ActEntry {
                    jit_name, shape: sname.clone(), act: "on_create".to_string(),
                    params: oc.params.clone(), body: oc.body.to_vec(),
                });
            }
            // acts regulares
            for (aname, adef) in &sdef.acts {
                let jit_name = format!("shape__{}__{}", sname, aname);
                let mut sig = self.module.make_signature();
                for _ in &adef.params { sig.params.push(AbiParam::new(types::I64)); }
                sig.returns.push(AbiParam::new(types::I64));
                let fid = self.module.declare_function(&jit_name, Linkage::Local, &sig)
                    .map_err(|e| e.to_string())?;
                self.fn_cache.insert(jit_name.clone(), fid);
                act_entries.push(ActEntry {
                    jit_name, shape: sname.clone(), act: aname.clone(),
                    params: adef.params.clone(), body: adef.body.to_vec(),
                });
            }
            let _ = field_names; // usado más abajo en fill_act_body
        }

        // 3. Declarar main
        let main_name = format!("orion_jit_main_{}", self.fn_counter);
        self.fn_counter += 1;
        let main_sig = self.module.make_signature();
        let main_id = self.module.declare_function(&main_name, Linkage::Local, &main_sig)
            .map_err(|e| e.to_string())?;

        self.publicadas = bc.functions.values()
            .flat_map(|f| globales_leidas(&f.body, &f.params, &[]))
            .chain(act_entries.iter().flat_map(|e| {
                let fields: Vec<String> = bc.shapes[&e.shape]
                    .fields.iter().map(|f| f.name.clone()).collect();
                globales_leidas(&e.body, &e.params, &fields)
            }))
            .collect();

        // 4. Definir cuerpos de funciones de usuario
        for name in &fn_names {
            let fdef = bc.functions[name].clone();
            let fid = self.fn_cache[name];
            let n_params = fdef.params.len();
            let mut fn_sig = self.module.make_signature();
            for _ in 0..n_params { fn_sig.params.push(AbiParam::new(types::I64)); }
            fn_sig.returns.push(AbiParam::new(types::I64));
            let mut ctx = self.module.make_context();
            ctx.func.signature = fn_sig;
            self.fill_function_body(&fdef.body, &fdef.params, &mut ctx, false, &shape_names, None)?;
            self.module.define_function(fid, &mut ctx)
                .map_err(|e| format!("JIT define fn '{name}': {e}"))?;
            self.module.clear_context(&mut ctx);
        }

        // 5. Definir cuerpos de acts
        for entry in &act_entries {
            let fid = self.fn_cache[&entry.jit_name];
            let n_params = entry.params.len();
            let mut fn_sig = self.module.make_signature();
            for _ in 0..n_params { fn_sig.params.push(AbiParam::new(types::I64)); }
            fn_sig.returns.push(AbiParam::new(types::I64));
            // Campos del shape para este act
            let field_names: Vec<String> = bc.shapes[&entry.shape]
                .fields.iter().map(|f| f.name.clone()).collect();
            let mut ctx = self.module.make_context();
            ctx.func.signature = fn_sig;
            self.fill_function_body(
                &entry.body, &entry.params, &mut ctx, false,
                &shape_names, Some(&field_names),
            )?;
            self.module.define_function(fid, &mut ctx)
                .map_err(|e| format!("JIT define act '{}': {e}", entry.jit_name))?;
            self.module.clear_context(&mut ctx);
        }

        // 6. Definir main
        let mut ctx = self.module.make_context();
        ctx.func.signature = main_sig;
        self.fill_function_body(&bc.main, &[], &mut ctx, true, &shape_names, None)?;
        self.module.define_function(main_id, &mut ctx)
            .map_err(|e| format!("JIT define main: {e}"))?;
        self.module.clear_context(&mut ctx);

        // 7. Entregar los ids: finalizar y ejecutar (JIT) o emitir el objeto
        //    (AOT) es responsabilidad del backend.
        Ok(Some(CompiledProgram {
            main: Some(main_id),
            functions: fn_names.iter()
                .map(|n| (n.clone(), self.fn_cache[n]))
                .collect(),
            methods: act_entries.iter()
                .map(|e| (e.shape.clone(), e.act.clone(), self.fn_cache[&e.jit_name]))
                .collect(),
            shapes: bc.shapes.iter()
                .map(|(n, d)| (
                    n.clone(),
                    d.fields.iter().map(|f| f.name.clone()).collect(),
                    d.using.clone(),
                ))
                .collect(),
        }))
    }

    //     Generación de IR

    fn fill_function_body(
        &mut self,
        instructions: &[Instruction],
        params: &[String],
        ctx: &mut cranelift_codegen::Context,
        is_main: bool,
        shape_names: &HashSet<String>,
        field_names: Option<&[String]>,  // Some para act bodies; activa sync-back de campos
    ) -> Result<(), String> {
        let block_starts = find_block_starts(instructions);
        let mut sorted_starts: Vec<usize> = block_starts.iter().cloned().collect();
        sorted_starts.sort_unstable();

        let rt = self.rt.as_ref().expect("the runtime must be initialized").clone();
        let cached_fns: Vec<(String, FuncId)> = self.fn_cache
            .iter().map(|(k, &v)| (k.clone(), v)).collect();

        // Declarar todas las func-refs ANTES de crear el builder
        let make_null_ref   = self.module.declare_func_in_func(rt.make_null,       &mut ctx.func);
        let make_int_ref    = self.module.declare_func_in_func(rt.make_int,        &mut ctx.func);
        let make_str_ref    = self.module.declare_func_in_func(rt.make_str,        &mut ctx.func);
        let push_arg_ref    = self.module.declare_func_in_func(rt.push_arg,        &mut ctx.func);
        let make_list_n_ref = self.module.declare_func_in_func(rt.make_list_n,     &mut ctx.func);
        let make_dict_n_ref = self.module.declare_func_in_func(rt.make_dict_n,     &mut ctx.func);
        let get_index_ref   = self.module.declare_func_in_func(rt.get_index,       &mut ctx.func);
        let set_index_ref   = self.module.declare_func_in_func(rt.set_index,       &mut ctx.func);
        let set_error_ref          = self.module.declare_func_in_func(rt.set_error,          &mut ctx.func);
        let take_error_ref         = self.module.declare_func_in_func(rt.take_error,         &mut ctx.func);
        let raise_exit_ref         = self.module.declare_func_in_func(rt.raise_exit,         &mut ctx.func);
        let error_pending_ref      = self.module.declare_func_in_func(rt.error_pending,      &mut ctx.func);
        let read_input_ref         = self.module.declare_func_in_func(rt.read_input,         &mut ctx.func);
        let read_input_choices_ref = self.module.declare_func_in_func(rt.read_input_choices, &mut ctx.func);
        let read_file_ref          = self.module.declare_func_in_func(rt.read_file,          &mut ctx.func);
        let write_file_ref         = self.module.declare_func_in_func(rt.write_file,         &mut ctx.func);
        let read_env_ref           = self.module.declare_func_in_func(rt.read_env,           &mut ctx.func);
        let use_module_ref         = self.module.declare_func_in_func(rt.use_module,         &mut ctx.func);
        let store_global_ref       = self.module.declare_func_in_func(rt.store_global,       &mut ctx.func);
        let load_global_ref        = self.module.declare_func_in_func(rt.load_global,        &mut ctx.func);
        let create_instance_ref    = self.module.declare_func_in_func(rt.create_instance,    &mut ctx.func);
        let get_attr_ref           = self.module.declare_func_in_func(rt.get_attr,           &mut ctx.func);
        let set_attr_ref           = self.module.declare_func_in_func(rt.set_attr,           &mut ctx.func);
        let is_instance_ref        = self.module.declare_func_in_func(rt.is_instance,        &mut ctx.func);
        let get_self_ref           = self.module.declare_func_in_func(rt.get_self,           &mut ctx.func);
        let _push_self_ref         = self.module.declare_func_in_func(rt.push_self,          &mut ctx.func);
        let _pop_self_ref          = self.module.declare_func_in_func(rt.pop_self,           &mut ctx.func);
        let get_self_field_ref     = self.module.declare_func_in_func(rt.get_self_field,     &mut ctx.func);
        let set_self_field_ref     = self.module.declare_func_in_func(rt.set_self_field,     &mut ctx.func);
        let call_method_ref        = self.module.declare_func_in_func(rt.call_method,        &mut ctx.func);
        let call_builtin_ref       = self.module.declare_func_in_func(rt.call_builtin,       &mut ctx.func);
        let make_closure_ref       = self.module.declare_func_in_func(rt.make_closure,       &mut ctx.func);
        let call_async_ref         = self.module.declare_func_in_func(rt.call_async,         &mut ctx.func);
        let spawn_ref              = self.module.declare_func_in_func(rt.spawn,              &mut ctx.func);
        let await_ref              = self.module.declare_func_in_func(rt.rt_await,           &mut ctx.func);
        let show_ref               = self.module.declare_func_in_func(rt.show,               &mut ctx.func);
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
                // JIT-4: UseModule almacena el namespace bajo su alias
                Instruction::UseModule(_, alias, selective) => {
                    if !var_names.contains(alias) { var_names.push(alias.clone()); }
                    for n in selective {
                        if !var_names.contains(n) { var_names.push(n.clone()); }
                    }
                }
                _ => {}
            }
        }
        for p in params {
            if !var_names.contains(p) { var_names.push(p.clone()); }
        }

        let locales = locales_de(instructions, params, field_names.unwrap_or(&[]));

        // Declarar variables Cranelift (todas i64 = puntero a OrionVal)
        let mut var_table: HashMap<String, Variable> = HashMap::new();
        for (vid, name) in var_names.iter().enumerate() {
            let v = Variable::from_u32(vid as u32);
            builder.declare_var(v, types::I64);
            var_table.insert(name.clone(), v);
        }

        // Puntos de llegada de `and`/`or`: el valor llega por dos caminos, así
        // que va en una variable de Cranelift por punto y no en la pila.
        // Valores vivos al cruzar de bloque (ternario, `and`/`or`, una suma con
        // un operando ya en la pila...): una variable por posición de la pila,
        // y Cranelift construye los phi. `profundidad` es la pila con que se
        // llega a cada bloque.
        let base_slots = var_names.len() as u32;
        let mut slots_declarados: u32 = 0;
        let mut profundidad: HashMap<usize, usize> = HashMap::new();

        // Bloque de entrada
        let entry_block = block_map[&0];
        if !params.is_empty() {
            builder.append_block_params_for_function_params(entry_block);
        }
        builder.switch_to_block(entry_block);

        // Inicializar variables: campos desde self (act body) o null (función normal)
        for (name, &v) in &var_table {
            if params.contains(name) { continue; }
            let init_val = if let Some(fields) = field_names {
                if fields.contains(name) {
                    // Leer el campo del self activo via TLS
                    let name_ptr = self.cstr_ptr(&mut builder, name);
                    let call = builder.ins().call(get_self_field_ref, &[name_ptr]);
                    builder.inst_results(call)[0]
                } else {
                    let call = builder.ins().call(make_null_ref, &[]);
                    builder.inst_results(call)[0]
                }
            } else {
                let call = builder.ins().call(make_null_ref, &[]);
                builder.inst_results(call)[0]
            };
            builder.def_var(v, init_val);
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

        // JIT-3: pre-computar qué bloques son de handler (reciben el error al entrar)
        let handler_block_addrs: HashSet<usize> = instructions.iter()
            .filter_map(|ins| if let Instruction::BeginAttempt(h) = ins { Some(*h) } else { None })
            .collect();
        // Stack de handlers en tiempo de compilación: bloque Cranelift del handler activo
        let mut handler_stack: Vec<cranelift_codegen::ir::Block> = Vec::new();

        // Bloque al que salta un error sin `handle` en esta función: sale con 0
        // (en main, lo imprime y termina). Se crea al primer uso.
        let mut propagar: Option<cranelift_codegen::ir::Block> = None;

        // Tras una llamada que puede fallar: el runtime devuelve 0 si dejó un
        // error pendiente, y entonces se salta al `handle` activo o a `propagar`.
        macro_rules! check {
            ($v:expr) => {{
                let destino = match handler_stack.last() {
                    Some(&h) => h,
                    None => *propagar.get_or_insert_with(|| builder.create_block()),
                };
                let sigue = builder.create_block();
                builder.ins().brif($v, sigue, &[], destino, &[]);
                builder.switch_to_block(sigue);
            }};
        }
        // Lo mismo para las llamadas que no devuelven valor.
        macro_rules! check_pending {
            () => {{
                let c = builder.ins().call(error_pending_ref, &[]);
                let hay = builder.inst_results(c)[0];
                let ok = builder.ins().icmp_imm(cranelift_codegen::ir::condcodes::IntCC::Equal, hay, 0);
                check!(ok);
            }};
        }

        // Macro para llamadas binarias frecuentes
        macro_rules! binop {
            ($fref:expr) => {{
                let b = stack.pop().ok_or(concat!(stringify!($fref), ": pila vacía"))?;
                let a = stack.pop().ok_or(concat!(stringify!($fref), ": pila vacía"))?;
                let call = builder.ins().call($fref, &[a, b]);
                stack.push(builder.inst_results(call)[0]);
            }};
        }
        macro_rules! binop_check {
            ($fref:expr) => {{
                binop!($fref);
                let r = *stack.last().unwrap();
                check!(r);
            }};
        }
        // Operación binaria con ruta rápida: enteros de 48 bits, decimales, y si
        // no aplica (otros tipos, desbordamiento, divisor 0) el runtime.
        macro_rules! fast_binop {
            ($slow:expr, $int:expr, $flt:expr) => {{
                let (int_op, flt_op): (inline::Op, inline::Op) = ($int, $flt);
                let b = stack.pop().ok_or("fast_binop: pila vacía")?;
                let a = stack.pop().ok_or("fast_binop: pila vacía")?;
                let merge = builder.create_block();
                builder.append_block_param(merge, types::I64);
                let int_blk = builder.create_block();
                let no_int = builder.create_block();
                let flt_blk = builder.create_block();
                let slow = builder.create_block();

                let c = inline::both_int(&mut builder, a, b);
                builder.ins().brif(c, int_blk, &[], no_int, &[]);
                builder.switch_to_block(int_blk);
                let (r, ok) = int_op(&mut builder, a, b);
                builder.ins().brif(ok, merge, &[r], slow, &[]);

                builder.switch_to_block(no_int);
                let c = inline::both_double(&mut builder, a, b);
                builder.ins().brif(c, flt_blk, &[], slow, &[]);
                builder.switch_to_block(flt_blk);
                let (r, ok) = flt_op(&mut builder, a, b);
                builder.ins().brif(ok, merge, &[r], slow, &[]);

                builder.switch_to_block(slow);
                let call = builder.ins().call($slow, &[a, b]);
                let r = builder.inst_results(call)[0];
                check!(r);
                builder.ins().jump(merge, &[r]);

                builder.switch_to_block(merge);
                stack.push(builder.block_params(merge)[0]);
            }};
        }
        // Condición de un salto: `true`/`false`/`null` se deciden en línea.
        macro_rules! branch_on {
            ($val:expr, $si:expr, $no:expr) => {{
                let v = $val;
                let resto = builder.create_block();
                let lento = builder.create_block();
                let es_true = builder.ins().icmp_imm(
                    cranelift_codegen::ir::condcodes::IntCC::Equal, v, super::runtime::VAL_TRUE);
                builder.ins().brif(es_true, $si, &[], resto, &[]);
                builder.switch_to_block(resto);
                let es_false = builder.ins().icmp_imm(
                    cranelift_codegen::ir::condcodes::IntCC::Equal, v, super::runtime::VAL_FALSE);
                let es_null = builder.ins().icmp_imm(
                    cranelift_codegen::ir::condcodes::IntCC::Equal, v, super::runtime::VAL_NULL);
                let falso = builder.ins().bor(es_false, es_null);
                builder.ins().brif(falso, $no, &[], lento, &[]);
                builder.switch_to_block(lento);
                let cond_call = builder.ins().call(is_truthy_ref, &[v]);
                let cond = builder.inst_results(cond_call)[0];
                builder.ins().brif(cond, $si, &[], $no, &[]);
            }};
        }
        macro_rules! unop {
            ($fref:expr) => {{
                let a = stack.pop().ok_or(concat!(stringify!($fref), ": pila vacía"))?;
                let call = builder.ins().call($fref, &[a]);
                stack.push(builder.inst_results(call)[0]);
            }};
        }

        macro_rules! slot {
            ($k:expr) => {{
                let k = $k as u32;
                while slots_declarados <= k {
                    builder.declare_var(Variable::from_u32(base_slots + slots_declarados), types::I64);
                    slots_declarados += 1;
                }
                Variable::from_u32(base_slots + k)
            }};
        }
        // Antes de saltar al bloque de la instrucción `$t`: deja la pila en las
        // variables de posición. Dos llegadas con pilas distintas no se pueden
        // unir: el programa va entero al intérprete.
        macro_rules! salir {
            ($t:expr) => {{
                let t: usize = $t;
                let d = stack.len();
                match profundidad.get(&t) {
                    Some(&e) if e != d => return Err(format!(
                        "JIT: el bloque {t} recibe pilas de {e} y de {d} valores")),
                    Some(_) => {}
                    None => { profundidad.insert(t, d); }
                }
                for k in 0..d {
                    let v = stack[k];
                    let var = slot!(k);
                    builder.def_var(var, v);
                }
            }};
        }

        for (i, instr) in instructions.iter().enumerate() {
            // Cambio de bloque básico
            if i > 0 && block_starts.contains(&i) {
                if !terminated {
                    salir!(i);
                    builder.ins().jump(block_map[&i], &[]);
                }
                builder.switch_to_block(block_map[&i]);
                terminated = false;
                stack.clear();
                if handler_block_addrs.contains(&i) {
                    // Inicio de un `handle`: la pila es solo el error.
                    let call = builder.ins().call(take_error_ref, &[]);
                    stack.push(builder.inst_results(call)[0]);
                } else {
                    let d = *profundidad.entry(i).or_insert(0);
                    for k in 0..d {
                        let var = slot!(k);
                        stack.push(builder.use_var(var));
                    }
                }
            }

            if terminated {
                if let Instruction::EndAttempt(_) = instr { handler_stack.pop(); }
                continue;
            }

            match instr {
                //    Literales                                                 
                // Las constantes que caben en el i64 se emiten ya codificadas.
                Instruction::LoadNull => {
                    stack.push(builder.ins().iconst(types::I64, super::runtime::VAL_NULL));
                }
                Instruction::LoadInt(n) => {
                    let v = super::runtime::alloc_val(super::runtime::TAG_INT, *n, 0.0);
                    if v & super::runtime::INT_TAG == super::runtime::INT_TAG {
                        stack.push(builder.ins().iconst(types::I64, v));
                    } else {
                        // No cabe en 48 bits: va al heap en cada ejecución.
                        let nv = builder.ins().iconst(types::I64, *n);
                        let call = builder.ins().call(make_int_ref, &[nv]);
                        stack.push(builder.inst_results(call)[0]);
                    }
                }
                Instruction::LoadFloat(f) => {
                    stack.push(builder.ins().iconst(types::I64, super::runtime::encode_f64(*f)));
                }
                Instruction::LoadBool(b) => {
                    let v = if *b { super::runtime::VAL_TRUE } else { super::runtime::VAL_FALSE };
                    stack.push(builder.ins().iconst(types::I64, v));
                }
                Instruction::LoadStr(s) => {
                    let ptr = self.cstr_ptr(&mut builder, s);
                    let call = builder.ins().call(make_str_ref, &[ptr]);
                    stack.push(builder.inst_results(call)[0]);
                }

                //    Variables                                                 
                Instruction::LoadVar(name) => {
                    if !is_main && !locales.contains(name) {
                        let name_ptr = self.cstr_ptr(&mut builder, name);
                        let call = builder.ins().call(load_global_ref, &[name_ptr]);
                        stack.push(builder.inst_results(call)[0]);
                    } else {
                        let &var = var_table.get(name)
                            .ok_or_else(|| format!("JIT: variable '{name}' no declarada"))?;
                        stack.push(builder.use_var(var));
                    }
                }
                Instruction::StoreVar(name) | Instruction::StoreConst(name) => {
                    let val = stack.pop().ok_or("StoreVar: pila vacía")?;
                    if let Some(&var) = var_table.get(name) {
                        builder.def_var(var, val);
                    }

                    if is_main && self.publicadas.contains(name) {
                        let name_ptr = self.cstr_ptr(&mut builder, name);
                        builder.ins().call(store_global_ref, &[name_ptr, val]);
                    }
                    // JIT-5: si es campo de un act, sincronizar al self activo
                    if let Some(fields) = field_names {
                        if fields.contains(name) {
                            let name_ptr = self.cstr_ptr(&mut builder, name);
                            builder.ins().call(set_self_field_ref, &[name_ptr, val]);
                        }
                    }
                }

                //    Aritmética                                                
                Instruction::Add => { fast_binop!(add_ref, inline::int_add, inline::flt_add); }
                Instruction::Sub => { fast_binop!(sub_ref, inline::int_sub, inline::flt_sub); }
                Instruction::Mul => { fast_binop!(mul_ref, inline::int_mul, inline::flt_mul); }
                Instruction::Div => { fast_binop!(div_ref, inline::int_div, inline::flt_div); }
                Instruction::Mod => { fast_binop!(mod_ref, inline::int_mod, inline::flt_none); }
                Instruction::Pow => { binop_check!(pow_ref); }
                Instruction::Neg => {
                    unop!(neg_ref);
                    let r = *stack.last().unwrap();
                    check!(r);
                }

                //    Comparación                                               
                Instruction::Eq    => { fast_binop!(eq_ref,   inline::int_eq, inline::flt_eq); }
                Instruction::NotEq => { fast_binop!(neq_ref,  inline::int_ne, inline::flt_ne); }
                Instruction::Lt    => { fast_binop!(lt_ref,   inline::int_lt, inline::flt_lt); }
                Instruction::LtEq  => { fast_binop!(lteq_ref, inline::int_le, inline::flt_le); }
                Instruction::Gt    => { fast_binop!(gt_ref,   inline::int_gt, inline::flt_gt); }
                Instruction::GtEq  => { fast_binop!(gteq_ref, inline::int_ge, inline::flt_ge); }

                //    Lógica                                                    
                Instruction::And => { binop!(and_ref); }
                Instruction::Or  => { binop!(or_ref);  }
                Instruction::Not => { unop!(not_ref);  }

                //    Control de flujo                                          
                Instruction::Jump(target) => {
                    let tb = *block_map.get(target)
                        .ok_or_else(|| format!("Jump: bloque {target} no encontrado"))?;
                    salir!(*target);
                    builder.ins().jump(tb, &[]);
                    terminated = true;
                }
                Instruction::JumpIfFalse(target) => {
                    let val = stack.pop().ok_or("JumpIfFalse: pila vacía")?;
                    let false_block = *block_map.get(target)
                        .ok_or_else(|| format!("JumpIfFalse: {target} no encontrado"))?;
                    let true_block  = *block_map.get(&(i + 1))
                        .ok_or_else(|| format!("JumpIfFalse: {} no encontrado", i + 1))?;
                    salir!(*target);
                    salir!(i + 1);
                    branch_on!(val, true_block, false_block);
                    terminated = true;
                }
                // `and` / `or`: si el valor ya decide, llega convertido a
                // booleano al punto de llegada; si no, se sigue con la derecha.
                Instruction::JumpIfFalseOrPop(target) | Instruction::JumpIfTrueOrPop(target) => {
                    let corta_si_falso = matches!(instr, Instruction::JumpIfFalseOrPop(_));
                    let val = stack.pop().ok_or("JumpIfOrPop: pila vacía")?;
                    let cond_call = builder.ins().call(is_truthy_ref, &[val]);
                    let cond = builder.inst_results(cond_call)[0];
                    let n1 = builder.ins().call(not_ref, &[val]);
                    let n1 = builder.inst_results(n1)[0];
                    let como_bool = builder.ins().call(not_ref, &[n1]);
                    let como_bool = builder.inst_results(como_bool)[0];
                    // La derecha sigue con la pila de antes; la llegada recibe
                    // además el valor ya convertido a booleano.
                    salir!(i + 1);
                    stack.push(como_bool);
                    salir!(*target);
                    stack.pop();
                    let llegada = *block_map.get(target)
                        .ok_or_else(|| format!("JumpIfOrPop: {target} no encontrado"))?;
                    let derecha = *block_map.get(&(i + 1))
                        .ok_or_else(|| format!("JumpIfOrPop: {} no encontrado", i + 1))?;
                    if corta_si_falso {
                        builder.ins().brif(cond, derecha, &[], llegada, &[]);
                    } else {
                        builder.ins().brif(cond, llegada, &[], derecha, &[]);
                    }
                    terminated = true;
                }
                Instruction::ToBool => {
                    unop!(not_ref);
                    unop!(not_ref);
                }
                Instruction::JumpIfTrue(target) => {
                    let val = stack.pop().ok_or("JumpIfTrue: pila vacía")?;
                    let true_block  = *block_map.get(target)
                        .ok_or_else(|| format!("JumpIfTrue: {target} no encontrado"))?;
                    let false_block = *block_map.get(&(i + 1))
                        .ok_or_else(|| format!("JumpIfTrue: {} no encontrado", i + 1))?;
                    salir!(*target);
                    salir!(i + 1);
                    branch_on!(val, true_block, false_block);
                    terminated = true;
                }

                //    Funciones                                                 
                Instruction::MakeFunction(_, _, _) => { /* no-op: ya compilado */ }
                Instruction::Call(fname, n_args) => {
                    let n = *n_args as usize;
                    if shape_names.contains(fname) {
                        // JIT-5: instanciación de shape
                        let mut args: Vec<cranelift_codegen::ir::Value> = (0..n)
                            .map(|_| stack.pop().ok_or("Call shape: pila vacía"))
                            .collect::<Result<_, _>>()?;
                        args.reverse();
                        // push args para el on_create
                        for &arg in &args {
                            builder.ins().call(push_arg_ref, &[arg]);
                        }
                        let name_ptr = self.cstr_ptr(&mut builder, fname);
                        let n_args_v  = builder.ins().iconst(types::I64, n as i64);
                        let call = builder.ins().call(create_instance_ref, &[name_ptr, n_args_v]);
                        stack.push(builder.inst_results(call)[0]);
                        check!(*stack.last().unwrap());
                    } else if let Some(&fref) = user_fn_refs.get(fname) {
                        let mut args: Vec<cranelift_codegen::ir::Value> = (0..n)
                            .map(|_| stack.pop().ok_or("Call: pila vacía"))
                            .collect::<Result<_, _>>()?;
                        args.reverse();
                        let call = builder.ins().call(fref, &args);
                        stack.push(builder.inst_results(call)[0]);
                        check!(*stack.last().unwrap());
                    } else {
                        // Builtin (str, len, push, range, ...): se despacha vía la VM.
                        // Args al ARG_BUF en orden (elem_0 primero), luego rt_call_builtin.
                        let mut args: Vec<cranelift_codegen::ir::Value> = (0..n)
                            .map(|_| stack.pop().ok_or("Call builtin: pila vacía"))
                            .collect::<Result<_, _>>()?;
                        args.reverse();
                        for &arg in &args {
                            builder.ins().call(push_arg_ref, &[arg]);
                        }
                        let name_ptr = self.cstr_ptr(&mut builder, fname);
                        let n_args_v = builder.ins().iconst(types::I64, n as i64);
                        let call = builder.ins().call(call_builtin_ref, &[name_ptr, n_args_v]);
                        stack.push(builder.inst_results(call)[0]);
                        check!(*stack.last().unwrap());
                    }
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

                //    I/O                                                       
                Instruction::Show => {
                    let val = stack.pop().ok_or("Show: pila vacía")?;
                    builder.ins().call(show_ref, &[val]);
                }

                //    Stack                                                     
                Instruction::Pop => { stack.pop(); }
                Instruction::Dup => {
                    let top = stack.last().cloned().ok_or("Dup: pila vacía")?;
                    stack.push(top);
                }

                //    Terminadores                                              
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

                //    Manejo de errores — JIT-3                                 
                Instruction::BeginAttempt(handler_addr) => {
                    let handler_block = *block_map.get(handler_addr)
                        .ok_or_else(|| format!("BeginAttempt: handler {handler_addr} no encontrado"))?;
                    handler_stack.push(handler_block);
                    // No emite IR: la caída natural lleva al cuerpo del attempt
                }
                Instruction::EndAttempt(end_addr) => {
                    handler_stack.pop();
                    let end_block = *block_map.get(end_addr)
                        .ok_or_else(|| format!("EndAttempt: bloque {end_addr} no encontrado"))?;
                    salir!(*end_addr);
                    builder.ins().jump(end_block, &[]);
                    terminated = true;
                }
                Instruction::Raise => {
                    let msg = stack.pop().ok_or("Raise: pila vacía")?;
                    if let Some(&handler_block) = handler_stack.last() {
                        builder.ins().call(set_error_ref, &[msg]);
                        builder.ins().jump(handler_block, &[]);
                    } else if is_main {
                        builder.ins().call(raise_exit_ref, &[msg]);
                        builder.ins().return_(&[]);
                    } else {
                        builder.ins().call(set_error_ref, &[msg]);
                        let cero = builder.ins().iconst(types::I64, 0);
                        builder.ins().return_(&[cero]);
                    }
                    terminated = true;
                }

                //    Colecciones — JIT-2                                       
                Instruction::MakeList(n_count) => {
                    let n = *n_count as usize;
                    // Pop N elementos del stack en orden inverso, luego revertir.
                    let mut items: Vec<cranelift_codegen::ir::Value> = (0..n)
                        .map(|_| stack.pop().ok_or("MakeList: pila vacía"))
                        .collect::<Result<_, _>>()?;
                    items.reverse(); // items[0] = primer elemento de la lista
                    for item in &items {
                        builder.ins().call(push_arg_ref, &[*item]);
                    }
                    let nv = builder.ins().iconst(types::I64, n as i64);
                    let call = builder.ins().call(make_list_n_ref, &[nv]);
                    stack.push(builder.inst_results(call)[0]);
                }
                Instruction::MakeDict(n_count) => {
                    let n = *n_count as usize;
                    let mut pares = Vec::with_capacity(n);
                    for _ in 0..n {
                        let val = stack.pop().ok_or("MakeDict: pila vacía (val)")?;
                        let key = stack.pop().ok_or("MakeDict: pila vacía (key)")?;
                        pares.push((val, key));
                    }
                    for (val, key) in pares.into_iter().rev() {
                        builder.ins().call(push_arg_ref, &[val]);
                        builder.ins().call(push_arg_ref, &[key]);
                    }
                    let nv = builder.ins().iconst(types::I64, n as i64);
                    let call = builder.ins().call(make_dict_n_ref, &[nv]);
                    stack.push(builder.inst_results(call)[0]);
                }
                Instruction::GetIndex => {
                    let idx = stack.pop().ok_or("GetIndex: pila vacía")?;
                    let obj = stack.pop().ok_or("GetIndex: pila vacía")?;
                    let call = builder.ins().call(get_index_ref, &[obj, idx]);
                    stack.push(builder.inst_results(call)[0]);
                    check!(*stack.last().unwrap());
                }
                Instruction::SetIndex => {
                    let val = stack.pop().ok_or("SetIndex: pila vacía")?;
                    let idx = stack.pop().ok_or("SetIndex: pila vacía")?;
                    let obj = stack.pop().ok_or("SetIndex: pila vacía")?;
                    let call = builder.ins().call(set_index_ref, &[obj, idx, val]);
                    stack.push(builder.inst_results(call)[0]);
                    check!(*stack.last().unwrap());
                }

                //    OOP — JIT-5                                              
                Instruction::DefineShape(_) => { /* no-op: shapes ya registradas en run_program */ }

                Instruction::GetAttr(attr) => {
                    let obj = stack.pop().ok_or("GetAttr: pila vacía")?;
                    let name_ptr = self.cstr_ptr(&mut builder, attr);
                    let call = builder.ins().call(get_attr_ref, &[obj, name_ptr]);
                    stack.push(builder.inst_results(call)[0]);
                    check!(*stack.last().unwrap());
                }
                Instruction::SetAttr(attr) => {
                    let val = stack.pop().ok_or("SetAttr: pila vacía (val)")?;
                    let obj = stack.pop().ok_or("SetAttr: pila vacía (obj)")?;
                    let name_ptr = self.cstr_ptr(&mut builder, attr);
                    builder.ins().call(set_attr_ref, &[obj, name_ptr, val]);
                    check_pending!();
                }
                Instruction::IsInstance(shape_name) => {
                    let obj = stack.pop().ok_or("IsInstance: pila vacía")?;
                    let name_ptr = self.cstr_ptr(&mut builder, shape_name);
                    let call = builder.ins().call(is_instance_ref, &[obj, name_ptr]);
                    stack.push(builder.inst_results(call)[0]);
                }
                Instruction::PushSelf => {
                    let call = builder.ins().call(get_self_ref, &[]);
                    stack.push(builder.inst_results(call)[0]);
                }
                Instruction::CallMethod(method_name, n_args) => {
                    let n = *n_args as usize;
                    // Pop args en orden y push a ARG_BUF
                    let mut args: Vec<cranelift_codegen::ir::Value> = (0..n)
                        .map(|_| stack.pop().ok_or("CallMethod: pila vacía"))
                        .collect::<Result<_, _>>()?;
                    args.reverse();
                    for &arg in &args {
                        builder.ins().call(push_arg_ref, &[arg]);
                    }
                    let obj = stack.pop().ok_or("CallMethod: pila vacía (obj)")?;
                    let name_ptr = self.cstr_ptr(&mut builder, method_name);
                    let n_val    = builder.ins().iconst(types::I64, n as i64);
                    let call = builder.ins().call(call_method_ref, &[obj, name_ptr, n_val]);
                    stack.push(builder.inst_results(call)[0]);
                    check!(*stack.last().unwrap());
                }

                //    I/O nativo — JIT-4                                       
                Instruction::ReadInput { cast, choices } => {
                    let cast_ptr = if let Some(c) = cast {
                        self.cstr_ptr(&mut builder, c)
                    } else {
                        builder.ins().iconst(types::I64, 0i64)
                    };
                    if *choices {
                        let prompt      = stack.pop().ok_or("ReadInput: pila vacía (prompt)")?;
                        let choices_val = stack.pop().ok_or("ReadInput: pila vacía (choices)")?;
                        let call = builder.ins().call(read_input_choices_ref, &[prompt, choices_val, cast_ptr]);
                        stack.push(builder.inst_results(call)[0]);
                    } else {
                        let prompt = stack.pop().ok_or("ReadInput: pila vacía (prompt)")?;
                        let call = builder.ins().call(read_input_ref, &[prompt, cast_ptr]);
                        stack.push(builder.inst_results(call)[0]);
                    }
                }
                Instruction::ReadFile(fmt) => {
                    let fmt_ptr = self.cstr_ptr(&mut builder, fmt);
                    let path = stack.pop().ok_or("ReadFile: pila vacía")?;
                    let call = builder.ins().call(read_file_ref, &[path, fmt_ptr]);
                    stack.push(builder.inst_results(call)[0]);
                    check!(*stack.last().unwrap());
                }
                Instruction::WriteFile(mode) => {
                    let mode_ptr = self.cstr_ptr(&mut builder, mode);
                    let data = stack.pop().ok_or("WriteFile: pila vacía (data)")?;
                    let path = stack.pop().ok_or("WriteFile: pila vacía (path)")?;
                    builder.ins().call(write_file_ref, &[path, data, mode_ptr]);
                    check_pending!();
                }
                Instruction::ReadEnv(cast) => {
                    let cast_ptr = self.cstr_ptr(&mut builder, cast);
                    let key = stack.pop().ok_or("ReadEnv: pila vacía")?;
                    let call = builder.ins().call(read_env_ref, &[key, cast_ptr]);
                    stack.push(builder.inst_results(call)[0]);
                }
                Instruction::UseModule(path, alias, _selective) => {
                    let path_ptr = self.cstr_ptr(&mut builder, path);
                    let call = builder.ins().call(use_module_ref, &[path_ptr]);
                    let module_val = builder.inst_results(call)[0];
                    check!(module_val);
                    if is_main {
                        let alias_ptr = self.cstr_ptr(&mut builder, alias);
                        builder.ins().call(store_global_ref, &[alias_ptr, module_val]);
                    }
                    if let Some(&var) = var_table.get(alias) {
                        builder.def_var(var, module_val);
                    }
                }

                //    JIT-6: Closures                                              
                Instruction::MakeClosure(fn_name) => {
                    let name_ptr = self.cstr_ptr(&mut builder, fn_name);
                    let call = builder.ins().call(make_closure_ref, &[name_ptr]);
                    stack.push(builder.inst_results(call)[0]);
                }

                //    JIT-6: Async                                                  
                Instruction::CallAsync(fname, n_args) => {
                    let n = *n_args as usize;
                    // Pop args del stack en orden, revertir, pushear al ARG_BUF
                    let mut args: Vec<cranelift_codegen::ir::Value> = (0..n)
                        .map(|_| stack.pop().ok_or("CallAsync: pila vacía"))
                        .collect::<Result<_, _>>()?;
                    args.reverse();
                    for &arg in &args {
                        builder.ins().call(push_arg_ref, &[arg]);
                    }
                    let name_ptr = self.cstr_ptr(&mut builder, fname);
                    let n_val    = builder.ins().iconst(types::I64, n as i64);
                    // `spawn f()` es CallAsync + Pop: la tarea queda suelta.
                    let suelta = matches!(instructions.get(i + 1), Some(Instruction::Pop));
                    let lanzar = if suelta { spawn_ref } else { call_async_ref };
                    let call = builder.ins().call(lanzar, &[name_ptr, n_val]);
                    stack.push(builder.inst_results(call)[0]);
                    check!(*stack.last().unwrap());
                }
                Instruction::Await => {
                    let val = stack.pop().ok_or("Await: pila vacía")?;
                    let call = builder.ins().call(await_ref, &[val]);
                    stack.push(builder.inst_results(call)[0]);
                    check!(*stack.last().unwrap());
                }

                other => {
                    return Err(format!("JIT: instruction not supported at this stage: {other:?}"));
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

        if let Some(b) = propagar {
            builder.switch_to_block(b);
            if is_main {
                let c = builder.ins().call(take_error_ref, &[]);
                let e = builder.inst_results(c)[0];
                builder.ins().call(raise_exit_ref, &[e]);
                builder.ins().return_(&[]);
            } else {
                let cero = builder.ins().iconst(types::I64, 0);
                builder.ins().return_(&[cero]);
            }
        }

        builder.seal_all_blocks();
        builder.finalize();
        Ok(())
    }
}

//     Rutas rápidas en línea

/// IR sobre la codificación de `runtime::alloc_val`. Cada operación devuelve
/// (resultado codificado, vale): si `vale` es 0, se llama al runtime.
mod inline {
    use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
    use cranelift_codegen::ir::{types, InstBuilder, MemFlags, Value};
    use cranelift_frontend::FunctionBuilder;

    use crate::jit::runtime::{DOUBLE_OFFSET, INT_TAG, VAL_FALSE, VAL_TRUE};

    pub type Op = fn(&mut FunctionBuilder, Value, Value) -> (Value, Value);

    pub fn both_int(b: &mut FunctionBuilder, x: Value, y: Value) -> Value {
        let both = b.ins().band(x, y);
        let top = b.ins().ushr_imm(both, 48);
        b.ins().icmp_imm(IntCC::Equal, top, (INT_TAG as u64 >> 48) as i64)
    }

    fn is_double(b: &mut FunctionBuilder, v: Value) -> Value {
        let lo = b.ins().icmp_imm(IntCC::UnsignedGreaterThanOrEqual, v, DOUBLE_OFFSET);
        let hi = b.ins().icmp_imm(IntCC::UnsignedLessThan, v, INT_TAG);
        b.ins().band(lo, hi)
    }

    pub fn both_double(b: &mut FunctionBuilder, x: Value, y: Value) -> Value {
        let dx = is_double(b, x);
        let dy = is_double(b, y);
        b.ins().band(dx, dy)
    }

    fn int_of(b: &mut FunctionBuilder, v: Value) -> Value {
        let s = b.ins().ishl_imm(v, 16);
        b.ins().sshr_imm(s, 16)
    }

    fn fits48(b: &mut FunctionBuilder, r: Value) -> Value {
        let back = int_of(b, r);
        b.ins().icmp(IntCC::Equal, back, r)
    }

    fn enc_int(b: &mut FunctionBuilder, r: Value) -> Value {
        let low = b.ins().band_imm(r, 0xFFFF_FFFF_FFFF);
        b.ins().bor_imm(low, INT_TAG)
    }

    fn f64_of(b: &mut FunctionBuilder, v: Value) -> Value {
        let bits = b.ins().iadd_imm(v, -DOUBLE_OFFSET);
        b.ins().bitcast(types::F64, MemFlags::new(), bits)
    }

    fn enc_f64(b: &mut FunctionBuilder, f: Value) -> Value {
        let bits = b.ins().bitcast(types::I64, MemFlags::new(), f);
        let nan = b.ins().fcmp(FloatCC::Unordered, f, f);
        let canon = b.ins().iconst(types::I64, 0x7FF8_0000_0000_0000);
        let bits = b.ins().select(nan, canon, bits);
        b.ins().iadd_imm(bits, DOUBLE_OFFSET)
    }

    fn enc_bool(b: &mut FunctionBuilder, c: Value) -> Value {
        let t = b.ins().iconst(types::I64, VAL_TRUE);
        let f = b.ins().iconst(types::I64, VAL_FALSE);
        b.ins().select(c, t, f)
    }

    fn yes(b: &mut FunctionBuilder) -> Value {
        b.ins().iconst(types::I8, 1)
    }

    fn no(b: &mut FunctionBuilder) -> Value {
        b.ins().iconst(types::I8, 0)
    }

    fn ints(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) {
        (int_of(b, x), int_of(b, y))
    }

    fn flts(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) {
        (f64_of(b, x), f64_of(b, y))
    }

    // Enteros: los operandos tienen 48 bits, así que suma y resta no
    // desbordan el i64; basta ver si el resultado cabe en 48.
    pub fn int_add(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) {
        let (a, c) = ints(b, x, y);
        let r = b.ins().iadd(a, c);
        (enc_int(b, r), fits48(b, r))
    }

    pub fn int_sub(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) {
        let (a, c) = ints(b, x, y);
        let r = b.ins().isub(a, c);
        (enc_int(b, r), fits48(b, r))
    }

    pub fn int_mul(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) {
        let (a, c) = ints(b, x, y);
        let lo = b.ins().imul(a, c);
        let hi = b.ins().smulhi(a, c);
        let sign = b.ins().sshr_imm(lo, 63);
        let no_ovf = b.ins().icmp(IntCC::Equal, hi, sign);
        let fits = fits48(b, lo);
        (enc_int(b, lo), b.ins().band(no_ovf, fits))
    }

    /// int / int es decimal, como en la VM; un divisor 0 va al runtime (error).
    pub fn int_div(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) {
        let (a, c) = ints(b, x, y);
        let ok = b.ins().icmp_imm(IntCC::NotEqual, c, 0);
        let fa = b.ins().fcvt_from_sint(types::F64, a);
        let fc = b.ins().fcvt_from_sint(types::F64, c);
        let q = b.ins().fdiv(fa, fc);
        (enc_f64(b, q), ok)
    }

    /// `srem` con divisor 0 detiene el proceso: se divide por 1 y se descarta.
    pub fn int_mod(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) {
        let (a, c) = ints(b, x, y);
        let ok = b.ins().icmp_imm(IntCC::NotEqual, c, 0);
        let one = b.ins().iconst(types::I64, 1);
        let d = b.ins().select(ok, c, one);
        let r = b.ins().srem(a, d);
        (enc_int(b, r), ok)
    }

    fn int_cmp(b: &mut FunctionBuilder, x: Value, y: Value, cc: IntCC) -> (Value, Value) {
        let (a, c) = ints(b, x, y);
        let r = b.ins().icmp(cc, a, c);
        (enc_bool(b, r), yes(b))
    }

    pub fn int_lt(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) { int_cmp(b, x, y, IntCC::SignedLessThan) }
    pub fn int_le(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) { int_cmp(b, x, y, IntCC::SignedLessThanOrEqual) }
    pub fn int_gt(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) { int_cmp(b, x, y, IntCC::SignedGreaterThan) }
    pub fn int_ge(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) { int_cmp(b, x, y, IntCC::SignedGreaterThanOrEqual) }

    /// Dos enteros en línea son iguales si su codificación lo es.
    pub fn int_eq(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) {
        let r = b.ins().icmp(IntCC::Equal, x, y);
        (enc_bool(b, r), yes(b))
    }

    pub fn int_ne(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) {
        let r = b.ins().icmp(IntCC::NotEqual, x, y);
        (enc_bool(b, r), yes(b))
    }

    // Decimales.
    pub fn flt_add(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) {
        let (a, c) = flts(b, x, y);
        let r = b.ins().fadd(a, c);
        (enc_f64(b, r), yes(b))
    }

    pub fn flt_sub(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) {
        let (a, c) = flts(b, x, y);
        let r = b.ins().fsub(a, c);
        (enc_f64(b, r), yes(b))
    }

    pub fn flt_mul(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) {
        let (a, c) = flts(b, x, y);
        let r = b.ins().fmul(a, c);
        (enc_f64(b, r), yes(b))
    }

    /// Un divisor 0.0 va al runtime (error "División por cero", como en la VM).
    pub fn flt_div(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) {
        let (a, c) = flts(b, x, y);
        let zero = b.ins().f64const(0.0);
        let ok = b.ins().fcmp(FloatCC::NotEqual, c, zero);
        let r = b.ins().fdiv(a, c);
        (enc_f64(b, r), ok)
    }

    /// `%` solo admite enteros: con decimales decide el runtime.
    pub fn flt_none(b: &mut FunctionBuilder, x: Value, _y: Value) -> (Value, Value) {
        (x, no(b))
    }

    fn flt_cmp(b: &mut FunctionBuilder, x: Value, y: Value, cc: FloatCC) -> (Value, Value) {
        let (a, c) = flts(b, x, y);
        let r = b.ins().fcmp(cc, a, c);
        (enc_bool(b, r), yes(b))
    }

    // Igual que la VM: `>` es "ni < ni ==" y `>=` es "no <", así que con NaN
    // dan verdadero.
    pub fn flt_lt(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) { flt_cmp(b, x, y, FloatCC::LessThan) }
    pub fn flt_le(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) { flt_cmp(b, x, y, FloatCC::LessThanOrEqual) }
    pub fn flt_gt(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) { flt_cmp(b, x, y, FloatCC::UnorderedOrGreaterThan) }
    pub fn flt_ge(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) { flt_cmp(b, x, y, FloatCC::UnorderedOrGreaterThanOrEqual) }
    pub fn flt_eq(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) { flt_cmp(b, x, y, FloatCC::Equal) }
    pub fn flt_ne(b: &mut FunctionBuilder, x: Value, y: Value) -> (Value, Value) { flt_cmp(b, x, y, FloatCC::NotEqual) }
}

//! Backend AOT: compila el programa a código nativo dentro de un archivo objeto.
//!
//! Reutiliza el mismo generador de IR que el JIT ([`super::compiler::CodeGen`])
//! cambiando el backend de `JITModule` a `ObjectModule`. La diferencia con el
//! modo bundle (ver `crate::aot`) es sustancial: allí el objeto solo lleva el
//! bytecode y un `main` que arranca el intérprete, aquí el objeto lleva el
//! código máquina de las funciones del programa.
//!
//! Dos cosas que en JIT resuelve el compilador en tiempo de compilación tienen
//! que ocurrir dentro del binario, y por eso se emite un prólogo en `main`:
//!
//!   1. El registro de shapes (campos y padres), que vive en TLS.
//!   2. El registro de punteros de funciones y acts, que en JIT vienen de
//!      `get_finalized_function` y aquí los resuelve el linker.

use cranelift_codegen::ir::{types, AbiParam, InstBuilder};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{default_libcall_names, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use indexmap::IndexMap as HashMap;

use crate::bytecode::OrionBytecode;
use super::compiler::CodeGen;
use super::runtime_oop::join_names;

pub fn compile_to_native_object(bc: &OrionBytecode) -> Result<Option<Vec<u8>>, String> {
    let mut flag_builder = settings::builder();
    flag_builder.set("use_colocated_libcalls", "false").unwrap();
    flag_builder.set("is_pic", "false").unwrap();
    flag_builder.set("opt_level", "speed").unwrap();

    let isa = cranelift_native::builder()
        .map_err(|e| format!("ISA nativa no disponible: {e}"))?
        .finish(settings::Flags::new(flag_builder))
        .map_err(|e| format!("Error construyendo ISA: {e}"))?;

    let obj_builder = ObjectBuilder::new(isa, "orion_program", default_libcall_names())
        .map_err(|e| format!("Error creando ObjectBuilder: {e}"))?;

    let mut cg = CodeGen::new_aot(ObjectModule::new(obj_builder));

    // Los cuerpos del programa: funciones de usuario, acts y el main de Orion.
    let prog = match cg.compile_program(bc)? {
        Some(p) => p,
        None => return Ok(None),
    };

    // El `main` del ejecutable: prólogo de registro + salto al main de Orion.
    let module = cg.module_mut();
    let mut main_sig = module.make_signature();
    main_sig.returns.push(AbiParam::new(types::I32));
    let main_id = module
        .declare_function("main", Linkage::Export, &main_sig)
        .map_err(|e| format!("Error declaring main: {e}"))?;

    let mut reg_shape_sig = module.make_signature();
    for _ in 0..3 { reg_shape_sig.params.push(AbiParam::new(types::I64)); }
    let reg_shape_id = module
        .declare_function("rt_register_shape", Linkage::Import, &reg_shape_sig)
        .map_err(|e| format!("Error declaring rt_register_shape: {e}"))?;

    let mut reg_fn_sig = module.make_signature();
    for _ in 0..2 { reg_fn_sig.params.push(AbiParam::new(types::I64)); }
    let reg_fn_id = module
        .declare_function("rt_register_fn", Linkage::Import, &reg_fn_sig)
        .map_err(|e| format!("Error declaring rt_register_fn: {e}"))?;

    let mut reg_method_sig = module.make_signature();
    for _ in 0..3 { reg_method_sig.params.push(AbiParam::new(types::I64)); }
    let reg_method_id = module
        .declare_function("rt_register_method", Linkage::Import, &reg_method_sig)
        .map_err(|e| format!("Error declaring rt_register_method: {e}"))?;

    let mut lits: HashMap<String, String> = HashMap::new();
    for (name, fields, parents) in &prog.shapes {
        lits.insert(format!("shape:{name}"), name.clone());
        lits.insert(format!("fields:{name}"), join_names(fields));
        lits.insert(format!("parents:{name}"), join_names(parents));
    }
    for (name, _) in &prog.functions {
        lits.insert(format!("fn:{name}"), name.clone());
    }
    for (shape, act, _) in &prog.methods {
        lits.insert(format!("mshape:{shape}"), shape.clone());
        lits.insert(format!("mact:{act}"), act.clone());
    }
    let lit_ids = cg.emit_literals(&lits)?;

    let mut ctx = cg.module_mut().make_context();
    ctx.func.signature = main_sig;
    {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);

        let module = cg.module_mut();
        let reg_shape_ref  = module.declare_func_in_func(reg_shape_id,  builder.func);
        let reg_fn_ref     = module.declare_func_in_func(reg_fn_id,     builder.func);
        let reg_method_ref = module.declare_func_in_func(reg_method_id, builder.func);

        // Dirección de un literal ya emitido.
        macro_rules! lit {
            ($builder:expr, $key:expr) => {{
                let id = lit_ids[&$key];
                let gv = cg.module_mut().declare_data_in_func(id, $builder.func);
                $builder.ins().symbol_value(types::I64, gv)
            }};
        }

        for (name, _, _) in &prog.shapes {
            let n = lit!(builder, format!("shape:{name}"));
            let f = lit!(builder, format!("fields:{name}"));
            let p = lit!(builder, format!("parents:{name}"));
            builder.ins().call(reg_shape_ref, &[n, f, p]);
        }

        for (name, fid) in &prog.functions {
            let n = lit!(builder, format!("fn:{name}"));
            let fref = cg.module_mut().declare_func_in_func(*fid, builder.func);
            let addr = builder.ins().func_addr(types::I64, fref);
            builder.ins().call(reg_fn_ref, &[n, addr]);
        }

        for (shape, act, fid) in &prog.methods {
            let s = lit!(builder, format!("mshape:{shape}"));
            let a = lit!(builder, format!("mact:{act}"));
            let fref = cg.module_mut().declare_func_in_func(*fid, builder.func);
            let addr = builder.ins().func_addr(types::I64, fref);
            builder.ins().call(reg_method_ref, &[s, a, addr]);
        }

        // Cuerpo del programa. Un programa vacío no tiene main que llamar.
        if let Some(orion_main) = prog.main {
            let mref = cg.module_mut().declare_func_in_func(orion_main, builder.func);
            builder.ins().call(mref, &[]);
        }

        let zero = builder.ins().iconst(types::I32, 0);
        builder.ins().return_(&[zero]);
        builder.finalize();
    }

    cg.module_mut()
        .define_function(main_id, &mut ctx)
        .map_err(|e| format!("Error compiling main(): {e}"))?;

    let product = cg.into_module().finish();
    product
        .object
        .write()
        .map(Some)
        .map_err(|e| format!("Error serializando objeto: {e}"))
}

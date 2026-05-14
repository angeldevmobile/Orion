//! Compilador AOT (Ahead-of-Time) de Orion usando cranelift-object.
//!
//! Produce un archivo objeto nativo (.o / .obj) que contiene:
//!   - El bytecode serializado como sección RODATA (`_orion_bc`)
//!   - La longitud del bytecode como dato de 8 bytes (`_orion_bc_len`)
//!   - Una función `main()` en Cranelift IR que llama `orion_rt_exec(ptr, len)`
//!
//! El archivo objeto se enlaza con la staticlib de Orion VM para producir
//! un ejecutable nativo standalone.

use cranelift_codegen::ir::{types, AbiParam, InstBuilder};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{default_libcall_names, DataDescription, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

/// Compila el bytecode a un archivo objeto nativo.
///
/// Devuelve los bytes del objeto (.o / .obj) listos para escribir a disco.
pub fn compile_to_object(bytecode_bytes: &[u8]) -> Result<Vec<u8>, String> {
    //    ISA nativa                                                           
    let mut flag_builder = settings::builder();
    flag_builder.set("use_colocated_libcalls", "false").unwrap();
    flag_builder.set("is_pic", "false").unwrap();
    flag_builder.set("opt_level", "speed").unwrap();

    let isa_builder = cranelift_native::builder()
        .map_err(|e| format!("ISA nativa no disponible: {e}"))?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|e| format!("Error construyendo ISA: {e}"))?;

    //    ObjectModule                                                         
    let obj_builder = ObjectBuilder::new(
        isa,
        "orion_program",
        default_libcall_names(),
    ).map_err(|e| format!("Error creando ObjectBuilder: {e}"))?;

    let mut module = ObjectModule::new(obj_builder);

    //    Sección RODATA: bytecode                                          
    //    _orion_bc  — los bytes del bytecode serializado (JSON)
    let bc_id = module
        .declare_data("_orion_bc", Linkage::Export, false, false)
        .map_err(|e| format!("Error declarando _orion_bc: {e}"))?;

    let mut bc_desc = DataDescription::new();
    bc_desc.define(bytecode_bytes.to_vec().into_boxed_slice());
    module
        .define_data(bc_id, &bc_desc)
        .map_err(|e| format!("Error definiendo _orion_bc: {e}"))?;

    //    Sección DATA: longitud del bytecode                               
    //    _orion_bc_len  — u64 con el número de bytes
    let len_id = module
        .declare_data("_orion_bc_len", Linkage::Export, false, false)
        .map_err(|e| format!("Error declarando _orion_bc_len: {e}"))?;

    let mut len_desc = DataDescription::new();
    let len_bytes = (bytecode_bytes.len() as u64).to_le_bytes();
    len_desc.define(Box::new(len_bytes));
    module
        .define_data(len_id, &len_desc)
        .map_err(|e| format!("Error definiendo _orion_bc_len: {e}"))?;

    //    Declarar orion_rt_exec (símbolo externo, lo provee la staticlib)   
    let mut rt_sig = module.make_signature();
    rt_sig.params.push(AbiParam::new(types::I64)); // bytecode_ptr
    rt_sig.params.push(AbiParam::new(types::I64)); // bytecode_len
    rt_sig.returns.push(AbiParam::new(types::I32)); // exit code

    let rt_exec_id = module
        .declare_function("orion_rt_exec", Linkage::Import, &rt_sig)
        .map_err(|e| format!("Error declarando orion_rt_exec: {e}"))?;

    //    Función main()                                                     
    //    Signature: fn() -> i32  (C ABI)
    let mut main_sig = module.make_signature();
    main_sig.returns.push(AbiParam::new(types::I32));

    let main_id = module
        .declare_function("main", Linkage::Export, &main_sig)
        .map_err(|e| format!("Error declarando main: {e}"))?;

    // Obtener referencias globales para usar dentro de la función
    let bc_gv   = module.declare_data_in_func(bc_id,  &mut cranelift_codegen::ir::Function::new());
    let len_gv  = module.declare_data_in_func(len_id, &mut cranelift_codegen::ir::Function::new());
    let _ = (bc_gv, len_gv); // se usarán dentro del FunctionBuilder

    let mut ctx = Context::new();
    ctx.func.signature = main_sig;

    {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);

        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);

        // Declarar los globales dentro del contexto de esta función
        let bc_gv  = module.declare_data_in_func(bc_id,  builder.func);
        let len_gv = module.declare_data_in_func(len_id, builder.func);

        // ptr = symbol_value(_orion_bc)
        let ptr = builder.ins().symbol_value(types::I64, bc_gv);

        // len_addr = symbol_value(_orion_bc_len)
        let len_addr = builder.ins().symbol_value(types::I64, len_gv);
        // len = load i64 from len_addr
        let len = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            len_addr,
            0,
        );

        // call orion_rt_exec(ptr, len)
        let rt_ref = module.declare_func_in_func(rt_exec_id, builder.func);
        let call   = builder.ins().call(rt_ref, &[ptr, len]);
        let result = builder.inst_results(call)[0];

        builder.ins().return_(&[result]);
        builder.finalize();
    }

    module
        .define_function(main_id, &mut ctx)
        .map_err(|e| format!("Error compilando main(): {e}"))?;

    //    Emitir el objeto                                                  
    let product = module.finish();
    product
        .object
        .write()
        .map_err(|e| format!("Error serializando objeto: {e}"))
}

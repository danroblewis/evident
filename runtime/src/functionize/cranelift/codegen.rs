//! Cranelift IR emission: lower a Z3Program into JIT machine code.
//! compile_program builds the function; the emit_* helpers walk Z3 ASTs
//! (write-value / write-record into the output struct, compute-i64 for the
//! scalar arithmetic/comparison core).

use std::collections::HashMap;
use cranelift::prelude::{AbiParam, FunctionBuilder, FunctionBuilderContext,
    InstBuilder, MemFlags, settings, types, StackSlotData, StackSlotKind};
use cranelift::prelude::Value as ClValue;
use cranelift::prelude::settings::Configurable;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};
use z3::ast::{Ast, Dynamic};
use z3::AstKind;
use z3_sys::DeclKind;

use crate::translate::{EnumRegistry, Value};
use crate::z3_eval::{Z3Program, Z3Step, GuardedBody};

use super::{JitProgram, OutputKind, HelperRefs, declare_helpers, import_helpers};

pub fn compile_program<'ctx>(
    program: &Z3Program<'ctx>,
    enums: &EnumRegistry,
    datatypes: &crate::core::DatatypeRegistry,
) -> Option<JitProgram> {

    let record_info: HashMap<String, Vec<crate::core::FieldKind>> = {
        let dts = datatypes.borrow();
        dts.iter().filter_map(|(_type_name, (dt, fields))| {
            let ctor = dt.variants.first()?.constructor.name();
            Some((ctor, fields.clone()))
        }).collect()
    };

    let mut enum_tags: HashMap<String, HashMap<String, i64>> = HashMap::new();
    let mut enum_variants: HashMap<String, Vec<String>> = HashMap::new();
    let mut variant_arity: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
    {
        let by_name = enums.by_name.borrow();
        for (enum_name, (_dt, variants)) in by_name.iter() {
            let mut tags = HashMap::new();
            let mut names = Vec::with_capacity(variants.len());
            let mut arities = HashMap::new();
            for (idx, v) in variants.iter().enumerate() {
                tags.insert(v.name.clone(), idx as i64);
                names.push(v.name.clone());
                arities.insert(v.name.clone(),
                    v.fields.iter().map(|f| f.type_name.clone()).collect());
            }
            enum_tags.insert(enum_name.clone(), tags);
            enum_variants.insert(enum_name.clone(), names);
            variant_arity.insert(enum_name.clone(), arities);
        }
    }

    let mut input_set: std::collections::BTreeSet<(String, OutputKind)> =
        std::collections::BTreeSet::new();
    let mut output_kinds_local: Vec<(String, OutputKind)> = Vec::new();
    for step in &program.steps {
        let (var, kind) = match step {
            Z3Step::Scalar { var, expr } => {
                let k = kind_of_dynamic(expr, &enum_variants, &variant_arity)
                    .unwrap_or(OutputKind::Int);
                (var.clone(), k)
            }
            Z3Step::Seq { var, .. } => (var.clone(), OutputKind::Seq),
            Z3Step::Guarded { .. } => {

                return None;
            }
            Z3Step::PreBaked { var, .. } => (var.clone(), OutputKind::Seq ),
        };
        output_kinds_local.push((var, kind));
        match step {
            Z3Step::Scalar { expr, .. } =>
                collect_inputs(expr, &mut input_set, &enum_variants, &variant_arity),
            Z3Step::Seq { elem_exprs, .. } =>
                for e in elem_exprs { collect_inputs(e, &mut input_set, &enum_variants, &variant_arity); },
            Z3Step::Guarded { branches, .. } => {
                for b in branches {
                    collect_inputs(&b.guard, &mut input_set, &enum_variants, &variant_arity);
                    match &b.body {
                        GuardedBody::Scalar(e) =>
                            collect_inputs(e, &mut input_set, &enum_variants, &variant_arity),
                        GuardedBody::Seq(es) =>
                            for e in es {
                                collect_inputs(e, &mut input_set, &enum_variants, &variant_arity);
                            },
                    }
                }
            }
            _ => {}
        }
    }
    let output_names: std::collections::HashSet<String> = output_kinds_local.iter()
        .map(|(n, _)| n.clone()).collect();

    let input_names: Vec<(String, OutputKind)> = input_set.into_iter()
        .filter(|(n, _)| !output_names.contains(n))
        .collect();

    let mut flag_builder = settings::builder();
    flag_builder.set("use_colocated_libcalls", "false").ok()?;
    flag_builder.set("is_pic", "false").ok()?;
    let isa_builder = cranelift_native::builder().ok()?;
    let isa = isa_builder.finish(settings::Flags::new(flag_builder)).ok()?;
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    for (name, addr) in crate::value_builders::symbol_table() {
        builder.symbol(name, addr);
    }
    let mut module = JITModule::new(builder);
    let ptr_t = module.target_config().pointer_type();

    let helper_ids = declare_helpers(&mut module, ptr_t)?;

    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(ptr_t));
    sig.params.push(AbiParam::new(ptr_t));
    sig.params.push(AbiParam::new(ptr_t));
    let func_id = module.declare_function("compiled_program",
        Linkage::Local, &sig).ok()?;
    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    let helpers = import_helpers(&mut module, helper_ids, &mut ctx.func);

    let input_offsets: HashMap<String, usize> = input_names.iter().enumerate()
        .map(|(i, (n, _))| (n.clone(), i)).collect();
    let input_kinds: HashMap<String, OutputKind> = input_names.iter().cloned().collect();
    let mut output_offsets: HashMap<String, usize> = HashMap::new();
    let mut output_kinds: HashMap<String, OutputKind> = HashMap::new();
    for (i, (name, kind)) in output_kinds_local.iter().enumerate() {
        output_offsets.insert(name.clone(), i);
        output_kinds.insert(name.clone(), kind.clone());
    }
    let size_of_value = std::mem::size_of::<Value>() as i64;

    let mut string_pool: Vec<Box<str>> = Vec::new();
    let mut value_pool: Vec<Value> = Vec::new();
    {
        let mut func_ctx = FunctionBuilderContext::new();
        let mut bcx = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
        let entry = bcx.create_block();
        bcx.append_block_params_for_function_params(entry);
        bcx.switch_to_block(entry);
        bcx.seal_block(entry);

        let inputs_ptr  = bcx.block_params(entry)[0];
        let outputs_ptr = bcx.block_params(entry)[1];
        let pool_ptr    = bcx.block_params(entry)[2];

        let mut env: HashMap<String, ClValue> = HashMap::new();
        for (name, idx) in &input_offsets {
            let off = (*idx as i64) * size_of_value;
            let off_v = bcx.ins().iconst(types::I64, off);
            let slot = bcx.ins().iadd(inputs_ptr, off_v);
            env.insert(name.clone(), slot);
        }

        for step in &program.steps {
            let out_idx = output_offsets[step.var()];
            let out_offset = (out_idx as i64) * size_of_value;
            let off_v = bcx.ins().iconst(types::I64, out_offset);
            let out_slot = bcx.ins().iadd(outputs_ptr, off_v);

            match step {
                Z3Step::Scalar { var, expr } => {
                    if emit_write_value(&mut bcx, expr, out_slot, &env,
                        &helpers, &variant_arity, &record_info, &mut string_pool,
                        ptr_t, size_of_value).is_none() {
                        return None;
                    }
                    env.insert(var.clone(), out_slot);
                }
                Z3Step::Seq { var, elem_exprs } => {

                    let cap = bcx.ins().iconst(types::I64, elem_exprs.len() as i64);
                    bcx.ins().call(helpers.seq_new, &[out_slot, cap]);

                    let temp_slot = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let temp_ptr = bcx.ins().stack_addr(ptr_t, temp_slot, 0);
                    bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                    for elem in elem_exprs.iter() {
                        if emit_write_value(&mut bcx, elem, temp_ptr, &env,
                            &helpers, &variant_arity, &record_info, &mut string_pool,
                            ptr_t, size_of_value).is_none() {
                            return None;
                        }
                        bcx.ins().call(helpers.seq_push_clone, &[out_slot, temp_ptr]);
                    }
                    env.insert(var.clone(), out_slot);
                }
                Z3Step::Guarded { var, branches } => {

                    let merge_block = bcx.create_block();
                    for branch in branches {
                        let body_block = bcx.create_block();
                        let next_block = bcx.create_block();
                        let cond_v = match emit_compute_i64(&mut bcx, &branch.guard, &env,
                            &helpers, &variant_arity, &record_info, &mut string_pool, ptr_t, size_of_value)
                        {
                            Some(v) => v,
                            None => return None,
                        };
                        bcx.ins().brif(cond_v, body_block, &[], next_block, &[]);
                        bcx.switch_to_block(body_block);
                        bcx.seal_block(body_block);
                        match &branch.body {
                            GuardedBody::Scalar(e) => {
                                if emit_write_value(&mut bcx, e, out_slot, &env,
                                    &helpers, &variant_arity, &record_info, &mut string_pool,
                                    ptr_t, size_of_value).is_none()
                                {
                                    return None;
                                }
                            }
                            GuardedBody::Seq(es) => {
                                let cap = bcx.ins().iconst(types::I64, es.len() as i64);
                                bcx.ins().call(helpers.seq_new, &[out_slot, cap]);
                                let temp_slot = bcx.create_sized_stack_slot(
                                    StackSlotData::new(StackSlotKind::ExplicitSlot,
                                                       size_of_value as u32));
                                let temp_ptr = bcx.ins().stack_addr(ptr_t, temp_slot, 0);
                                bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                                for e in es {
                                    if emit_write_value(&mut bcx, e, temp_ptr, &env,
                                        &helpers, &variant_arity, &record_info, &mut string_pool,
                                        ptr_t, size_of_value).is_none()
                                    {
                                        return None;
                                    }
                                    bcx.ins().call(helpers.seq_push_clone,
                                        &[out_slot, temp_ptr]);
                                }
                            }
                        }
                        bcx.ins().jump(merge_block, &[]);
                        bcx.switch_to_block(next_block);
                        bcx.seal_block(next_block);
                    }

                    let zero = bcx.ins().iconst(types::I64, 0);
                    bcx.ins().call(helpers.set_int, &[out_slot, zero]);
                    bcx.ins().jump(merge_block, &[]);
                    bcx.switch_to_block(merge_block);
                    bcx.seal_block(merge_block);
                    env.insert(var.clone(), out_slot);
                }
                Z3Step::PreBaked { var, value } => {
                    let idx = value_pool.len();
                    value_pool.push(value.clone());
                    let idx_v = bcx.ins().iconst(types::I64, idx as i64);
                    bcx.ins().call(helpers.clone_from_pool,
                        &[out_slot, pool_ptr, idx_v]);
                    env.insert(var.clone(), out_slot);
                }
            }
        }
        bcx.ins().return_(&[]);
        bcx.finalize();
    }

    module.define_function(func_id, &mut ctx).ok()?;
    module.clear_context(&mut ctx);
    module.finalize_definitions().ok()?;
    let code_ptr = module.get_finalized_function(func_id);

    let func: unsafe extern "C" fn(*const Value, *mut Value, *const Value) = unsafe {
        std::mem::transmute(code_ptr)
    };
    Some(JitProgram {
        _module: module,
        func,
        input_offsets,
        input_kinds,
        output_offsets,
        output_kinds,
        enum_tags,
        enum_variants,
        _string_pool: string_pool,
        value_pool,
    })
}

fn intern_str(pool: &mut Vec<Box<str>>, s: &str) -> (i64, i64) {
    let boxed: Box<str> = s.to_string().into_boxed_str();
    let ptr = boxed.as_ptr() as usize as i64;
    let len = boxed.len() as i64;
    pool.push(boxed);
    (ptr, len)
}

fn emit_write_value<'ctx>(
    bcx: &mut FunctionBuilder,
    expr: &Dynamic<'ctx>,
    out_slot: ClValue,
    env: &HashMap<String, ClValue>,
    helpers: &HelperRefs,
    variant_arity: &HashMap<String, HashMap<String, Vec<String>>>,
    record_info: &HashMap<String, Vec<crate::core::FieldKind>>,
    string_pool: &mut Vec<Box<str>>,
    ptr_t: cranelift::prelude::Type,
    size_of_value: i64,
) -> Option<()> {

    if expr.kind() == AstKind::App && expr.num_children() == 0 {
        let is_free_var = expr.safe_decl().ok()
            .map(|d| d.kind() == DeclKind::UNINTERPRETED)
            .unwrap_or(false);
        if !is_free_var {
            if let Some(zs) = expr.as_string() {
                if let Some(s) = zs.as_string() {
                    let (p, l) = intern_str(string_pool, &s);
                    let pv = bcx.ins().iconst(types::I64, p);
                    let lv = bcx.ins().iconst(types::I64, l);
                    bcx.ins().call(helpers.set_str, &[out_slot, pv, lv]);
                    return Some(());
                }
            }
        }
    }

    match expr.kind() {
        AstKind::Numeral => {
            let i = expr.as_int().and_then(|x| x.as_i64())?;
            let n = bcx.ins().iconst(types::I64, i);
            bcx.ins().call(helpers.set_int, &[out_slot, n]);
            Some(())
        }
        AstKind::App => {
            let decl = expr.safe_decl().ok()?;
            let kind = decl.kind();
            let children: Vec<Dynamic<'ctx>> = expr.children();
            match kind {
                DeclKind::TRUE => {
                    let n = bcx.ins().iconst(types::I64, 1);
                    bcx.ins().call(helpers.set_bool, &[out_slot, n]);
                    Some(())
                }
                DeclKind::FALSE => {
                    let n = bcx.ins().iconst(types::I64, 0);
                    bcx.ins().call(helpers.set_bool, &[out_slot, n]);
                    Some(())
                }
                DeclKind::UNINTERPRETED => {
                    if children.is_empty() {

                        let name = decl.name();
                        let src_slot = *env.get(&name)?;
                        let zero = bcx.ins().iconst(types::I64, 0);
                        bcx.ins().call(helpers.clone_from_pool,
                            &[out_slot, src_slot, zero]);
                        Some(())
                    } else if children.len() == 1 {

                        let name = decl.name();
                        let logical = if let Some(s) = name.strip_suffix("__arr") {
                            s.to_string()
                        } else if let Some(_s) = name.strip_suffix("__len") {
                            return None;
                        } else { return None; };
                        let temp = bcx.create_sized_stack_slot(
                            StackSlotData::new(StackSlotKind::ExplicitSlot,
                                               size_of_value as u32));
                        let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                        bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                        emit_write_value(bcx, &children[0], temp_ptr, env,
                            helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                        let (np, nl) = intern_str(string_pool, &logical);
                        let np_v = bcx.ins().iconst(types::I64, np);
                        let nl_v = bcx.ins().iconst(types::I64, nl);
                        bcx.ins().call(helpers.extract_field,
                            &[out_slot, temp_ptr, np_v, nl_v]);
                        Some(())
                    } else {
                        None
                    }
                }
                DeclKind::DT_ACCESSOR => {
                    if children.len() != 1 { return None; }
                    let raw = decl.name();

                    let accessor_name = raw.strip_suffix("__arr")
                        .or_else(|| raw.strip_suffix("__len"))
                        .map(|s| s.to_string())
                        .unwrap_or(raw);
                    let temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                    bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                    emit_write_value(bcx, &children[0], temp_ptr, env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let (np, nl) = intern_str(string_pool, &accessor_name);
                    let np_v = bcx.ins().iconst(types::I64, np);
                    let nl_v = bcx.ins().iconst(types::I64, nl);
                    bcx.ins().call(helpers.extract_field,
                        &[out_slot, temp_ptr, np_v, nl_v]);
                    Some(())
                }
                DeclKind::DT_IS | DeclKind::DT_RECOGNISER => {
                    if children.len() != 1 { return None; }

                    let app_text = format!("{expr}");
                    let variant = crate::z3_eval::extract_is_variant_pub(&app_text)
                        .or_else(|| decl.name().strip_prefix("is_").map(|s| s.to_string()))?;
                    let temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                    bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                    emit_write_value(bcx, &children[0], temp_ptr, env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let (vp, vl) = intern_str(string_pool, &variant);
                    let vp_v = bcx.ins().iconst(types::I64, vp);
                    let vl_v = bcx.ins().iconst(types::I64, vl);
                    let call = bcx.ins().call(helpers.is_variant,
                        &[temp_ptr, vp_v, vl_v]);
                    let r = bcx.inst_results(call)[0];
                    bcx.ins().call(helpers.set_bool, &[out_slot, r]);
                    Some(())
                }
                DeclKind::CONST_ARRAY => {

                    let cap = bcx.ins().iconst(types::I64, 0);
                    bcx.ins().call(helpers.seq_new, &[out_slot, cap]);
                    Some(())
                }
                DeclKind::STORE => {

                    if children.len() != 3 { return None; }
                    emit_write_value(bcx, &children[0], out_slot, env,
                        helpers, variant_arity, record_info, string_pool,
                        ptr_t, size_of_value)?;
                    let idx_v = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                    bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                    emit_write_value(bcx, &children[2], temp_ptr, env,
                        helpers, variant_arity, record_info, string_pool,
                        ptr_t, size_of_value)?;
                    bcx.ins().call(helpers.seq_set, &[out_slot, idx_v, temp_ptr]);
                    Some(())
                }
                DeclKind::SELECT => {
                    if children.len() != 2 {
                        return None;
                    }
                    let temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                    bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                    if emit_write_value(bcx, &children[0], temp_ptr, env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value).is_none()
                    {
                        return None;
                    }
                    let idx_v = match emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)
                    {
                        Some(v) => v,
                        None => return None,
                    };
                    bcx.ins().call(helpers.seq_select,
                        &[out_slot, temp_ptr, idx_v]);
                    Some(())
                }
                DeclKind::DT_CONSTRUCTOR => {
                    let variant = decl.name();

                    if let Some(fields) = record_info.get(&variant) {
                        return emit_write_record(bcx, &children, fields, out_slot,
                            env, helpers, variant_arity, record_info,
                            string_pool, ptr_t, size_of_value);
                    }

                    let (enum_name, field_types) = lookup_variant(&variant, variant_arity)?;
                    let (ep, el) = intern_str(string_pool, &enum_name);
                    let (vp, vl) = intern_str(string_pool, &variant);
                    let ep_v = bcx.ins().iconst(types::I64, ep);
                    let el_v = bcx.ins().iconst(types::I64, el);
                    let vp_v = bcx.ins().iconst(types::I64, vp);
                    let vl_v = bcx.ins().iconst(types::I64, vl);
                    if field_types.is_empty() {
                        bcx.ins().call(helpers.set_enum_nullary,
                            &[out_slot, ep_v, el_v, vp_v, vl_v]);
                        return Some(());
                    }
                    if field_types.len() == 1 {
                        let arg = &children[0];
                        match field_types[0].as_str() {
                            "Int" | "Nat" => {
                                if let Some(n) = arg.as_int().and_then(|x| x.as_i64()) {
                                    let n_v = bcx.ins().iconst(types::I64, n);
                                    bcx.ins().call(helpers.set_enum_int,
                                        &[out_slot, ep_v, el_v, vp_v, vl_v, n_v]);
                                    return Some(());
                                }

                            }
                            "String" => {

                                let is_literal = arg.kind() == AstKind::App
                                    && arg.num_children() == 0
                                    && arg.safe_decl().ok()
                                        .map(|d| d.kind() != DeclKind::UNINTERPRETED)
                                        .unwrap_or(false);
                                if is_literal {
                                    if let Some(zs) = arg.as_string() {
                                        if let Some(s) = zs.as_string() {
                                            let (p, l) = intern_str(string_pool, &s);
                                            let p_v = bcx.ins().iconst(types::I64, p);
                                            let l_v = bcx.ins().iconst(types::I64, l);
                                            bcx.ins().call(helpers.set_enum_str,
                                                &[out_slot, ep_v, el_v, vp_v, vl_v, p_v, l_v]);
                                            return Some(());
                                        }
                                    }
                                }

                            }
                            _ => {}
                        }
                    }

                    let n = children.len();

                    let arg_slots: Vec<ClValue> = (0..n).map(|_| {
                        let s = bcx.create_sized_stack_slot(
                            StackSlotData::new(StackSlotKind::ExplicitSlot,
                                               size_of_value as u32));
                        bcx.ins().stack_addr(ptr_t, s, 0)
                    }).collect();
                    for s in &arg_slots {
                        bcx.ins().call(helpers.init_slot, &[*s]);
                    }

                    for (i, child) in children.iter().enumerate() {
                        emit_write_value(bcx, child, arg_slots[i], env,
                            helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    }

                    let array_slot = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           (n as u32) * 8));
                    let array_ptr = bcx.ins().stack_addr(ptr_t, array_slot, 0);
                    for (i, &s) in arg_slots.iter().enumerate() {
                        bcx.ins().store(MemFlags::new(),
                            s, array_ptr, (i as i32) * 8);
                    }
                    let n_v = bcx.ins().iconst(types::I64, n as i64);
                    bcx.ins().call(helpers.set_enum_multifield,
                        &[out_slot, ep_v, el_v, vp_v, vl_v, array_ptr, n_v]);
                    Some(())
                }
                DeclKind::ITE => {

                    if children.len() != 3 { return None; }
                    let cond_v = emit_compute_i64(bcx, &children[0], env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let then_block = bcx.create_block();
                    let else_block = bcx.create_block();
                    let merge_block = bcx.create_block();
                    bcx.ins().brif(cond_v, then_block, &[], else_block, &[]);
                    bcx.switch_to_block(then_block);
                    bcx.seal_block(then_block);
                    emit_write_value(bcx, &children[1], out_slot, env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    bcx.ins().jump(merge_block, &[]);
                    bcx.switch_to_block(else_block);
                    bcx.seal_block(else_block);
                    emit_write_value(bcx, &children[2], out_slot, env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    bcx.ins().jump(merge_block, &[]);
                    bcx.switch_to_block(merge_block);
                    bcx.seal_block(merge_block);
                    Some(())
                }
                DeclKind::ADD | DeclKind::SUB | DeclKind::MUL | DeclKind::UMINUS => {

                    let v = emit_compute_i64(bcx, expr, env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    bcx.ins().call(helpers.set_int, &[out_slot, v]);
                    Some(())
                }
                DeclKind::LT | DeclKind::LE | DeclKind::GT | DeclKind::GE
                | DeclKind::EQ | DeclKind::AND | DeclKind::OR | DeclKind::NOT => {

                    let v = emit_compute_i64(bcx, expr, env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    bcx.ins().call(helpers.set_bool, &[out_slot, v]);
                    Some(())
                }
                _ => None,
            }
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_write_record<'ctx>(
    bcx: &mut FunctionBuilder,
    children: &[Dynamic<'ctx>],
    fields: &[crate::core::FieldKind],
    out_slot: ClValue,
    env: &HashMap<String, ClValue>,
    helpers: &HelperRefs,
    variant_arity: &HashMap<String, HashMap<String, Vec<String>>>,
    record_info: &HashMap<String, Vec<crate::core::FieldKind>>,
    string_pool: &mut Vec<Box<str>>,
    ptr_t: cranelift::prelude::Type,
    size_of_value: i64,
) -> Option<()> {
    use crate::core::FieldKind;
    let n = fields.len();

    let val_slots: Vec<ClValue> = (0..n).map(|_| {
        let s = bcx.create_sized_stack_slot(
            StackSlotData::new(StackSlotKind::ExplicitSlot, size_of_value as u32));
        let p = bcx.ins().stack_addr(ptr_t, s, 0);
        bcx.ins().call(helpers.init_slot, &[p]);
        p
    }).collect();

    let mut name_pl: Vec<(i64, i64)> = Vec::with_capacity(n);
    let mut arg_idx = 0usize;
    for (fi, fk) in fields.iter().enumerate() {
        name_pl.push(intern_str(string_pool, fk.name()));
        match fk {

            FieldKind::SeqField { .. } => {
                let arr_child = children.get(arg_idx)?;
                emit_write_value(bcx, arr_child, val_slots[fi], env,
                    helpers, variant_arity, record_info, string_pool,
                    ptr_t, size_of_value)?;
                arg_idx += 2;
            }

            _ => {
                let child = children.get(arg_idx)?;
                emit_write_value(bcx, child, val_slots[fi], env,
                    helpers, variant_arity, record_info, string_pool,
                    ptr_t, size_of_value)?;
                arg_idx += 1;
            }
        }
    }

    let mk_arr = |bcx: &mut FunctionBuilder| {
        let slot = bcx.create_sized_stack_slot(
            StackSlotData::new(StackSlotKind::ExplicitSlot, (n.max(1) as u32) * 8));
        bcx.ins().stack_addr(ptr_t, slot, 0)
    };
    let name_ptr_base = mk_arr(bcx);
    let name_len_base = mk_arr(bcx);
    let val_ptr_base  = mk_arr(bcx);
    for (i, (p, l)) in name_pl.iter().enumerate() {
        let pv = bcx.ins().iconst(types::I64, *p);
        bcx.ins().store(MemFlags::new(), pv, name_ptr_base, (i as i32) * 8);
        let lv = bcx.ins().iconst(types::I64, *l);
        bcx.ins().store(MemFlags::new(), lv, name_len_base, (i as i32) * 8);
    }
    for (i, &s) in val_slots.iter().enumerate() {
        bcx.ins().store(MemFlags::new(), s, val_ptr_base, (i as i32) * 8);
    }
    let n_v = bcx.ins().iconst(types::I64, n as i64);
    bcx.ins().call(helpers.set_composite,
        &[out_slot, name_ptr_base, name_len_base, val_ptr_base, n_v]);
    Some(())
}

fn emit_compute_i64<'ctx>(
    bcx: &mut FunctionBuilder,
    expr: &Dynamic<'ctx>,
    env: &HashMap<String, ClValue>,
    helpers: &HelperRefs,
    variant_arity: &HashMap<String, HashMap<String, Vec<String>>>,
    record_info: &HashMap<String, Vec<crate::core::FieldKind>>,
    string_pool: &mut Vec<Box<str>>,
    ptr_t: cranelift::prelude::Type,
    size_of_value: i64,
) -> Option<ClValue> {
    match expr.kind() {
        AstKind::Numeral => {
            let i = expr.as_int().and_then(|x| x.as_i64())?;
            Some(bcx.ins().iconst(types::I64, i))
        }
        AstKind::App => {
            let decl = expr.safe_decl().ok()?;
            let kind = decl.kind();
            let children: Vec<Dynamic<'ctx>> = expr.children();
            match kind {
                DeclKind::TRUE  => Some(bcx.ins().iconst(types::I64, 1)),
                DeclKind::FALSE => Some(bcx.ins().iconst(types::I64, 0)),
                DeclKind::UNINTERPRETED => {
                    if !children.is_empty() { return None; }
                    let name = decl.name();
                    let src_slot = *env.get(&name)?;

                    let sort_name = format!("{}", expr.get_sort());
                    let loader = if sort_name == "Bool" {
                        helpers.load_bool
                    } else {
                        helpers.load_int
                    };
                    let call = bcx.ins().call(loader, &[src_slot]);
                    let result = bcx.inst_results(call)[0];
                    Some(result)
                }
                DeclKind::ADD | DeclKind::SUB | DeclKind::MUL => {
                    if children.is_empty() { return None; }
                    let mut acc = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    for c in &children[1..] {
                        let v = emit_compute_i64(bcx, c, env, helpers,
                            variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                        acc = match kind {
                            DeclKind::ADD => bcx.ins().iadd(acc, v),
                            DeclKind::SUB => bcx.ins().isub(acc, v),
                            DeclKind::MUL => bcx.ins().imul(acc, v),
                            _ => unreachable!(),
                        };
                    }
                    Some(acc)
                }
                DeclKind::UMINUS => {
                    if children.len() != 1 { return None; }
                    let v = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    Some(bcx.ins().ineg(v))
                }
                DeclKind::IDIV | DeclKind::DIV => {
                    if children.len() != 2 { return None; }
                    let l = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let r = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    Some(bcx.ins().sdiv(l, r))
                }
                DeclKind::MOD | DeclKind::REM => {
                    if children.len() != 2 { return None; }
                    let l = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let r = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    Some(bcx.ins().srem(l, r))
                }
                DeclKind::LT | DeclKind::LE | DeclKind::GT | DeclKind::GE
                | DeclKind::EQ => {
                    if children.len() != 2 { return None; }

                    if matches!(kind, DeclKind::EQ) {
                        let try_nullary_eq = |child: &Dynamic<'ctx>, other: &Dynamic<'ctx>|
                            -> Option<ClValue>
                        {
                            if child.kind() == AstKind::App {
                                let d = child.safe_decl().ok()?;
                                if d.kind() == DeclKind::DT_CONSTRUCTOR
                                    && child.num_children() == 0
                                {
                                    let variant = d.name();

                                    let _ = variant;
                                    return Some(ClValue::from_u32(0));
                                }
                            }
                            None
                        };
                        if try_nullary_eq(&children[1], &children[0]).is_some() {

                            let variant = children[1].safe_decl().ok()?.name();
                            let temp = bcx.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot,
                                                   size_of_value as u32));
                            let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                            bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                            emit_write_value(bcx, &children[0], temp_ptr, env,
                                helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                            let (vp, vl) = intern_str(string_pool, &variant);
                            let vp_v = bcx.ins().iconst(types::I64, vp);
                            let vl_v = bcx.ins().iconst(types::I64, vl);
                            let call = bcx.ins().call(helpers.is_variant,
                                &[temp_ptr, vp_v, vl_v]);
                            return Some(bcx.inst_results(call)[0]);
                        }
                        if try_nullary_eq(&children[0], &children[1]).is_some() {
                            let variant = children[0].safe_decl().ok()?.name();
                            let temp = bcx.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot,
                                                   size_of_value as u32));
                            let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                            bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                            emit_write_value(bcx, &children[1], temp_ptr, env,
                                helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                            let (vp, vl) = intern_str(string_pool, &variant);
                            let vp_v = bcx.ins().iconst(types::I64, vp);
                            let vl_v = bcx.ins().iconst(types::I64, vl);
                            let call = bcx.ins().call(helpers.is_variant,
                                &[temp_ptr, vp_v, vl_v]);
                            return Some(bcx.inst_results(call)[0]);
                        }
                    }
                    let l = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let r = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    use cranelift::prelude::IntCC;
                    let cc = match kind {
                        DeclKind::LT => IntCC::SignedLessThan,
                        DeclKind::LE => IntCC::SignedLessThanOrEqual,
                        DeclKind::GT => IntCC::SignedGreaterThan,
                        DeclKind::GE => IntCC::SignedGreaterThanOrEqual,
                        DeclKind::EQ => IntCC::Equal,
                        _ => unreachable!(),
                    };
                    let cmp = bcx.ins().icmp(cc, l, r);

                    Some(bcx.ins().uextend(types::I64, cmp))
                }
                DeclKind::AND => {
                    if children.is_empty() { return Some(bcx.ins().iconst(types::I64, 1)); }
                    let mut acc = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    for c in &children[1..] {
                        let v = emit_compute_i64(bcx, c, env, helpers,
                            variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                        acc = bcx.ins().band(acc, v);
                    }
                    Some(acc)
                }
                DeclKind::OR => {
                    if children.is_empty() { return Some(bcx.ins().iconst(types::I64, 0)); }
                    let mut acc = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    for c in &children[1..] {
                        let v = emit_compute_i64(bcx, c, env, helpers,
                            variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                        acc = bcx.ins().bor(acc, v);
                    }
                    Some(acc)
                }
                DeclKind::NOT => {
                    if children.len() != 1 { return None; }
                    let v = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let one = bcx.ins().iconst(types::I64, 1);
                    Some(bcx.ins().bxor(v, one))
                }
                DeclKind::ITE => {
                    if children.len() != 3 { return None; }
                    let cond = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let t = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let e = emit_compute_i64(bcx, &children[2], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    Some(bcx.ins().select(cond, t, e))
                }
                DeclKind::DT_IS | DeclKind::DT_RECOGNISER => {
                    if children.len() != 1 { return None; }
                    let app_text = format!("{expr}");
                    let variant = crate::z3_eval::extract_is_variant_pub(&app_text)
                        .or_else(|| decl.name().strip_prefix("is_").map(|s| s.to_string()))?;

                    let temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                    bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                    emit_write_value(bcx, &children[0], temp_ptr, env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let (vp, vl) = intern_str(string_pool, &variant);
                    let vp_v = bcx.ins().iconst(types::I64, vp);
                    let vl_v = bcx.ins().iconst(types::I64, vl);
                    let call = bcx.ins().call(helpers.is_variant,
                        &[temp_ptr, vp_v, vl_v]);
                    Some(bcx.inst_results(call)[0])
                }
                DeclKind::DT_ACCESSOR => {
                    if children.len() != 1 { return None; }
                    let raw = decl.name();
                    let accessor_name = raw.strip_suffix("__arr")
                        .or_else(|| raw.strip_suffix("__len"))
                        .map(|s| s.to_string())
                        .unwrap_or(raw);

                    let inner_temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let inner_ptr = bcx.ins().stack_addr(ptr_t, inner_temp, 0);
                    bcx.ins().call(helpers.init_slot, &[inner_ptr]);
                    emit_write_value(bcx, &children[0], inner_ptr, env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let field_temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let field_ptr = bcx.ins().stack_addr(ptr_t, field_temp, 0);
                    bcx.ins().call(helpers.init_slot, &[field_ptr]);
                    let (np, nl) = intern_str(string_pool, &accessor_name);
                    let np_v = bcx.ins().iconst(types::I64, np);
                    let nl_v = bcx.ins().iconst(types::I64, nl);
                    bcx.ins().call(helpers.extract_field,
                        &[field_ptr, inner_ptr, np_v, nl_v]);

                    let call = bcx.ins().call(helpers.load_int, &[field_ptr]);
                    Some(bcx.inst_results(call)[0])
                }
                DeclKind::SELECT => {
                    if children.len() != 2 { return None; }

                    let arr_temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let arr_ptr = bcx.ins().stack_addr(ptr_t, arr_temp, 0);
                    bcx.ins().call(helpers.init_slot, &[arr_ptr]);
                    emit_write_value(bcx, &children[0], arr_ptr, env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let idx_v = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let elem_temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let elem_ptr = bcx.ins().stack_addr(ptr_t, elem_temp, 0);
                    bcx.ins().call(helpers.init_slot, &[elem_ptr]);
                    bcx.ins().call(helpers.seq_select,
                        &[elem_ptr, arr_ptr, idx_v]);
                    let call = bcx.ins().call(helpers.load_int, &[elem_ptr]);
                    Some(bcx.inst_results(call)[0])
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn kind_of_dynamic<'ctx>(
    e: &Dynamic<'ctx>,
    enum_variants: &HashMap<String, Vec<String>>,
    variant_arity: &HashMap<String, HashMap<String, Vec<String>>>,
) -> Option<OutputKind> {
    let sort = e.get_sort();
    let sort_name = format!("{sort}");
    if sort_name == "Int" || sort_name == "Real" { return Some(OutputKind::Int); }
    if sort_name == "Bool"   { return Some(OutputKind::Bool); }
    if sort_name == "String" { return Some(OutputKind::Str); }
    for (en, _) in enum_variants {
        if &sort_name == en {
            let all_nullary = variant_arity.get(en).map(|m|
                m.values().all(|v| v.is_empty())).unwrap_or(true);
            return Some(if all_nullary {
                OutputKind::Enum(en.clone())
            } else {
                OutputKind::EnumPayload(en.clone())
            });
        }
    }
    None
}

fn collect_inputs<'ctx>(
    e: &Dynamic<'ctx>,
    out: &mut std::collections::BTreeSet<(String, OutputKind)>,
    enum_variants: &HashMap<String, Vec<String>>,
    variant_arity: &HashMap<String, HashMap<String, Vec<String>>>,
) {
    if e.kind() == AstKind::App {
        if let Ok(decl) = e.safe_decl() {
            if decl.kind() == DeclKind::UNINTERPRETED && e.num_children() == 0 {
                let name = decl.name();

                let k = kind_of_dynamic(e, enum_variants, variant_arity)
                    .unwrap_or(OutputKind::Seq);
                out.insert((name, k));
                return;
            }
        }
        for c in e.children() {
            collect_inputs(&c, out, enum_variants, variant_arity);
        }
    }
}

fn lookup_variant(
    variant: &str,
    variant_arity: &HashMap<String, HashMap<String, Vec<String>>>,
) -> Option<(String, Vec<String>)> {
    for (en, vs) in variant_arity {
        if let Some(fields) = vs.get(variant) {
            return Some((en.clone(), fields.clone()));
        }
    }
    None
}

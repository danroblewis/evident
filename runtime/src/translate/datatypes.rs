use std::collections::HashMap;
use z3::{Context, DatatypeAccessor, DatatypeBuilder, DatatypeSort, Sort};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, FieldKind, SeqElem, SeqFieldElem};

pub(super) fn get_or_build_datatype(
    type_name: &str,
    ctx: &'static Context,
    schemas: &HashMap<String, SchemaDecl>,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
) -> Option<(&'static DatatypeSort<'static>, Vec<FieldKind>)> {

    if let Some((dt, fields)) = registry.borrow().get(type_name) {
        return Some((*dt, fields.clone()));
    }
    let schema = schemas.get(type_name)?;

    let mut fields: Vec<FieldKind> = Vec::new();
    let mut field_sorts: Vec<(String, Sort<'static>)> = Vec::new();
    for item in &schema.body {
        if let BodyItem::Membership { name, type_name: ftype, .. } = item {
            match ftype.as_str() {
                "Int" | "Nat" | "Pos" => {
                    fields.push(FieldKind::Primitive {
                        name: name.clone(),
                        prim_type: ftype.clone(),
                    });
                    field_sorts.push((name.clone(), Sort::int(ctx)));
                }
                "Bool" => {
                    fields.push(FieldKind::Primitive {
                        name: name.clone(),
                        prim_type: ftype.clone(),
                    });
                    field_sorts.push((name.clone(), Sort::bool(ctx)));
                }
                "String" => {
                    fields.push(FieldKind::Primitive {
                        name: name.clone(),
                        prim_type: ftype.clone(),
                    });
                    field_sorts.push((name.clone(), Sort::string(ctx)));
                }

                s if s.starts_with("Seq(") && s.ends_with(')') => {
                    let inner = &s[4..s.len() - 1];
                    let (elem_sort, seq_elem): (Sort<'static>, SeqFieldElem) = match inner {
                        "Int" | "Nat" | "Pos" =>
                            (Sort::int(ctx), SeqFieldElem::Primitive(SeqElem::Int)),
                        "Bool" =>
                            (Sort::bool(ctx), SeqFieldElem::Primitive(SeqElem::Bool)),
                        "String" =>
                            (Sort::string(ctx), SeqFieldElem::Primitive(SeqElem::Str)),
                        enum_name if enums
                            .map(|er| er.by_name.borrow().contains_key(enum_name))
                            .unwrap_or(false) =>
                        {
                            let er = enums.unwrap();
                            let dts = er.by_name.borrow();
                            let (dt, _variants) = dts.get(enum_name).unwrap();
                            (dt.sort.clone(), SeqFieldElem::Enum {
                                enum_name: enum_name.to_string(),
                                dt: *dt,
                            })
                        }
                        user_type if schemas.contains_key(user_type) => {
                            let Some((nested_dt, sub_fields)) = get_or_build_datatype(
                                user_type, ctx, schemas, registry, enums)
                            else { return None; };
                            (nested_dt.sort.clone(), SeqFieldElem::Composite {
                                type_name: user_type.to_string(),
                                dt: nested_dt,
                                sub_fields,
                            })
                        }
                        _ => {
                            eprintln!(
                                "warning: unsupported Seq element {} in Datatype \
                                 field `{}` for {}; supported: Int/Nat/Pos/Bool/\
                                 String, enums, user structs",
                                inner, name, type_name
                            );
                            return None;
                        }
                    };
                    let arr_idx = fields.len();
                    let arr_sort = Sort::array(ctx, &Sort::int(ctx), &elem_sort);
                    field_sorts.push((format!("{}__arr", name), arr_sort));
                    field_sorts.push((format!("{}__len", name), Sort::int(ctx)));
                    fields.push(FieldKind::SeqField {
                        name: name.clone(),
                        arr_idx,
                        len_idx: arr_idx + 1,
                        elem_type_name: inner.to_string(),
                        elem: seq_elem,
                    });
                }

                user_type if schemas.contains_key(user_type) => {
                    let Some((nested_dt, sub_fields)) =
                        get_or_build_datatype(user_type, ctx, schemas, registry, enums)
                    else {

                        return None;
                    };
                    field_sorts.push((name.clone(), nested_dt.sort.clone()));
                    fields.push(FieldKind::Nested {
                        name: name.clone(),
                        type_name: user_type.to_string(),
                        dt: nested_dt,
                        sub_fields,
                    });
                }
                _ => {
                    eprintln!(
                        "warning: unsupported field type {} in Datatype for {}; \
                         supported: Int/Nat/Pos/Bool/String, Seq(...), enums, \
                         user struct types",
                        ftype, type_name
                    );
                    return None;
                }
            }
        }

    }
    if fields.is_empty() {
        eprintln!("warning: type {} has no fields; can't build Datatype", type_name);
        return None;
    }

    let ctor_name = format!("mk_{}", type_name);
    let field_refs: Vec<(&str, DatatypeAccessor<'static>)> = field_sorts
        .iter()
        .map(|(n, s)| (n.as_str(), DatatypeAccessor::Sort(s.clone())))
        .collect();
    let dt: DatatypeSort<'static> = DatatypeBuilder::new(ctx, type_name)
        .variant(&ctor_name, field_refs)
        .finish();
    let leaked: &'static DatatypeSort<'static> = Box::leak(Box::new(dt));
    registry.borrow_mut().insert(type_name.to_string(), (leaked, fields.clone()));
    Some((leaked, fields))
}

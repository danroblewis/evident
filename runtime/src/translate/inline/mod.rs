//! Per-claim constraint inlining: schema body items → Z3 assertions.
//! Sub-modules: walk (entry points), membership, calls, subschema, dispatch, rewrite, recursion, guards.

mod calls;
mod dispatch;
mod guards;
mod membership;
mod recursion;
mod rewrite;
mod subschema;
mod walk;

pub(in crate::translate) use walk::inline_body_items;

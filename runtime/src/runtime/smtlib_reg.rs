//! Registration of SMT-LIB-driven FSMs (strategy 2 of runtime-evolve).
//!
//! Registering an [`SmtLibFsm`] does two things: (1) inserts the FSM into the
//! `smtlib_fsms` registry so `query_with_pins_and_given` routes its per-tick
//! solve to the SMT-LIB path, and (2) inserts the *synthetic* `fsm`-keyword
//! `SchemaDecl` (Memberships only) so the scheduler's `resolve_fsm` /
//! `MainShape` / `all_fsms` / `_var` time-shift see the FSM's shape. The
//! behavior lives in the SMT-LIB text; the schema is just the shape.

use crate::core::ast::{BodyItem, Keyword, Pins, SchemaDecl};
use crate::smtlib_fsm::{FixtureProgram, SmtLibFsm, WorldDecl};

use super::EvidentRuntime;

impl EvidentRuntime {
    /// Register one SMT-LIB FSM: store it in the registry and inject its
    /// synthetic shape schema. Overwrites any prior schema of the same name.
    pub fn register_smtlib_fsm(&mut self, fsm: SmtLibFsm) {
        let schema = fsm.synthetic_schema();
        let name = schema.name.clone();
        if self.schemas.insert(name.clone(), schema).is_none() {
            self.schema_order.push(name.clone());
        }
        self.smtlib_fsms.borrow_mut().insert(name, fsm);
    }

    /// Register a shared `world` record type from a fixture's world declaration,
    /// as a plain `type` whose fields the scheduler's world plumbing reads.
    pub fn register_smtlib_world(&mut self, world: &WorldDecl) {
        let body: Vec<BodyItem> = world
            .fields
            .iter()
            .map(|f| BodyItem::Membership {
                name: f.name.clone(),
                type_name: f.sort.evident_type().to_string(),
                pins: Pins::None,
            })
            .collect();
        let schema = SchemaDecl {
            keyword: Keyword::Type,
            name: world.type_name.clone(),
            type_params: vec![],
            param_count: 0,
            external: false,
            body,
        };
        let name = schema.name.clone();
        if self.schemas.insert(name.clone(), schema).is_none() {
            self.schema_order.push(name);
        }
    }

    /// Register a whole fixture program (optional world + all FSMs).
    pub fn register_smtlib_program(&mut self, program: FixtureProgram) {
        if let Some(world) = &program.world {
            self.register_smtlib_world(world);
        }
        for fsm in program.fsms {
            self.register_smtlib_fsm(fsm);
        }
    }
}

//! Probe: which (if any) Mario FSM bodies now pass the function-izer
//! gate after Rounds 2-4 expansions?

use evident_runtime::EvidentRuntime;
use std::path::Path;

fn main() {
    let mut rt = EvidentRuntime::new();
    std::env::set_var("EVIDENT_LENIENT", "1");
    if let Err(e) = rt.load_file(Path::new("../examples/test_21_mario/main.ev")) {
        eprintln!("load failed: {:?}", e);
        return;
    }
    let names = ["level_gen", "game", "keyboard", "display"];
    for name in names {
        let Some(_) = rt.get_schema(name) else {
            println!("{}: not loaded", name);
            continue;
        };
        // We can't call try_functionize directly (private). Use rt.query
        // with EVIDENT_FUNCTIONIZE=1 + EVIDENT_FUNCTIONIZE_TRACE=1
        // and look for HIT/MISS output. Since these FSMs need plenty
        // of given values, we'll just classify the components (which
        // includes the gate check + functionality verdict).
        match rt.classify_components(name, &std::collections::HashMap::new()) {
            Ok(comps) => {
                let mvar: Vec<_> = comps.iter().filter(|c| c.component.vars.len() > 1).collect();
                let func_mvar = mvar.iter().filter(|c| c.functional).count();
                println!("{:<15}: {:>3} components ({:>3} singletons), {:>2} multi-var ({:>2} functional)",
                    name,
                    comps.len(),
                    comps.iter().filter(|c| c.component.vars.len() == 1).count(),
                    mvar.len(),
                    func_mvar);
            }
            Err(e) => println!("{}: classify error: {:?}", name, e),
        }
    }
}

# runtime/src/fti.rs — Z3-replaceability
**What it does:** FTI (Foreign Type Interface) registry: maps type-name strings (`"FrameClock"`, `"Timer"`) to install functions that spawn thread-driven event sources and return `FtiInstall { source, keys }`. Also maintains the shimmed-stdlib list.
**Criticality:** critical
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** Tier-0 kernel. The registry is a static lookup table (two entries) plus install functions that spawn OS threads and send on `mpsc::Sender<SchedulerEvent>`. There is no search space; the "solve" is just name→fn dispatch. Z3 cannot spawn threads or produce OS event sources. The install functions themselves require unsafe/OS primitives.
**Change made:** none

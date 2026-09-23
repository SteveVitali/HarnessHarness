//! `DecisionGuard` — the runtime half of AC-R-2.8.1-11: **no model call while a
//! decision is open**. The monitor's `authorize` holds the guard for its
//! duration; a `model_call` attempted while the guard is held trips
//! `ModelCallDuringDecision` — a runtime assertion, not a decision (the
//! invariant is structural: the authorize path must be re-entrant-safe against
//! a model call arriving mid-decision, and at Stage 1 there is no legal path
//! that issues one).

/// `ModelCallDuringDecision` — the runtime trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelCallDuringDecision;

/// The decision-open guard — a scoped flag. `open()` sets it; the returned
/// `DecisionGuard` clears it on `Drop`. `try_model_call` refuses while open.
pub struct DecisionGuard {
    cell: std::rc::Rc<std::cell::Cell<bool>>,
}

impl DecisionGuard {
    /// A fresh flag cell — the monitor's `decision_open` state is the caller's
    /// `Rc<Cell<bool>>` when the monitor is shared; this free-standing cell
    /// covers the common "guard this decision" use.
    pub fn new_cell() -> std::rc::Rc<std::cell::Cell<bool>> {
        std::rc::Rc::new(std::cell::Cell::new(false))
    }

    /// Open a decision on `cell` — the guard holds it set until dropped.
    pub fn open(cell: std::rc::Rc<std::cell::Cell<bool>>) -> DecisionGuard {
        cell.set(true);
        DecisionGuard { cell }
    }

    /// Whether a decision is open on this cell.
    pub fn is_open(&self) -> bool {
        self.cell.get()
    }
}

impl Drop for DecisionGuard {
    fn drop(&mut self) {
        self.cell.set(false);
    }
}

/// `try_model_call(cell)` — the runtime assertion: `Err(ModelCallDuringDecision)`
/// while a decision is open on `cell`.
pub fn try_model_call(
    cell: &std::rc::Rc<std::cell::Cell<bool>>,
) -> Result<(), ModelCallDuringDecision> {
    if cell.get() {
        Err(ModelCallDuringDecision)
    } else {
        Ok(())
    }
}

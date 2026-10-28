//! Sparse flow builder for one mechanism's algebraic variable.
use crate::dual_var::DualVar;
use crate::prim_var::Flow;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TempFlow {
    flow: Flow,
}

impl TempFlow {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a coefficient, removing exact cancellations to keep the flow sparse.
    pub fn add(&mut self, var: DualVar, amount: f64) {
        if amount == 0.0 {
            return;
        }
        let new_amount = self.flow.get(&var).copied().unwrap_or_default() + amount;
        if new_amount == 0.0 {
            self.flow.shift_remove(&var);
        } else {
            self.flow.insert(var, new_amount);
        }
    }

    pub fn scale(&mut self, factor: f64) {
        if factor != 1.0 {
            for amount in self.flow.values_mut() {
                *amount *= factor;
            }
        }
    }

    pub fn into_flow(self) -> Flow {
        self.flow
    }
}

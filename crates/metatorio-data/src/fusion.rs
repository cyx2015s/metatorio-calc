//! Fusion temperatures and project-independent default estimates.
use crate::store::{AIndexMap, PrototypeGroup, PrototypeStore};
use crate::{FluidComponent, FusionReactorComponent};
use std::collections::BTreeMap;

pub const DEFAULT_REACTOR_NEIGHBOURS: u8 = 3;
pub fn reactor_neighbours(estimates: &BTreeMap<String, u8>, entity: &str) -> u8 {
    estimates
        .get(entity)
        .copied()
        .unwrap_or(DEFAULT_REACTOR_NEIGHBOURS)
}
/// Keep the same truncation policy as existing fluid temperature identities.
pub fn fusion_temperature(
    reactor: &FusionReactorComponent,
    output: &FluidComponent,
    neighbours: u8,
) -> Option<i32> {
    let value = reactor
        .target_temperature
        .unwrap_or(output.default_temperature)
        * (1.0 + f64::from(neighbours) * reactor.neighbour_bonus);
    (value.is_finite() && value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX))
        .then_some(value as i32)
}
impl PrototypeStore {
    /// Static prototype temperatures plus project overrides. Connection counts
    /// only broaden the temperature vocabulary; they do not promise a layout.
    pub fn fluid_temperatures_with_reactors(
        &self,
        estimates: &BTreeMap<String, u8>,
    ) -> AIndexMap<String, Vec<i32>> {
        let mut out = self.fluid_temperatures().clone();
        self.add_fusion_temperatures(&mut out, estimates);
        out
    }
    pub(crate) fn add_fusion_temperatures(
        &self,
        out: &mut AIndexMap<String, Vec<i32>>,
        estimates: &BTreeMap<String, u8>,
    ) {
        for record in self.group(PrototypeGroup::Entity) {
            let Some(reactor) = record.component::<FusionReactorComponent>() else {
                continue;
            };
            let Some(name) = reactor.output_fluid_box.filter.as_deref() else {
                continue;
            };
            let Some(fluid) = self
                .get(PrototypeGroup::Fluid, name)
                .and_then(|r| r.component::<FluidComponent>())
            else {
                continue;
            };
            let ports = reactor
                .neighbour_connectable
                .as_ref()
                .map_or(0, |c| c.connections.len())
                .min(usize::from(u8::MAX)) as u8;
            let maximum = ports.max(reactor_neighbours(estimates, &record.name));
            let temperatures = out.entry(name.to_string()).or_default();
            temperatures
                .extend((0..=maximum).filter_map(|n| fusion_temperature(reactor, fluid, n)));
            temperatures.sort_unstable();
            temperatures.dedup();
        }
    }
}

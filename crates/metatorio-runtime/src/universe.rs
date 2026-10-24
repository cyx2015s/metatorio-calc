//! Universe-factory expansion and LP solve.
//!
//! Locations are part of the LP coordinates: a `LocatedDualVar` is balanced
//! independently at each planet/space location. Local mechanics are expanded
//! with the environment of their own location; transfers consume local rocket
//! weight capacity and the item, then create the same item in space. Space
//! items can fan out to the same item at every planet for free.

use std::collections::{BTreeSet, HashMap};

use metatorio_core::{
    AIndexMap, Context, DualVar, Flow, IdWithQuality, LocatedDualVar, LocatedMechanic, Location,
};
use metatorio_data::store::PrototypeGroup;
use metatorio_data::store::PrototypeStore;
use metatorio_solver::{Flow as SolverFlow, SolverData, SolverSolution};
use serde::{Deserialize, Serialize};

use crate::document::{ProjectDocument, UniverseDocument};
use crate::id::{MechanicId, ProjectId, UniverseId};
use crate::solve::{
    SolveDiagnostics, apply_environment_to_game_state, make_game_state_with_accessibility,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UniverseVarId(pub u64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocatedMechanicSolution {
    pub mechanic: MechanicId,
    pub location: Location,
    pub amount: f64,
    pub rate: f64,
    pub cost: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocatedFlow {
    pub flow: LocatedDualVar,
    pub amount: f64,
    pub rate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum UniverseSolveStatus {
    Solved {
        cost: f64,
        flows: Vec<LocatedFlow>,
        mechanics: Vec<LocatedMechanicSolution>,
    },
    NotSolved {
        description: String,
        no_provider: Vec<LocatedDualVar>,
        no_consumer: Vec<LocatedDualVar>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniverseSolveResult {
    pub project: ProjectId,
    pub universe: UniverseId,
    pub status: UniverseSolveStatus,
    pub report: SolveDiagnostics,
}

#[derive(Debug, Clone)]
enum VarOrigin {
    Local {
        mechanic: MechanicId,
        location: Location,
        cost: f64,
    },
    Transfer {
        mechanic: MechanicId,
        location: Location,
    },
    Conversion,
}

fn environment_for(location: &Location) -> (Option<&str>, Option<&str>) {
    match location {
        Location::Planet(name) => (Some(name.as_str()), None),
        Location::Space => (None, Some("space-platform")),
    }
}

fn locate_flow(location: &Location, flow: Flow) -> SolverFlow<LocatedDualVar> {
    flow.into_iter()
        .map(|(flow, amount)| (LocatedDualVar::new(location.clone(), flow), amount))
        .collect()
}

/// All asteroid-chunk items are available in space. This intentionally unions
/// every chunk prototype instead of relying on one space-location name: a
/// universe factory has one abstract Space location.
fn space_source_flows(store: &PrototypeStore) -> Flow {
    let mut out = Flow::default();
    for record in store.group(PrototypeGroup::AsteroidChunk) {
        let Some(chunk) = record.component::<metatorio_data::AsteroidChunkComponent>() else {
            continue;
        };
        let Some(minable) = &chunk.minable else {
            continue;
        };
        if minable.count.unwrap_or(1) == 0 {
            continue;
        }
        let mut add = |name: &str| {
            if store.item(name).is_some() {
                out.insert(
                    DualVar::Item(IdWithQuality::new(name, metatorio_core::NORMAL_QUALITY)),
                    0.0,
                );
            }
        };
        if let Some(result) = &minable.result {
            add(result);
        }
        for product in &minable.results {
            if let metatorio_data::types::Product::Item(product) = product {
                add(&product.name);
            }
        }
    }
    out
}

fn planet_source_flows(store: &PrototypeStore, planet: &str) -> Flow {
    crate::planet::implicit_source_flows(store, planet, None)
}

/// Return all locations mentioned by mechanics and targets.
fn used_locations(universe: &UniverseDocument) -> BTreeSet<Location> {
    let mut locations = BTreeSet::new();
    // Space is always available: its asteroid chunks are global implicit
    // sources and can feed planet targets even without an explicit space
    // mechanic.
    locations.insert(Location::Space);
    for entry in &universe.mechanics {
        locations.insert(entry.mechanic.location().clone());
    }
    for target in &universe.targets {
        locations.insert(target.flow.location.clone());
    }
    locations
}

fn add_located_conversion_flows(
    flows: &mut metatorio_core::AIndexMap<UniverseVarId, (SolverFlow<LocatedDualVar>, f64)>,
    locations: &BTreeSet<Location>,
    target: &SolverFlow<LocatedDualVar>,
    sources: &SolverFlow<LocatedDualVar>,
    store: &PrototypeStore,
    next_id: &mut u64,
) {
    for location in locations {
        let mut local_flows: metatorio_core::AIndexMap<crate::solve::ExpandedVarId, (Flow, f64)> =
            metatorio_core::AIndexMap::default();
        let mut synthetic = 0u64;
        for (_, (flow, cost)) in flows.iter() {
            let mut local = Flow::default();
            for (located, value) in flow {
                if &located.location == location {
                    *local.entry(located.flow.clone()).or_insert(0.0) += *value;
                }
            }
            if !local.is_empty() {
                local_flows.insert(
                    crate::solve::ExpandedVarId {
                        mechanic: MechanicId(synthetic),
                        variant: 0,
                    },
                    (local, *cost),
                );
                synthetic += 1;
            }
        }
        let local_target: Flow = target
            .iter()
            .filter(|(flow, _)| &flow.location == location)
            .map(|(flow, value)| (flow.flow.clone(), *value))
            .collect();
        let local_sources: Flow = sources
            .iter()
            .filter(|(flow, _)| &flow.location == location)
            .map(|(flow, value)| (flow.flow.clone(), *value))
            .collect();
        let before = local_flows.len();
        crate::solve::add_conversion_flows(&mut local_flows, store, &local_target, &local_sources);
        let aux: Vec<Flow> = local_flows
            .iter()
            .skip(before)
            .filter(|(id, _)| id.mechanic == MechanicId(u64::MAX))
            .map(|(_, (flow, _))| flow.clone())
            .collect();
        for flow in aux {
            let id = UniverseVarId(*next_id);
            *next_id += 1;
            flows.insert(id, (locate_flow(location, flow), 0.0));
        }
    }
}

/// Build and solve a universe factory with the project's selected backend.
pub fn solve_universe(
    store: &PrototypeStore,
    project: &ProjectDocument,
    universe: &UniverseDocument,
    accessibility: &metatorio_core::Accessibility,
) -> Result<UniverseSolveResult, String> {
    let base = make_game_state_with_accessibility(store, project, accessibility);
    let mut flows: metatorio_core::AIndexMap<UniverseVarId, (SolverFlow<LocatedDualVar>, f64)> =
        metatorio_core::AIndexMap::default();
    let mut origins = HashMap::new();
    let mut next_id = 0u64;
    let mut next_variant: HashMap<MechanicId, u16> = HashMap::new();

    for entry in &universe.mechanics {
        if !entry.enabled {
            continue;
        }
        match &entry.mechanic {
            LocatedMechanic::Local(local) => {
                let (planet, surface) = environment_for(&local.location);
                let mut game = base.clone();
                apply_environment_to_game_state(store, &mut game, planet, surface);
                let context = Context::new(store, &game);
                let expansion =
                    metatorio_core::expand::expand([(entry.id, &local.mechanic)], &context);
                for variable in expansion.variables {
                    let id = UniverseVarId(next_id);
                    next_id += 1;
                    let variant = *next_variant.entry(entry.id).or_default();
                    *next_variant.get_mut(&entry.id).unwrap() = variant.saturating_add(1);
                    flows.insert(
                        id,
                        (locate_flow(&local.location, variable.flow), variable.cost),
                    );
                    origins.insert(
                        id,
                        VarOrigin::Local {
                            mechanic: entry.id,
                            location: local.location.clone(),
                            cost: variable.cost,
                        },
                    );
                }
            }
            LocatedMechanic::Transfer(transfer) => {
                let Location::Planet(_) = &transfer.location else {
                    return Err("火箭转移的起点必须是星球".to_string());
                };
                let weight = store
                    .item_weight(&transfer.item.id)
                    .map(|weight| weight.weight)
                    .unwrap_or(store.weight_constants().default_item_weight);
                if weight <= 0.0 {
                    return Err(format!(
                        "火箭转移物品 {} 的重量必须大于 0",
                        transfer.item.id
                    ));
                }
                let id = UniverseVarId(next_id);
                next_id += 1;
                let mut located = SolverFlow::default();
                located.insert(
                    LocatedDualVar::new(
                        transfer.location.clone(),
                        DualVar::Item(transfer.item.clone()),
                    ),
                    -1.0,
                );
                located.insert(
                    LocatedDualVar::new(transfer.location.clone(), DualVar::RocketWeightCapacity),
                    -weight,
                );
                located.insert(
                    LocatedDualVar::new(Location::Space, DualVar::Item(transfer.item.clone())),
                    1.0,
                );
                flows.insert(id, (located, 0.0));
                origins.insert(
                    id,
                    VarOrigin::Transfer {
                        mechanic: entry.id,
                        location: transfer.location.clone(),
                    },
                );
            }
        }
    }

    // Implicit sources: each planet's own resources and all space chunks.
    let locations = used_locations(universe);
    let mut sources: SolverFlow<LocatedDualVar> = SolverFlow::default();
    for location in &locations {
        let source_flow = match location {
            Location::Planet(name) => planet_source_flows(store, name),
            Location::Space => space_source_flows(store),
        };
        for (flow, penalty) in locate_flow(location, source_flow) {
            sources.insert(flow, penalty);
        }
    }
    let mut target: SolverFlow<LocatedDualVar> = SolverFlow::default();
    for target_entry in &universe.targets {
        *target.entry(target_entry.flow.clone()).or_insert(0.0) += target_entry.amount;
    }

    // Space items may become the same item at any planet for free. Include
    // implicit asteroid sources and space targets, not only mechanic flows.
    let planet_locations: Vec<Location> = locations
        .iter()
        .filter(|location| matches!(location, Location::Planet(_)))
        .cloned()
        .collect();
    let mut space_items: AIndexMap<IdWithQuality, ()> = AIndexMap::default();
    let mut collect_space_item = |located: &LocatedDualVar| {
        if located.location == Location::Space
            && let DualVar::Item(item) = &located.flow
        {
            space_items.insert(item.clone(), ());
        }
    };
    for (_, (flow, _)) in &flows {
        for located in flow.keys() {
            collect_space_item(located);
        }
    }
    for located in sources.keys() {
        collect_space_item(located);
    }
    for located in target.keys() {
        collect_space_item(located);
    }
    for item in space_items.into_keys() {
        for planet in &planet_locations {
            let id = UniverseVarId(next_id);
            next_id += 1;
            let mut located = SolverFlow::default();
            located.insert(
                LocatedDualVar::new(Location::Space, DualVar::Item(item.clone())),
                -1.0,
            );
            located.insert(
                LocatedDualVar::new(planet.clone(), DualVar::Item(item.clone())),
                1.0,
            );
            flows.insert(id, (located, 0.0));
            origins.insert(id, VarOrigin::Conversion);
        }
    }

    // Temperature/fuel/heat subtype conversions are location-local; a global
    // set would create illegal cross-location shortcuts.
    add_located_conversion_flows(
        &mut flows,
        &locations,
        &target,
        &sources,
        store,
        &mut next_id,
    );

    let mut unconstrained: metatorio_solver::AIndexSet<LocatedDualVar> =
        metatorio_solver::AIndexSet::default();
    for located in flows
        .values()
        .flat_map(|(flow, _)| flow.keys())
        .chain(sources.keys())
        .chain(target.keys())
    {
        if matches!(located.flow, DualVar::Pollution { .. }) {
            unconstrained.insert(located.clone());
        }
    }

    let mut problem = SolverData::new_simple(target, flows);
    problem.sources = sources;
    problem.strict_source = universe.strict_source;
    problem.strict_sink = universe.strict_sink;
    problem.unconstrained = unconstrained;
    problem.solver = project.settings.solver;
    let flow_specs = problem.flows.clone();
    let solution = problem.solve();

    let (prim, report, cost) = match solution {
        SolverSolution::Solved {
            prim, report, cost, ..
        } => (prim, report, cost),
        SolverSolution::NotSolved {
            no_provider,
            no_consumer,
            description,
        } => {
            return Ok(UniverseSolveResult {
                project: project.id,
                universe: universe.id,
                status: UniverseSolveStatus::NotSolved {
                    description,
                    no_provider,
                    no_consumer,
                },
                report: SolveDiagnostics::default(),
            });
        }
    };

    let mut flow_totals: SolverFlow<LocatedDualVar> = SolverFlow::default();
    let mut mechanic_amounts: HashMap<MechanicId, (Location, f64, f64)> = HashMap::new();
    for (id, value) in &prim {
        let Some(spec) = flow_specs.get(id) else {
            continue;
        };
        for (flow, coefficient) in &spec.coefficients {
            *flow_totals.entry((*flow).clone()).or_insert(0.0) += *coefficient * value;
        }
        if let Some(origin) = origins.get(id) {
            match origin {
                VarOrigin::Local {
                    mechanic,
                    location,
                    cost,
                } => {
                    let entry = mechanic_amounts
                        .entry(*mechanic)
                        .or_insert_with(|| (location.clone(), 0.0, *cost));
                    entry.1 += value;
                }
                VarOrigin::Transfer { mechanic, location } => {
                    let entry = mechanic_amounts
                        .entry(*mechanic)
                        .or_insert_with(|| (location.clone(), 0.0, 0.0));
                    entry.1 += value;
                }
                VarOrigin::Conversion => {}
            }
        }
    }

    let mut diagnostics = SolveDiagnostics::default();
    diagnostics.variables_before = flow_specs.len();
    diagnostics.variables_after = prim.values().filter(|value| **value != 0.0).count();
    diagnostics.solver = report.solver;
    diagnostics.optimization = report.optimization.clone();
    diagnostics.exact_primal_verified = report.exact_primal_verified;
    diagnostics.exact_optimality_verified = report.exact_optimality_verified;

    Ok(UniverseSolveResult {
        project: project.id,
        universe: universe.id,
        status: UniverseSolveStatus::Solved {
            cost,
            flows: flow_totals
                .into_iter()
                .map(|(flow, amount)| LocatedFlow {
                    flow,
                    amount,
                    rate: amount,
                })
                .collect(),
            mechanics: mechanic_amounts
                .into_iter()
                .map(
                    |(mechanic, (location, amount, cost))| LocatedMechanicSolution {
                        mechanic,
                        location,
                        amount,
                        rate: amount,
                        cost,
                    },
                )
                .collect(),
        },
        report: diagnostics,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{LocatedTarget, ProjectSettings, UniverseMechanicEntry};
    use metatorio_core::{LocalLocatedMechanic, LocatedMechanic, Mechanic, RecipeMechanic};
    use serde_json::json;

    fn item(name: &str) -> IdWithQuality {
        IdWithQuality::new(name, "normal")
    }

    #[test]
    fn local_recipe_and_space_to_planet_conversion_solve() {
        let store = PrototypeStore::load(&json!({
            "item": {
                "iron-ore": { "type": "item", "name": "iron-ore", "stack_size": 50 }
            },
            "recipe": {
                "free-ore": {
                    "type": "recipe", "name": "free-ore",
                    "energy_required": 1,
                    "ingredients": [],
                    "results": [{ "type": "item", "name": "iron-ore", "amount": 1 }]
                }
            },
            "assembling-machine": {
                "assembling-machine-1": {
                    "type": "assembling-machine", "name": "assembling-machine-1",
                    "crafting_categories": ["crafting"], "crafting_speed": 1,
                    "energy_usage": "90kW",
                    "energy_source": { "type": "electric", "drain": "0J" }
                }
            }
        }))
        .expect("store");
        let project = ProjectDocument {
            id: ProjectId(1),
            settings: ProjectSettings::default(),
            ..ProjectDocument::default()
        };
        let mechanic = |location: Location| UniverseMechanicEntry {
            id: MechanicId(2),
            enabled: true,
            mechanic: LocatedMechanic::Local(LocalLocatedMechanic {
                location,
                mechanic: Mechanic::Recipe(RecipeMechanic {
                    recipe: item("free-ore"),
                    machine: item("assembling-machine-1"),
                    module_config: Default::default(),
                    fuel: None,
                }),
            }),
        };
        let universe = UniverseDocument {
            id: UniverseId(3),
            name: "test".into(),
            mechanics: vec![mechanic(Location::Space)],
            targets: vec![LocatedTarget {
                id: crate::id::TargetId(4),
                flow: LocatedDualVar::new(
                    Location::planet("nauvis"),
                    DualVar::Item(item("iron-ore")),
                ),
                amount: 1.0,
            }],
            strict_source: false,
            strict_sink: false,
        };
        let result = solve_universe(&store, &project, &universe, &Default::default()).unwrap();
        let UniverseSolveStatus::Solved { cost, flows, .. } = result.status else {
            panic!("universe should solve: {result:?}");
        };
        assert!(cost >= 0.0);
        assert!(flows.iter().any(|flow| {
            flow.flow.location == Location::planet("nauvis")
                && flow.flow.flow == DualVar::Item(item("iron-ore"))
                && flow.amount > 0.9
        }));
    }
    #[test]
    fn transfer_consumes_local_rocket_weight_capacity() {
        let store = PrototypeStore::load(&json!({
            "utility-constants": {"default": {"default_item_weight": 100, "default_rocket_lift_weight": 1200}},
            "item": {
                "iron-ore": { "type": "item", "name": "iron-ore", "stack_size": 50, "weight": 1000 }
            },
            "recipe": {
                "free-ore": {
                    "type": "recipe", "name": "free-ore",
                    "energy_required": 1,
                    "ingredients": [],
                    "results": [{ "type": "item", "name": "iron-ore", "amount": 1 }]
                }
            },
            "rocket-silo": {
                "silo": {
                    "type": "rocket-silo", "name": "silo",
                    "energy_usage": "1MW",
                    "energy_source": { "type": "electric", "drain": "0J" },
                    "crafting_speed": 1,
                    "crafting_categories": ["crafting"],
                    "launch_to_space_platforms": true,
                    "lift_weight": 1200,
                    "rocket_parts_required": 1
                }
            }
        }))
        .expect("store");
        let project = ProjectDocument {
            id: ProjectId(1),
            settings: ProjectSettings::default(),
            ..ProjectDocument::default()
        };
        let universe = UniverseDocument {
            id: UniverseId(3),
            name: "transfer".into(),
            mechanics: vec![
                UniverseMechanicEntry {
                    id: MechanicId(2),
                    enabled: true,
                    mechanic: LocatedMechanic::Local(LocalLocatedMechanic {
                        location: Location::planet("nauvis"),
                        mechanic: Mechanic::Recipe(RecipeMechanic {
                            recipe: item("free-ore"),
                            machine: item("silo"),
                            module_config: Default::default(),
                            fuel: None,
                        }),
                    }),
                },
                UniverseMechanicEntry {
                    id: MechanicId(4),
                    enabled: true,
                    mechanic: LocatedMechanic::Transfer(metatorio_core::TransferLocatedMechanic {
                        location: Location::planet("nauvis"),
                        item: item("iron-ore"),
                    }),
                },
            ],
            targets: vec![LocatedTarget {
                id: crate::id::TargetId(5),
                flow: LocatedDualVar::new(Location::Space, DualVar::Item(item("iron-ore"))),
                amount: 1.0,
            }],
            strict_source: false,
            strict_sink: false,
        };
        let result = solve_universe(&store, &project, &universe, &Default::default()).unwrap();
        let UniverseSolveStatus::Solved { cost, flows, .. } = result.status else {
            panic!("transfer universe should solve: {result:?}");
        };
        assert!(cost >= 0.0);
        assert!(flows.iter().any(|flow| {
            flow.flow.location == Location::Space
                && flow.flow.flow == DualVar::Item(item("iron-ore"))
                && flow.amount > 0.9
        }));
    }
    #[test]
    fn space_implicit_source_reaches_a_planet_target() {
        let store = PrototypeStore::load(&json!({
            "item": {
                "iron-chunk": { "type": "item", "name": "iron-chunk", "stack_size": 50 }
            },
            "asteroid-chunk": {
                "direct": { "minable": { "mining_time": 1, "result": "iron-chunk" } }
            }
        }))
        .expect("store");
        let project = ProjectDocument {
            id: ProjectId(1),
            settings: ProjectSettings::default(),
            ..ProjectDocument::default()
        };
        let universe = UniverseDocument {
            id: UniverseId(3),
            name: "source".into(),
            mechanics: Vec::new(),
            targets: vec![LocatedTarget {
                id: crate::id::TargetId(5),
                flow: LocatedDualVar::new(
                    Location::planet("nauvis"),
                    DualVar::Item(item("iron-chunk")),
                ),
                amount: 1.0,
            }],
            strict_source: false,
            strict_sink: false,
        };
        let result = solve_universe(&store, &project, &universe, &Default::default()).unwrap();
        assert!(
            matches!(result.status, UniverseSolveStatus::Solved { .. }),
            "{result:?}"
        );
    }
}

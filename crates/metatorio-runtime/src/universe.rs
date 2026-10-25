//! Universe-factory expansion and LP solve.
//!
//! Locations are part of the LP coordinates: a `LocatedDualVar` is balanced
//! independently at each planet/space location. Local mechanics are expanded
//! with their own environment. Rocket exports consume local items and rocket
//! weight capacity. Orbital drops consume Space items and deliver them to one
//! planet. Project-level estimates price exports per kg and drops per stack.

use std::collections::{BTreeSet, HashMap};

use metatorio_core::{
    Context, DualVar, Flow, IdWithQuality, LocatedDualVar, LocatedMechanic, Location, Mechanic,
};
use metatorio_data::store::PrototypeGroup;
use metatorio_data::store::PrototypeStore;

use metatorio_solver::{Flow as SolverFlow, SolverData, SolverSolution, TargetSpec};
use serde::{Deserialize, Serialize};

use crate::document::{ProjectDocument, UniverseDocument};
use crate::id::{MechanicId, ProjectId, UniverseId};
use crate::solve::{
    SolveDiagnostics, apply_environment_to_game_state, make_game_state_with_accessibility,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UniverseVarId(pub u64);
fn identity_scale() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocatedMechanicSolution {
    pub mechanic: MechanicId,
    pub location: Location,
    pub amount: f64,
    pub rate: f64,
    #[serde(default = "identity_scale")]
    pub scale: f64,
    pub cost: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocatedFlow {
    pub flow: LocatedDualVar,
    pub amount: f64,
    pub rate: f64,
    #[serde(default = "identity_scale")]
    pub scale: f64,
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
    #[serde(default)]
    pub revision: u64,
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
    Transport {
        mechanic: MechanicId,
        location: Location,
        cost: f64,
    },
}

pub(crate) fn environment_for(location: &Location) -> (Option<&str>, Option<&str>) {
    match location {
        Location::Planet(name) => (Some(name.as_str()), None),
        Location::Space => (None, Some("space-platform")),
    }
}

pub(crate) fn locate_flow(location: &Location, flow: Flow) -> SolverFlow<LocatedDualVar> {
    flow.into_iter()
        .map(|(flow, amount)| (LocatedDualVar::new(location.clone(), flow), amount))
        .collect()
}

/// All asteroid-chunk items are available in space. This intentionally unions
/// every chunk prototype instead of relying on one space-location name: a
/// universe factory has one abstract Space location.
pub(crate) fn space_source_flows(store: &PrototypeStore) -> Flow {
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

pub(crate) fn planet_source_flows(store: &PrototypeStore, planet: &str) -> Flow {
    crate::planet::implicit_source_flows(store, planet, None)
}

/// Return all locations mentioned by mechanics and targets.
pub(crate) fn used_locations(universe: &UniverseDocument) -> BTreeSet<Location> {
    let mut locations = BTreeSet::new();
    // Space is always available: its asteroid chunks are global implicit
    // sources and can feed planet targets even without an explicit space
    // mechanic.
    locations.insert(Location::Space);
    for entry in &universe.mechanics {
        locations.insert(entry.mechanic.location().clone());
    }
    for input in &universe.external_inputs {
        locations.insert(input.flow.location.clone());
    }
    for target in &universe.targets {
        locations.insert(target.flow.location.clone());
    }
    for expression in &universe.target_expressions {
        for term in &expression.terms {
            locations.insert(term.flow.location.clone());
        }
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

/// Read-only view data uses exactly the same per-mechanism expansion as the LP.
pub fn expand_located_mechanic(
    store: &PrototypeStore,
    base: &metatorio_core::GameState,
    mechanic: &LocatedMechanic,
) -> Result<Vec<(SolverFlow<LocatedDualVar>, f64)>, String> {
    expand_located_mechanic_with_costs(store, base, mechanic, &Default::default())
}

pub fn expand_located_mechanic_with_costs(
    store: &PrototypeStore,
    base: &metatorio_core::GameState,
    mechanic: &LocatedMechanic,
    costs: &crate::document::OrbitalTransportCosts,
) -> Result<Vec<(SolverFlow<LocatedDualVar>, f64)>, String> {
    costs.validate()?;
    match mechanic {
        LocatedMechanic::Local(local) => {
            let (planet, surface) = environment_for(&local.location);
            let mut game = base.clone();
            apply_environment_to_game_state(store, &mut game, planet, surface);
            let context = Context::new(store, &game);
            Ok(
                metatorio_core::expand::expand([(0usize, &local.mechanic)], &context)
                    .variables
                    .into_iter()
                    .map(|v| (locate_flow(&local.location, v.flow), v.cost))
                    .collect(),
            )
        }
        LocatedMechanic::RocketExport(transfer) => {
            if !matches!(transfer.location, Location::Planet(_)) {
                return Err("火箭发运的起点必须是星球".into());
            }
            let weight = store
                .item_weight(&transfer.item.id)
                .map(|w| w.weight)
                .unwrap_or(store.weight_constants().default_item_weight);
            if weight <= 0.0 {
                return Err(format!(
                    "火箭发运物品 {} 的重量必须大于 0",
                    transfer.item.id
                ));
            }
            let mut flow = SolverFlow::default();
            flow.insert(
                LocatedDualVar::new(
                    transfer.location.clone(),
                    DualVar::Item(transfer.item.clone()),
                ),
                -1.0,
            );
            flow.insert(
                LocatedDualVar::new(transfer.location.clone(), DualVar::RocketWeightCapacity),
                -weight,
            );
            flow.insert(
                LocatedDualVar::new(Location::Space, DualVar::Item(transfer.item.clone())),
                1.0,
            );
            Ok(vec![(flow, weight / 1000.0 * costs.rocket_export_per_kg)])
        }
        LocatedMechanic::OrbitalDrop(cargo) => {
            if !matches!(cargo.location, Location::Planet(_)) {
                return Err("轨道投送的终点必须是星球".into());
            }
            let item = store
                .get(PrototypeGroup::Item, &cargo.item.id)
                .and_then(|record| record.component::<metatorio_data::ItemComponent>())
                .filter(|item| item.stack_size > 0)
                .ok_or_else(|| format!("轨道投送物品 {} 缺少有效堆叠数量", cargo.item.id))?;
            let spoil_penalty = if let Some(ticks) = item.spoil_ticks.filter(|ticks| *ticks > 0) {
                let multiplier = store
                    .get(PrototypeGroup::Quality, &cargo.item.quality)
                    .and_then(|record| record.component::<metatorio_data::QualityComponent>())
                    .map(|quality| quality.spoil_ticks_multiplier())
                    .unwrap_or(1.0);
                let duration = f64::from(ticks) * multiplier;
                if !duration.is_finite() || duration <= 0.0 {
                    return Err(format!(
                        "轨道投送物品 {} 的有效变质时间必须为有限正数",
                        cargo.item.id
                    ));
                }
                costs.orbital_drop_spoil_penalty / duration
            } else {
                0.0
            };
            let mut flow = SolverFlow::default();
            flow.insert(
                LocatedDualVar::new(Location::Space, DualVar::Item(cargo.item.clone())),
                -1.0,
            );
            flow.insert(
                LocatedDualVar::new(cargo.location.clone(), DualVar::Item(cargo.item.clone())),
                1.0,
            );
            Ok(vec![(
                flow,
                costs.orbital_drop_per_stack / f64::from(item.stack_size) + spoil_penalty,
            )])
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UniversePreviewEntry {
    pub id: MechanicId,
    pub flows: Vec<LocatedFlow>,
    pub error: Option<String>,
    pub solar_balance: Option<metatorio_core::SolarBalance>,
}
#[derive(Debug, Serialize)]
pub struct UniversePreview {
    pub mechanics: Vec<UniversePreviewEntry>,
    pub sources: Vec<LocatedDualVar>,
}
fn located_solar_balance(
    store: &PrototypeStore,
    base: &metatorio_core::GameState,
    mechanic: &LocatedMechanic,
) -> Option<metatorio_core::SolarBalance> {
    let LocatedMechanic::Local(local) = mechanic else {
        return None;
    };
    let Mechanic::Solar(solar) = &local.mechanic else {
        return None;
    };
    let (planet, surface) = environment_for(&local.location);
    let mut game = base.clone();
    apply_environment_to_game_state(store, &mut game, planet, surface);
    metatorio_core::solar_balance(&Context::new(store, &game), solar)
}

pub fn preview_universe(
    store: &PrototypeStore,
    project: &ProjectDocument,
    universe: &UniverseDocument,
    accessibility: &metatorio_core::Accessibility,
) -> UniversePreview {
    let base = make_game_state_with_accessibility(store, project, accessibility);
    let mechanics = universe
        .mechanics
        .iter()
        .map(|entry| {
            match expand_located_mechanic_with_costs(
                store,
                &base,
                &entry.mechanic,
                &project.settings.orbital_transport_costs,
            ) {
                Ok(variants) => {
                    let mut total = SolverFlow::default();
                    for (flow, _) in variants {
                        for (key, amount) in flow {
                            *total.entry(key).or_insert(0.0) += amount;
                        }
                    }
                    UniversePreviewEntry {
                        id: entry.id,
                        error: None,
                        solar_balance: located_solar_balance(store, &base, &entry.mechanic),
                        flows: total
                            .into_iter()
                            .map(|(flow, amount)| LocatedFlow {
                                flow,
                                amount,
                                rate: amount,
                                scale: 1.0,
                            })
                            .collect(),
                    }
                }
                Err(error) => UniversePreviewEntry {
                    id: entry.id,
                    flows: vec![],
                    error: Some(error),
                    solar_balance: None,
                },
            }
        })
        .collect();
    let sources = used_locations(universe)
        .iter()
        .flat_map(|location| {
            let flow = match location {
                Location::Planet(name) => planet_source_flows(store, name),
                Location::Space => space_source_flows(store),
            };
            locate_flow(location, flow).into_keys()
        })
        .collect();
    UniversePreview { mechanics, sources }
}

/// Build and solve a universe factory with the project's selected backend.
pub fn solve_universe(
    store: &PrototypeStore,
    project: &ProjectDocument,
    universe: &UniverseDocument,
    accessibility: &metatorio_core::Accessibility,
) -> Result<UniverseSolveResult, String> {
    solve_universe_with_progress(store, project, universe, accessibility, |_| {})
}
pub fn solve_universe_with_progress(
    store: &PrototypeStore,
    project: &ProjectDocument,
    universe: &UniverseDocument,
    accessibility: &metatorio_core::Accessibility,
    progress: impl FnMut(metatorio_solver::ProblemSize),
) -> Result<UniverseSolveResult, String> {
    crate::solve::check_solve_cancelled().map_err(|e| e.to_string())?;
    let base = make_game_state_with_accessibility(store, project, accessibility);
    let mut flows: metatorio_core::AIndexMap<UniverseVarId, (SolverFlow<LocatedDualVar>, f64)> =
        metatorio_core::AIndexMap::default();
    let mut origins = HashMap::new();
    let mut next_id = 0u64;

    for entry in &universe.mechanics {
        if !entry.enabled {
            continue;
        }
        for (flow, cost) in expand_located_mechanic_with_costs(
            store,
            &base,
            &entry.mechanic,
            &project.settings.orbital_transport_costs,
        )? {
            let id = UniverseVarId(next_id);
            next_id += 1;
            flows.insert(id, (flow, cost));
            let origin = match &entry.mechanic {
                LocatedMechanic::Local(local) => VarOrigin::Local {
                    mechanic: entry.id,
                    location: local.location.clone(),
                    cost,
                },
                LocatedMechanic::RocketExport(transfer)
                | LocatedMechanic::OrbitalDrop(transfer) => VarOrigin::Transport {
                    mechanic: entry.id,
                    location: transfer.location.clone(),
                    cost,
                },
            };
            origins.insert(id, origin);
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
    // Explicit inputs override the same located implicit source.
    for input in &universe.external_inputs {
        if !input.penalty.is_finite() || input.penalty < 0.0 {
            return Err("外部输入成本必须为非负有限数".into());
        }
        sources.insert(input.flow.clone(), input.penalty);
    }
    let mut target: SolverFlow<LocatedDualVar> = SolverFlow::default();
    for target_entry in &universe.targets {
        *target.entry(target_entry.flow.clone()).or_insert(0.0) += target_entry.amount;
    }
    let mut expression_flows: SolverFlow<LocatedDualVar> = SolverFlow::default();
    for expression in &universe.target_expressions {
        for term in &expression.terms {
            expression_flows.insert(term.flow.clone(), 1.0);
        }
    }
    let conversion_target =
        crate::solve::conversion_targets(&target, expression_flows.keys().cloned());

    // Temperature/fuel/heat subtype conversions are location-local; a global
    // set would create illegal cross-location shortcuts.
    add_located_conversion_flows(
        &mut flows,
        &locations,
        &conversion_target,
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
        .chain(expression_flows.keys())
    {
        if matches!(located.flow, DualVar::Pollution { .. }) {
            unconstrained.insert(located.clone());
        }
    }

    let expression_targets: Vec<TargetSpec<LocatedDualVar>> = universe
        .target_expressions
        .iter()
        .map(|expression| TargetSpec {
            constant: expression.constant,
            coefficients: expression
                .terms
                .iter()
                .map(|term| (term.flow.clone(), term.coefficient))
                .collect(),
        })
        .collect();
    let mut problem = SolverData::new_simple(target, flows);
    problem.target.extend(expression_targets);
    problem.sources = sources;
    problem.strict_source = universe.strict_source;
    problem.strict_sink = universe.strict_sink;
    problem.unconstrained = unconstrained;
    problem.solver = project.settings.solver;
    let flow_specs = problem.flows.clone();
    crate::solve::check_solve_cancelled().map_err(|e| e.to_string())?;
    let solution = problem.solve_with_progress(progress);
    crate::solve::check_solve_cancelled().map_err(|e| e.to_string())?;

    let (prim, prim_scale, dual_scale, report, cost) = match solution {
        SolverSolution::Solved {
            prim,
            prim_scale,
            dual_scale,
            report,
            cost,
            ..
        } => (prim, prim_scale, dual_scale, report, cost),
        SolverSolution::NotSolved {
            no_provider,
            no_consumer,
            description,
        } => {
            return Ok(UniverseSolveResult {
                revision: 0,
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
    let mut mechanic_amounts: HashMap<MechanicId, (Location, f64, f64, f64)> = HashMap::new();
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
                        .or_insert_with(|| (location.clone(), 0.0, 0.0, *cost));
                    entry.1 += value;
                    entry.2 += value / prim_scale.get(id).copied().unwrap_or(1.0).max(1e-12);
                }
                VarOrigin::Transport {
                    mechanic,
                    location,
                    cost,
                } => {
                    let entry = mechanic_amounts
                        .entry(*mechanic)
                        .or_insert_with(|| (location.clone(), 0.0, 0.0, *cost));
                    entry.1 += value;
                    entry.2 += value / prim_scale.get(id).copied().unwrap_or(1.0).max(1e-12);
                }
            }
        }
    }

    let diagnostics = SolveDiagnostics::from_solver(report);

    Ok(UniverseSolveResult {
        revision: 0,
        project: project.id,
        universe: universe.id,
        status: UniverseSolveStatus::Solved {
            cost,
            flows: flow_totals
                .into_iter()
                .map(|(flow, amount)| {
                    let scale = dual_scale.get(&flow).copied().unwrap_or(1.0);
                    LocatedFlow {
                        flow,
                        amount,
                        rate: amount / scale.max(1e-12),
                        scale,
                    }
                })
                .collect(),
            mechanics: mechanic_amounts
                .into_iter()
                .map(
                    |(mechanic, (location, amount, rate, cost))| LocatedMechanicSolution {
                        mechanic,
                        location,
                        amount,
                        rate,
                        scale: if rate != 0.0 { amount / rate } else { 1.0 },
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
    fn local_recipe_and_explicit_orbital_drop_solve() {
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
            mechanics: vec![
                mechanic(Location::Space),
                UniverseMechanicEntry {
                    id: MechanicId(5),
                    enabled: true,
                    mechanic: LocatedMechanic::OrbitalDrop(metatorio_core::OrbitalCargoMechanic {
                        location: Location::planet("nauvis"),
                        item: item("iron-ore"),
                    }),
                },
            ],
            targets: vec![LocatedTarget {
                id: crate::id::TargetId(4),
                flow: LocatedDualVar::new(
                    Location::planet("nauvis"),
                    DualVar::Item(item("iron-ore")),
                ),
                amount: 1.0,
            }],
            target_expressions: vec![crate::document::LocatedTargetExpression {
                id: crate::id::TargetExpressionId(6),
                constant: 2.0,
                terms: vec![
                    crate::document::LocatedTargetTerm {
                        id: crate::id::TargetTermId(7),
                        flow: LocatedDualVar::new(
                            Location::planet("nauvis"),
                            DualVar::Item(item("iron-ore")),
                        ),
                        coefficient: 1.0,
                    },
                    crate::document::LocatedTargetTerm {
                        id: crate::id::TargetTermId(8),
                        flow: LocatedDualVar::new(Location::Space, DualVar::Item(item("iron-ore"))),
                        coefficient: 1.0,
                    },
                ],
            }],
            external_inputs: vec![],
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
                    mechanic: LocatedMechanic::RocketExport(metatorio_core::OrbitalCargoMechanic {
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
            target_expressions: Vec::new(),
            external_inputs: vec![],
            strict_source: false,
            strict_sink: false,
        };
        let result = solve_universe(&store, &project, &universe, &Default::default()).unwrap();
        let UniverseSolveStatus::Solved {
            cost,
            flows,
            mechanics,
        } = result.status
        else {
            panic!("transfer universe should solve: {result:?}");
        };
        assert!(cost >= 0.0);
        assert!(flows.iter().any(|flow| {
            flow.flow.location == Location::Space
                && flow.flow.flow == DualVar::Item(item("iron-ore"))
                && flow.amount > 0.9
        }));
        // The LP may choose surplus capacity among zero-cost optima.
        // Transfers must never overdraw the local capacity.
        let amount = |id| {
            mechanics
                .iter()
                .find(|m| m.mechanic == MechanicId(id))
                .map_or(0.0, |m| m.amount)
        };
        let expected_capacity = 1200.0 * amount(2) - 1000.0 * amount(4);
        assert!(
            flows.iter().any(|flow| {
                flow.flow.location == Location::planet("nauvis")
                    && flow.flow.flow == DualVar::RocketWeightCapacity
                    && flow.amount >= -0.01
                    && (flow.amount - expected_capacity).abs() < 0.01
            }),
            "{flows:?}"
        );
    }
    #[test]
    fn space_implicit_source_requires_explicit_drop_to_reach_planet() {
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
        let mut universe = UniverseDocument {
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
            target_expressions: Vec::new(),
            external_inputs: vec![],
            strict_source: true,
            strict_sink: false,
        };
        let result = solve_universe(&store, &project, &universe, &Default::default()).unwrap();
        assert!(matches!(
            result.status,
            UniverseSolveStatus::NotSolved { .. }
        ));
        universe.mechanics.push(UniverseMechanicEntry {
            id: MechanicId(6),
            enabled: true,
            mechanic: LocatedMechanic::OrbitalDrop(metatorio_core::OrbitalCargoMechanic {
                location: Location::planet("nauvis"),
                item: item("iron-chunk"),
            }),
        });
        let result = solve_universe(&store, &project, &universe, &Default::default()).unwrap();
        assert!(
            matches!(result.status, UniverseSolveStatus::Solved { .. }),
            "{result:?}"
        );
    }

    #[test]
    fn vanilla_factory_auto_plan_iron_plate() {
        use crate::document::{
            AppDocument, DOCUMENT_SCHEMA_VERSION, FactoryDocument, FactorySettings, FlowTarget,
            ProjectDocument, ProjectSettings,
        };
        use crate::id::FactoryId;
        use crate::id::ProjectId;
        use crate::solve::Runtime;

        let dump: serde_json::Value =
            serde_json::from_str(include_str!("../../../assets/data-raw-dump.json")).unwrap();
        let store = PrototypeStore::load(&dump).expect("vanilla store");
        let factory = FactoryDocument {
            id: FactoryId(2),
            name: "iron".to_string(),
            settings: FactorySettings {
                planet: Some("nauvis".to_string()),
                ..FactorySettings::default()
            },
            targets: vec![FlowTarget {
                id: crate::id::TargetId(3),
                flow: DualVar::Item(item("iron-plate")),
                amount: 1.0,
            }],
            ..FactoryDocument::default()
        };
        let project = ProjectDocument {
            id: ProjectId(1),
            name: "vanilla".to_string(),
            context_id: Some("vanilla".to_string()),
            settings: ProjectSettings {
                all_accessible: true,
                ..ProjectSettings::default()
            },
            factories: vec![factory],
            ..ProjectDocument::default()
        };
        let document = AppDocument {
            schema_version: DOCUMENT_SCHEMA_VERSION,
            projects: vec![project],
        };
        let mut runtime = Runtime::from_document(document);
        runtime.install_context("vanilla".to_string(), store);
        runtime.set_active_context(Some("vanilla".to_string()));
        runtime
            .auto_plan(ProjectId(1), FactoryId(2))
            .expect("factory auto plan");
        assert!(
            runtime
                .state
                .factory(ProjectId(1), FactoryId(2))
                .unwrap()
                .mechanics
                .iter()
                .any(|entry| format!("{:?}", entry.mechanic).contains("iron")),
            "factory auto plan produced no iron mechanics"
        );
    }

    #[test]
    #[ignore = "full global strict LP; Rust backend takes minutes, run with --release --ignored"]
    fn vanilla_universe_auto_plan_iron_plate() {
        use crate::document::{
            AppDocument, DOCUMENT_SCHEMA_VERSION, LocatedTarget, ProjectDocument, ProjectSettings,
            UniverseDocument,
        };
        use crate::id::ProjectId;
        use crate::solve::Runtime;

        let dump: serde_json::Value =
            serde_json::from_str(include_str!("../../../assets/data-raw-dump.json")).unwrap();
        let store = PrototypeStore::load(&dump).expect("vanilla store");
        let universe = UniverseDocument {
            id: UniverseId(2),
            name: "iron".to_string(),
            targets: vec![LocatedTarget {
                id: crate::id::TargetId(3),
                flow: LocatedDualVar::new(
                    Location::planet("nauvis"),
                    DualVar::Item(item("iron-plate")),
                ),
                amount: 1000.0,
            }],
            ..UniverseDocument::default()
        };
        let project = ProjectDocument {
            id: ProjectId(1),
            name: "vanilla".to_string(),
            context_id: Some("vanilla".to_string()),
            settings: ProjectSettings {
                all_accessible: true,
                solver: metatorio_solver::LpSolver::Highs,
                ..ProjectSettings::default()
            },
            universes: vec![universe],
            ..ProjectDocument::default()
        };
        let document = AppDocument {
            schema_version: DOCUMENT_SCHEMA_VERSION,
            projects: vec![project],
        };
        let mut runtime = Runtime::from_document(document);
        runtime.install_context("vanilla".to_string(), store);
        runtime.set_active_context(Some("vanilla".to_string()));
        let result = runtime
            .auto_plan_universe(ProjectId(1), UniverseId(2))
            .expect("universe auto plan");
        let UniverseSolveStatus::Solved { flows, .. } = result.status else {
            panic!("iron plate should be planned: {result:?}");
        };
        assert!(
            flows.iter().any(|flow| {
                flow.flow.location == Location::planet("nauvis")
                    && flow.flow.flow == DualVar::Item(item("iron-plate"))
                    && flow.amount > 0.9
            }),
            "{flows:?}"
        );
    }

    #[test]
    #[ignore = "full vanilla universe auto-plan (33k candidates); run with --release --ignored"]
    fn vanilla_auto_plan_finds_quantum_processor_without_target_location() {
        use crate::document::{
            AppDocument, DOCUMENT_SCHEMA_VERSION, LocatedTargetExpression, LocatedTargetTerm,
            ProjectDocument, ProjectSettings, UniverseDocument,
        };
        use crate::id::ProjectId;
        use crate::solve::Runtime;

        let dump: serde_json::Value =
            serde_json::from_str(include_str!("../../../assets/data-raw-dump.json")).unwrap();
        let store = PrototypeStore::load(&dump).expect("vanilla store");
        let quantum = item("quantum-processor");
        let universe = UniverseDocument {
            id: UniverseId(2),
            name: "vanilla quantum".to_string(),
            target_expressions: vec![LocatedTargetExpression {
                id: crate::id::TargetExpressionId(3),
                constant: 1.0,
                terms: vec![
                    LocatedTargetTerm {
                        id: crate::id::TargetTermId(4),
                        flow: LocatedDualVar::new(
                            Location::planet("aquilo"),
                            DualVar::Item(quantum.clone()),
                        ),
                        coefficient: 1.0,
                    },
                    LocatedTargetTerm {
                        id: crate::id::TargetTermId(5),
                        flow: LocatedDualVar::new(
                            Location::planet("nauvis"),
                            DualVar::Item(quantum.clone()),
                        ),
                        coefficient: 1.0,
                    },
                ],
            }],
            ..UniverseDocument::default()
        };
        let project = ProjectDocument {
            id: ProjectId(1),
            name: "vanilla".to_string(),
            context_id: Some("vanilla".to_string()),
            settings: ProjectSettings {
                all_accessible: true,
                solver: metatorio_solver::LpSolver::Highs,
                ..ProjectSettings::default()
            },
            universes: vec![universe],
            ..ProjectDocument::default()
        };
        let document = AppDocument {
            schema_version: DOCUMENT_SCHEMA_VERSION,
            projects: vec![project],
        };
        let mut runtime = Runtime::from_document(document);
        runtime.install_context("vanilla".to_string(), store);
        runtime.set_active_context(Some("vanilla".to_string()));
        let result = runtime
            .auto_plan_universe(ProjectId(1), UniverseId(2))
            .expect("universe auto plan");
        let UniverseSolveStatus::Solved { flows, .. } = result.status else {
            panic!("quantum processor should be planned: {result:?}");
        };
        assert!(
            flows.iter().any(|flow| {
                flow.flow.location == Location::planet("aquilo")
                    && flow.flow.flow == DualVar::Item(quantum.clone())
                    && flow.amount > 0.9
            }),
            "{flows:?}"
        );
    }
}

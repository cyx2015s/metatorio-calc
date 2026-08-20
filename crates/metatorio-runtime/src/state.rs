use std::{
    collections::{BTreeSet, HashMap},
    fmt,
};

use metatorio_core::{BeaconConfig, DualVar, Mechanic, ModuleConfig};
use serde::Serialize;

use crate::document::{
    AppDocument, AutoBeaconPlan, ExternalInput, FactoryDocument, FlowTarget, MechanicEntry,
    MechanicKind, PlanningPreferences, ProjectDocument, TargetExpression, TargetTerm,
};
use crate::id::{
    ExternalInputId, FactoryId, MechanicId, ProjectId, TargetExpressionId, TargetId, TargetTermId,
};
use crate::message::{
    AppMessage, ApplicationAction, BoilerMechanicAction, CloseDecision, DeleteDecision,
    ExternalInputAction, FactoryAction, FactoryContextAction, FactoryTemplate, FlowAction,
    FluidFuelMechanicAction, FluidHeatMechanicAction, GeneratorMechanicAction,
    ItemFuelMechanicAction, ItemLaunchMechanicAction, MechanicAction, MechanicListAction,
    MiningMechanicAction, ModuleAction, PlantMechanicAction, PlanningAction, ProjectAction,
    ProjectPage, ReactorMechanicAction, RecipeMechanicAction, RuntimeCommand, SelectorKind,
    SelectorTarget, SelectorValue, SolveAction, SolarMechanicAction, SpoilMechanicAction,
    SuggestionAction, SuggestionCandidate, TargetAction, TargetExpressionAction, UiAction,
};

/// Mutable application state that is independent from any GUI framework.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeState {
    pub document: AppDocument,
    pub ui: UiState,
    pub revision: u64,
    pub dirty_projects: BTreeSet<ProjectId>,
    next_id: u64,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self::new(AppDocument::default())
    }
}

impl RuntimeState {
    pub fn new(document: AppDocument) -> Self {
        let mut state = Self {
            document,
            ui: UiState::default(),
            revision: 0,
            dirty_projects: BTreeSet::new(),
            next_id: 1,
        };
        state.refresh_next_id();
        state.select_first_project();
        state
    }

    /// Accept one user message and return side effects for the outer runtime.
    ///
    /// All application-owned enums are matched explicitly.  Adding a new
    /// message variant therefore forces this dispatcher to be updated.
    pub fn dispatch(&mut self, message: AppMessage) -> Result<DispatchResult, RuntimeError> {
        let outcome = match message {
            AppMessage::Application(action) => self.apply_application(action)?,
            AppMessage::Project { project, action } => self.apply_project(project, action)?,
            AppMessage::Factory {
                project,
                factory,
                action,
            } => self.apply_factory(project, factory, action)?,
            AppMessage::Ui(action) => self.apply_ui(action)?,
        };

        self.finish(outcome)
    }

    pub fn project(&self, id: ProjectId) -> Result<&ProjectDocument, RuntimeError> {
        self.document
            .projects
            .iter()
            .find(|project| project.id == id)
            .ok_or(RuntimeError::ProjectNotFound(id))
    }

    pub fn factory(
        &self,
        project: ProjectId,
        factory: FactoryId,
    ) -> Result<&FactoryDocument, RuntimeError> {
        self.project(project)?
            .factories
            .iter()
            .find(|candidate| candidate.id == factory)
            .ok_or(RuntimeError::FactoryNotFound { project, factory })
    }

    fn finish(&mut self, mut outcome: Outcome) -> Result<DispatchResult, RuntimeError> {
        if outcome.changed {
            self.revision = self.revision.wrapping_add(1);
            if let Some(project) = outcome.project {
                self.dirty_projects.insert(project);
                outcome.commands.push(RuntimeCommand::Persist {
                    project,
                    path: None,
                });
                outcome
                    .commands
                    .push(RuntimeCommand::EnsureQualityLimit { project });

                if outcome.recompute_all {
                    for factory in &self.project(project)?.factories {
                        outcome.commands.push(RuntimeCommand::Recompute {
                            project,
                            factory: factory.id,
                        });
                    }
                } else if let Some(factory) = outcome.factory {
                    outcome
                        .commands
                        .push(RuntimeCommand::Recompute { project, factory });
                }
            }
        }

        Ok(DispatchResult {
            revision: self.revision,
            changed: outcome.changed,
            commands: outcome.commands,
        })
    }

    fn apply_application(&mut self, action: ApplicationAction) -> Result<Outcome, RuntimeError> {
        match action {
            ApplicationAction::NewProject { name } => {
                let id = self.allocate_id();
                let project = ProjectDocument {
                    id,
                    name: non_empty(name, "Unnamed project"),
                    ..ProjectDocument::default()
                };
                self.document.projects.push(project);
                self.ui.selected_project = Some(id);
                self.ui.selected_factory = None;
                Ok(Outcome::changed(id))
            }
            ApplicationAction::OpenProject { path } => {
                Ok(Outcome::command(RuntimeCommand::LoadProject { path }))
            }
            ApplicationAction::SaveProject { project } => {
                self.project(project)?;
                Ok(Outcome::command(RuntimeCommand::Persist {
                    project,
                    path: None,
                }))
            }
            ApplicationAction::SaveProjectAs { project, path } => {
                self.project(project)?;
                Ok(Outcome::command(RuntimeCommand::Persist {
                    project,
                    path: Some(path),
                }))
            }
            ApplicationAction::CloseProject { project, decision } => {
                self.project(project)?;
                match decision {
                    CloseDecision::Cancel => Ok(Outcome::none()),
                    CloseDecision::Discard => {
                        self.remove_project(project)?;
                        Ok(Outcome::changed_without_project())
                    }
                    CloseDecision::Save => Ok(Outcome::commands(vec![
                        RuntimeCommand::Persist {
                            project,
                            path: None,
                        },
                        RuntimeCommand::CloseProject { project },
                    ])),
                }
            }
            ApplicationAction::DeleteProject { project, decision } => {
                if matches!(decision, DeleteDecision::Confirm) {
                    self.remove_project(project)?;
                    Ok(Outcome::changed_without_project())
                } else {
                    self.project(project)?;
                    Ok(Outcome::none())
                }
            }
            ApplicationAction::ReorderProject { project, position } => {
                let index = self
                    .document
                    .projects
                    .iter()
                    .position(|candidate| candidate.id == project)
                    .ok_or(RuntimeError::ProjectNotFound(project))?;
                let changed = move_item(&mut self.document.projects, index, position);
                Ok(Outcome::changed_if(changed, project))
            }
            ApplicationAction::LoadGameContext {
                executable_path,
                mod_path,
            } => Ok(Outcome::command(RuntimeCommand::LoadGameContext {
                executable_path,
                mod_path,
            })),
            ApplicationAction::LoadCachedContext => {
                Ok(Outcome::command(RuntimeCommand::LoadCachedContext))
            }
            ApplicationAction::CheckForUpdate => {
                Ok(Outcome::command(RuntimeCommand::CheckForUpdate))
            }
            ApplicationAction::InstallUpdate => Ok(Outcome::command(RuntimeCommand::InstallUpdate)),
            ApplicationAction::RestartAfterUpdate => {
                Ok(Outcome::command(RuntimeCommand::RestartAfterUpdate))
            }
        }
    }

    fn apply_project(
        &mut self,
        project_id: ProjectId,
        action: ProjectAction,
    ) -> Result<Outcome, RuntimeError> {
        self.project(project_id)?;

        match action {
            ProjectAction::SetName { name } => {
                let project = self.project_mut(project_id)?;
                let changed = replace(&mut project.name, non_empty(name, "Unnamed project"));
                Ok(Outcome::changed_if(changed, project_id))
            }
            ProjectAction::AddFactory { name, template } => {
                let factory_id = self.allocate_id();
                let factory = self.new_factory(factory_id, name, template);
                self.project_mut(project_id)?.factories.push(factory);
                self.ui.selected_project = Some(project_id);
                self.ui.selected_factory = Some(factory_id);
                Ok(Outcome::changed_factory(project_id, factory_id))
            }
            ProjectAction::CloneFactory { factory } => {
                let source = self
                    .project(project_id)?
                    .factories
                    .iter()
                    .find(|candidate| candidate.id == factory)
                    .cloned()
                    .ok_or(RuntimeError::FactoryNotFound {
                        project: project_id,
                        factory,
                    })?;
                let clone = self.clone_factory(source);
                let clone_id = clone.id;
                self.project_mut(project_id)?.factories.push(clone);
                self.ui.selected_project = Some(project_id);
                self.ui.selected_factory = Some(clone_id);
                Ok(Outcome::changed_factory(project_id, clone_id))
            }
            ProjectAction::RemoveFactory { factory } => {
                let new_selected_factory = {
                    let project = self.project_mut(project_id)?;
                    let index = project
                        .factories
                        .iter()
                        .position(|candidate| candidate.id == factory)
                        .ok_or(RuntimeError::FactoryNotFound {
                            project: project_id,
                            factory,
                        })?;
                    project.factories.remove(index);
                    project.factories.get(index.saturating_sub(1)).map(|f| f.id)
                };
                if self.ui.selected_factory == Some(factory) {
                    self.ui.selected_factory = new_selected_factory;
                }
                Ok(Outcome::changed(project_id))
            }
            ProjectAction::ReorderFactory { factory, position } => {
                let project = self.project_mut(project_id)?;
                let index = project
                    .factories
                    .iter()
                    .position(|candidate| candidate.id == factory)
                    .ok_or(RuntimeError::FactoryNotFound {
                        project: project_id,
                        factory,
                    })?;
                let changed = move_item(&mut project.factories, index, position);
                Ok(Outcome::changed_if(changed, project_id))
            }
            ProjectAction::SetTimeScale { time_scale } => {
                let changed = replace(
                    &mut self.project_mut(project_id)?.settings.time_scale,
                    time_scale,
                );
                Ok(Outcome::all_factories_if(changed, project_id))
            }
            ProjectAction::SetAllAccessible { enabled } => {
                let changed = replace(
                    &mut self.project_mut(project_id)?.settings.all_accessible,
                    enabled,
                );
                Ok(Outcome::all_factories_if(changed, project_id))
            }
            ProjectAction::AddMilestone { node, unlocked } => {
                let settings = &mut self.project_mut(project_id)?.settings;
                if let Some(existing) = settings
                    .milestones
                    .iter_mut()
                    .find(|candidate| candidate.node == node)
                {
                    let changed = replace(
                        existing,
                        crate::document::Milestone { node, unlocked },
                    );
                    Ok(Outcome::all_factories_if(changed, project_id))
                } else {
                    settings
                        .milestones
                        .push(crate::document::Milestone { node, unlocked });
                    Ok(Outcome::all_factories(project_id))
                }
            }
            ProjectAction::SetMilestoneUnlocked { node, unlocked } => {
                let settings = &mut self.project_mut(project_id)?.settings;
                let milestone = settings
                    .milestones
                    .iter_mut()
                    .find(|candidate| candidate.node == node)
                    .ok_or_else(|| {
                        RuntimeError::InvalidValue(format!("unknown milestone: {node:?}"))
                    })?;
                let changed = replace(&mut milestone.unlocked, unlocked);
                Ok(Outcome::all_factories_if(changed, project_id))
            }
            ProjectAction::RemoveMilestone { node } => {
                let milestones = &mut self.project_mut(project_id)?.settings.milestones;
                let before = milestones.len();
                milestones.retain(|candidate| candidate.node != node);
                Ok(Outcome::all_factories_if(
                    before != milestones.len(),
                    project_id,
                ))
            }
            ProjectAction::AddMarkedAccessible { node } => {
                let marks = &mut self.project_mut(project_id)?.settings.marked_accessible;
                let before = marks.len();
                if !marks.contains(&node) {
                    marks.push(node);
                }
                Ok(Outcome::all_factories_if(before != marks.len(), project_id))
            }
            ProjectAction::RemoveMarkedAccessible { node } => {
                let marks = &mut self.project_mut(project_id)?.settings.marked_accessible;
                let before = marks.len();
                marks.retain(|mark| mark != &node);
                Ok(Outcome::all_factories_if(before != marks.len(), project_id))
            }
            ProjectAction::AddMarkedInaccessible { node } => {
                let marks = &mut self.project_mut(project_id)?.settings.marked_inaccessible;
                let before = marks.len();
                if !marks.contains(&node) {
                    marks.push(node);
                }
                Ok(Outcome::all_factories_if(before != marks.len(), project_id))
            }
            ProjectAction::RemoveMarkedInaccessible { node } => {
                let marks = &mut self.project_mut(project_id)?.settings.marked_inaccessible;
                let before = marks.len();
                marks.retain(|mark| mark != &node);
                Ok(Outcome::all_factories_if(before != marks.len(), project_id))
            }
            ProjectAction::SetMiningProductivity { productivity } => {
                validate_non_negative("mining productivity", productivity)?;
                let changed = replace(
                    &mut self.project_mut(project_id)?.settings.mining_productivity,
                    productivity,
                );
                Ok(Outcome::all_factories_if(changed, project_id))
            }
            ProjectAction::SetIgnoreProductivity { ignore } => {
                let changed = replace(
                    &mut self.project_mut(project_id)?.settings.ignore_productivity,
                    ignore,
                );
                Ok(Outcome::all_factories_if(changed, project_id))
            }
            ProjectAction::SetRecipeProductivity { productivity } => {
                validate_non_negative("recipe productivity", productivity.productivity)?;
                let settings = &mut self.project_mut(project_id)?.settings;
                if let Some(existing) = settings
                    .recipe_productivity
                    .iter_mut()
                    .find(|candidate| candidate.recipe == productivity.recipe)
                {
                    let changed = replace(existing, productivity);
                    Ok(Outcome::all_factories_if(changed, project_id))
                } else {
                    settings.recipe_productivity.push(productivity);
                    Ok(Outcome::all_factories(project_id))
                }
            }
            ProjectAction::RemoveRecipeProductivity { recipe } => {
                let entries = &mut self.project_mut(project_id)?.settings.recipe_productivity;
                let before = entries.len();
                entries.retain(|entry| entry.recipe != recipe);
                Ok(Outcome::all_factories_if(
                    before != entries.len(),
                    project_id,
                ))
            }
            ProjectAction::SetQualityLimit { quality } => {
                let changed = replace(
                    &mut self.project_mut(project_id)?.settings.quality_limit,
                    quality,
                );
                Ok(Outcome::all_factories_if(changed, project_id))
            }
            ProjectAction::SetContext { context } => {
                let changed = replace(&mut self.project_mut(project_id)?.context_id, context);
                Ok(Outcome::all_factories_if(changed, project_id))
            }
            ProjectAction::Planning(action) => {
                // UseBestModules executes for the currently selected mechanic
                // even though the preference itself is project-global.
                let outcome = match action {
                    PlanningAction::UseBestModules => {
                        let factory = self.ui.selected_factory.ok_or(
                            RuntimeError::InvalidOperation("没有选中的工厂"),
                        )?;
                        let mechanic = self.ui.selected_mechanic.ok_or(
                            RuntimeError::InvalidOperation("没有选中的机制"),
                        )?;
                        Outcome::command(RuntimeCommand::UseBestModules {
                            project: project_id,
                            factory,
                            mechanic,
                        })
                    }
                    other => {
                        let changed = apply_planning_action(
                            &mut self.project_mut(project_id)?.planning,
                            other,
                        )?;
                        Outcome::all_factories_if(changed, project_id)
                    }
                };
                Ok(outcome)
            }
        }
    }

    fn apply_factory(
        &mut self,
        project_id: ProjectId,
        factory_id: FactoryId,
        action: FactoryAction,
    ) -> Result<Outcome, RuntimeError> {
        self.factory(project_id, factory_id)?;

        match action {
            FactoryAction::SetName { name } => {
                let changed = replace(
                    &mut self.factory_mut(project_id, factory_id)?.name,
                    non_empty(name, "Unnamed factory"),
                );
                Ok(Outcome::changed_factory_if(changed, project_id, factory_id))
            }
            FactoryAction::SetStrictSource { strict } => {
                let changed = replace(
                    &mut self.factory_mut(project_id, factory_id)?.strict_source,
                    strict,
                );
                Ok(Outcome::changed_factory_if(changed, project_id, factory_id))
            }
            FactoryAction::SetStrictSink { strict } => {
                let changed = replace(
                    &mut self.factory_mut(project_id, factory_id)?.strict_sink,
                    strict,
                );
                Ok(Outcome::changed_factory_if(changed, project_id, factory_id))
            }
            FactoryAction::Context(action) => {
                let changed = apply_factory_context(
                    &mut self.factory_mut(project_id, factory_id)?.settings,
                    action,
                )?;
                Ok(Outcome::changed_factory_if(changed, project_id, factory_id))
            }
            FactoryAction::Target(action) => self.apply_target(project_id, factory_id, action),
            FactoryAction::TargetExpression(action) => {
                self.apply_target_expression(project_id, factory_id, action)
            }
            FactoryAction::ExternalInput(action) => {
                self.apply_external_input(project_id, factory_id, action)
            }
            FactoryAction::MechanicList(action) => {
                self.apply_mechanic_list(project_id, factory_id, action)
            }
            FactoryAction::Mechanic { mechanic, action } => {
                // 配方/资源变化后，让外层校验机器与配方/资源的类别兼容性。
                let needs_compat = matches!(
                    &action,
                    MechanicAction::Recipe(RecipeMechanicAction::SetRecipe { .. })
                        | MechanicAction::Mining(MiningMechanicAction::SetResource { .. })
                );
                // 机器变化后，让外层按机器槽位上限钳制模块数量。
                let needs_clamp = matches!(
                    &action,
                    MechanicAction::Recipe(RecipeMechanicAction::SetMachine { .. })
                        | MechanicAction::Mining(MiningMechanicAction::SetMachine { .. })
                );
                let entry = self
                    .factory_mut(project_id, factory_id)?
                    .mechanics
                    .iter_mut()
                    .find(|entry| entry.id == mechanic)
                    .ok_or(RuntimeError::MechanicNotFound {
                        project: project_id,
                        factory: factory_id,
                        mechanic,
                    })?;
                let changed = apply_mechanic_action(entry, action)?;
                let mut outcome = Outcome::changed_factory_if(changed, project_id, factory_id);
                if changed {
                    if needs_compat {
                        outcome
                            .commands
                            .push(RuntimeCommand::EnsureMachineCompat {
                                project: project_id,
                                factory: factory_id,
                                mechanic,
                            });
                    }
                    if needs_clamp {
                        outcome
                            .commands
                            .push(RuntimeCommand::ClampModules {
                                project: project_id,
                                factory: factory_id,
                                mechanic,
                            });
                    }
                }
                Ok(outcome)
            }
            FactoryAction::Flow(action) => match action {
                FlowAction::AddToTarget { flow, amount } => {
                    let target = FlowTarget {
                        id: self.allocate_id(),
                        flow,
                        amount,
                    };
                    self.factory_mut(project_id, factory_id)?
                        .targets
                        .push(target);
                    Ok(Outcome::changed_factory(project_id, factory_id))
                }
                FlowAction::AddToExternalInput { flow, penalty } => {
                    validate_non_negative("external input penalty", penalty)?;
                    let input = ExternalInput {
                        id: self.allocate_id(),
                        flow,
                        penalty,
                    };
                    self.factory_mut(project_id, factory_id)?
                        .external_inputs
                        .push(input);
                    Ok(Outcome::changed_factory(project_id, factory_id))
                }
                FlowAction::RequestSuggestions { flow, amount } => {
                    Ok(Outcome::command(RuntimeCommand::RequestSuggestions {
                        project: project_id,
                        factory: factory_id,
                        flow,
                        amount,
                    }))
                }
            },
            FactoryAction::Suggestion(action) => {
                self.apply_suggestion(project_id, factory_id, action)
            }
            FactoryAction::Cleanup(action) => Ok(Outcome::command(RuntimeCommand::Cleanup {
                project: project_id,
                factory: factory_id,
                action,
            })),
            FactoryAction::Solve(action) => match action {
                SolveAction::Recompute => Ok(Outcome::command(RuntimeCommand::Recompute {
                    project: project_id,
                    factory: factory_id,
                })),
                SolveAction::AutoPlan => Ok(Outcome::command(RuntimeCommand::AutoPlan {
                    project: project_id,
                    factory: factory_id,
                })),
            },
        }
    }

    fn apply_target(
        &mut self,
        project_id: ProjectId,
        factory_id: FactoryId,
        action: TargetAction,
    ) -> Result<Outcome, RuntimeError> {
        let mut target_to_add = None;
        if let TargetAction::Add { target } = &action {
            let mut target = target.clone();
            if target.id.0 == 0 {
                target.id = self.allocate_id();
            }
            target_to_add = Some(target);
        }
        if let Some(target) = target_to_add {
            let factory = self.factory_mut(project_id, factory_id)?;
            ensure_unique_target(factory, target.id)?;
            factory.targets.push(target);
            return Ok(Outcome::changed_factory(project_id, factory_id));
        }

        let factory = self.factory_mut(project_id, factory_id)?;
        let changed = match action {
            TargetAction::Add { .. } => unreachable!("handled above"),
            TargetAction::Remove { target } => remove_by_id(&mut factory.targets, target)?,
            TargetAction::SetFlow { target, flow } => replace(
                &mut find_target_mut(&mut factory.targets, target)?.flow,
                flow,
            ),
            TargetAction::SetAmount { target, amount } => {
                validate_finite("target amount", amount)?;
                replace(
                    &mut find_target_mut(&mut factory.targets, target)?.amount,
                    amount,
                )
            }
            TargetAction::Reorder { target, position } => {
                let index = index_by_id(&factory.targets, target)?;
                move_item(&mut factory.targets, index, position)
            }
        };
        Ok(Outcome::changed_factory_if(changed, project_id, factory_id))
    }

    fn apply_target_expression(
        &mut self,
        project_id: ProjectId,
        factory_id: FactoryId,
        action: TargetExpressionAction,
    ) -> Result<Outcome, RuntimeError> {
        if let TargetExpressionAction::Add { mut expression } = action {
            if expression.id.0 == 0 {
                expression.id = self.allocate_id();
            }
            for term in &mut expression.terms {
                if term.id.0 == 0 {
                    term.id = self.allocate_id();
                }
            }
            let factory = self.factory_mut(project_id, factory_id)?;
            ensure_unique_expression(factory, expression.id)?;
            factory.target_expressions.push(expression);
            return Ok(Outcome::changed_factory(project_id, factory_id));
        }

        let factory = self.factory_mut(project_id, factory_id)?;
        let changed = match action {
            TargetExpressionAction::Add { .. } => unreachable!("handled above"),
            TargetExpressionAction::Remove { expression } => {
                remove_by_id(&mut factory.target_expressions, expression)?
            }
            TargetExpressionAction::SetConstant {
                expression,
                constant,
            } => {
                validate_finite("target expression constant", constant)?;
                replace(
                    &mut find_expression_mut(&mut factory.target_expressions, expression)?.constant,
                    constant,
                )
            }
            TargetExpressionAction::AddTerm {
                expression,
                mut term,
            } => {
                if term.id.0 == 0 {
                    term.id = TargetTermId(next_free_id(factory));
                }
                let expression = find_expression_mut(&mut factory.target_expressions, expression)?;
                if expression
                    .terms
                    .iter()
                    .any(|candidate| candidate.id == term.id)
                {
                    return Err(RuntimeError::DuplicateId("target term"));
                }
                expression.terms.push(term);
                true
            }
            TargetExpressionAction::RemoveTerm { expression, term } => {
                let expression = find_expression_mut(&mut factory.target_expressions, expression)?;
                remove_by_id(&mut expression.terms, term)?
            }
            TargetExpressionAction::SetTermFlow {
                expression,
                term,
                flow,
            } => replace(&mut find_term_mut(factory, expression, term)?.flow, flow),
            TargetExpressionAction::SetTermCoefficient {
                expression,
                term,
                coefficient,
            } => {
                validate_finite("target term coefficient", coefficient)?;
                replace(
                    &mut find_term_mut(factory, expression, term)?.coefficient,
                    coefficient,
                )
            }
            TargetExpressionAction::Reorder {
                expression,
                position,
            } => {
                let index = index_by_id(&factory.target_expressions, expression)?;
                move_item(&mut factory.target_expressions, index, position)
            }
            TargetExpressionAction::ReorderTerm {
                expression,
                term,
                position,
            } => {
                let expression = find_expression_mut(&mut factory.target_expressions, expression)?;
                let index = index_by_id(&expression.terms, term)?;
                move_item(&mut expression.terms, index, position)
            }
        };
        Ok(Outcome::changed_factory_if(changed, project_id, factory_id))
    }

    fn apply_external_input(
        &mut self,
        project_id: ProjectId,
        factory_id: FactoryId,
        action: ExternalInputAction,
    ) -> Result<Outcome, RuntimeError> {
        if let ExternalInputAction::Add { mut input } = action {
            if input.id.0 == 0 {
                input.id = self.allocate_id();
            }
            let factory = self.factory_mut(project_id, factory_id)?;
            ensure_unique_external(factory, input.id)?;
            factory.external_inputs.push(input);
            return Ok(Outcome::changed_factory(project_id, factory_id));
        }

        let factory = self.factory_mut(project_id, factory_id)?;
        let changed = match action {
            ExternalInputAction::Add { .. } => unreachable!("handled above"),
            ExternalInputAction::Remove { input } => {
                remove_by_id(&mut factory.external_inputs, input)?
            }
            ExternalInputAction::SetFlow { input, flow } => replace(
                &mut find_external_mut(&mut factory.external_inputs, input)?.flow,
                flow,
            ),
            ExternalInputAction::SetPenalty { input, penalty } => {
                validate_non_negative("external input penalty", penalty)?;
                replace(
                    &mut find_external_mut(&mut factory.external_inputs, input)?.penalty,
                    penalty,
                )
            }
            ExternalInputAction::Reorder { input, position } => {
                let index = index_by_id(&factory.external_inputs, input)?;
                move_item(&mut factory.external_inputs, index, position)
            }
            ExternalInputAction::ReplaceFromLocation { location } => {
                return Ok(Outcome::command(RuntimeCommand::ReplaceExternalInputs {
                    project: project_id,
                    factory: factory_id,
                    location,
                }));
            }
        };
        Ok(Outcome::changed_factory_if(changed, project_id, factory_id))
    }

    fn apply_mechanic_list(
        &mut self,
        project_id: ProjectId,
        factory_id: FactoryId,
        action: MechanicListAction,
    ) -> Result<Outcome, RuntimeError> {
        match action {
            MechanicListAction::Add { kind } => {
                let id = self.allocate_id();
                let entry =
                    MechanicEntry::new(id, kind).ok_or(RuntimeError::UnsupportedMechanic)?;
                self.factory_mut(project_id, factory_id)?
                    .mechanics
                    .push(entry);
                Ok(Outcome::changed_factory(project_id, factory_id))
            }
            MechanicListAction::Remove { mechanic } => {
                let changed = remove_by_id(
                    &mut self.factory_mut(project_id, factory_id)?.mechanics,
                    mechanic,
                )?;
                Ok(Outcome::changed_factory_if(changed, project_id, factory_id))
            }
            MechanicListAction::Clone { mechanic } => {
                let mut clone = self
                    .factory(project_id, factory_id)?
                    .mechanics
                    .iter()
                    .find(|entry| entry.id == mechanic)
                    .cloned()
                    .ok_or(RuntimeError::MechanicNotFound {
                        project: project_id,
                        factory: factory_id,
                        mechanic,
                    })?;
                clone.id = self.allocate_id();
                let new_id = clone.id;
                let mechanics = &mut self.factory_mut(project_id, factory_id)?.mechanics;
                let index = index_by_id(mechanics, mechanic)?;
                mechanics.insert(index + 1, clone);
                self.ui.selected_factory = Some(factory_id);
                self.ui.selected_mechanic = Some(new_id);
                Ok(Outcome::changed_factory(project_id, factory_id))
            }
            MechanicListAction::Reorder { mechanic, position } => {
                let mechanics = &mut self.factory_mut(project_id, factory_id)?.mechanics;
                let index = index_by_id(mechanics, mechanic)?;
                let changed = move_item(mechanics, index, position);
                Ok(Outcome::changed_factory_if(changed, project_id, factory_id))
            }
            MechanicListAction::SetEnabled { mechanic, enabled } => {
                let entry = self
                    .factory_mut(project_id, factory_id)?
                    .mechanics
                    .iter_mut()
                    .find(|entry| entry.id == mechanic)
                    .ok_or(RuntimeError::MechanicNotFound {
                        project: project_id,
                        factory: factory_id,
                        mechanic,
                    })?;
                let changed = replace(&mut entry.enabled, enabled);
                Ok(Outcome::changed_factory_if(changed, project_id, factory_id))
            }
        }
    }

    fn apply_suggestion(
        &mut self,
        project_id: ProjectId,
        factory_id: FactoryId,
        action: SuggestionAction,
    ) -> Result<Outcome, RuntimeError> {
        match action {
            SuggestionAction::SelectMechanic { mechanic } => {
                self.factory(project_id, factory_id)?
                    .mechanics
                    .iter()
                    .find(|entry| entry.id == mechanic)
                    .ok_or(RuntimeError::MechanicNotFound {
                        project: project_id,
                        factory: factory_id,
                        mechanic,
                    })?;
                self.ui.selected_mechanic = Some(mechanic);
                Ok(Outcome::none())
            }
            SuggestionAction::SetFilter { filter } => {
                self.ui.suggestion_filter = filter;
                Ok(Outcome::none())
            }
            SuggestionAction::Accept { candidate } => {
                let id = self.allocate_id();
                let mut entry = match candidate {
                    SuggestionCandidate::Recipe { recipe } => {
                        let mut entry = MechanicEntry::new(id, MechanicKind::Recipe).unwrap();
                        if let Mechanic::Recipe(mechanic) = &mut entry.mechanic {
                            mechanic.recipe = recipe;
                        }
                        entry
                    }
                    SuggestionCandidate::Resource { resource } => {
                        let mut entry = MechanicEntry::new(id, MechanicKind::Mining).unwrap();
                        if let Mechanic::Mining(mechanic) = &mut entry.mechanic {
                            mechanic.resource = resource;
                        }
                        entry
                    }
                    SuggestionCandidate::ItemFuel { item } => {
                        let mut entry = MechanicEntry::new(id, MechanicKind::ItemFuel).unwrap();
                        if let Mechanic::ItemFuel(mechanic) = &mut entry.mechanic {
                            mechanic.item = item;
                        }
                        entry
                    }
                    SuggestionCandidate::Generator { generator } => {
                        let mut entry = MechanicEntry::new(id, MechanicKind::Generator).unwrap();
                        if let Mechanic::Generator(mechanic) = &mut entry.mechanic {
                            mechanic.generator = generator;
                        }
                        entry
                    }
                };
                entry.enabled = true;
                self.factory_mut(project_id, factory_id)?
                    .mechanics
                    .push(entry);
                self.ui.selected_mechanic = Some(id);
                Ok(Outcome::changed_factory(project_id, factory_id))
            }
            SuggestionAction::Dismiss => Ok(Outcome::none()),
        }
    }

    fn apply_ui(&mut self, action: UiAction) -> Result<Outcome, RuntimeError> {
        match action {
            UiAction::SelectProject { project } => {
                if let Some(project) = project {
                    self.project(project)?;
                }
                self.ui.selected_project = project;
                self.ui.selected_factory = None;
                Ok(Outcome::none())
            }
            UiAction::SelectFactory { factory } => {
                if let Some(project) = self.ui.selected_project {
                    if let Some(factory) = factory {
                        self.factory(project, factory)?;
                    }
                } else if factory.is_some() {
                    return Err(RuntimeError::InvalidOperation(
                        "cannot select a factory without a selected project",
                    ));
                }
                self.ui.selected_factory = factory;
                Ok(Outcome::none())
            }
            UiAction::SelectPage { page } => {
                if let ProjectPage::Factory(factory) = page {
                    let project = self
                        .ui
                        .selected_project
                        .ok_or(RuntimeError::InvalidOperation("no selected project"))?;
                    self.factory(project, factory)?;
                    self.ui.selected_factory = Some(factory);
                }
                self.ui.page = page;
                Ok(Outcome::none())
            }
            UiAction::OpenSelector { target } => {
                self.ui.selector = Some(SelectorState {
                    target,
                    ..SelectorState::default()
                });
                Ok(Outcome::none())
            }
            UiAction::CloseSelector => {
                self.ui.selector = None;
                Ok(Outcome::none())
            }
            UiAction::SetSelectorQuery { query } => {
                self.selector_mut()?.query = query;
                Ok(Outcome::none())
            }
            UiAction::SelectSelectorGroup { group } => {
                let selector = self.selector_mut()?;
                selector.group = group;
                selector.subgroup = 0;
                Ok(Outcome::none())
            }
            UiAction::SelectSelectorSubgroup { subgroup } => {
                self.selector_mut()?.subgroup = subgroup;
                Ok(Outcome::none())
            }
            UiAction::CommitSelector { target, value } => {
                self.ui.selector = None;
                self.apply_selector_commit(target, value)
            }
            UiAction::SelectSuggestionMechanic { mechanic } => {
                self.ui.suggestion_mechanic = mechanic;
                Ok(Outcome::none())
            }
            UiAction::OpenLogs => {
                self.ui.logs_open = true;
                Ok(Outcome::none())
            }
            UiAction::SetFontFilter { filter } => {
                self.ui.font_filter = filter;
                Ok(Outcome::none())
            }
            UiAction::SelectFont { font } => {
                self.ui.font = Some(font);
                Ok(Outcome::none())
            }
            UiAction::SetLocale { locale } => {
                self.ui.locale = Some(locale);
                Ok(Outcome::none())
            }
            UiAction::ReloadIcons => Ok(Outcome::none()),
            UiAction::RequestWindowClose => {
                self.ui.close_requested = true;
                Ok(Outcome::none())
            }
            UiAction::ResolveWindowClose { decision } => {
                if matches!(decision, CloseDecision::Cancel) {
                    self.ui.close_requested = false;
                }
                Ok(Outcome::none())
            }
        }
    }

    fn apply_selector_commit(
        &mut self,
        target: SelectorTarget,
        value: SelectorValue,
    ) -> Result<Outcome, RuntimeError> {
        let project = self
            .ui
            .selected_project
            .ok_or(RuntimeError::InvalidOperation("no selected project"))?;
        match target {
            SelectorTarget::Target { factory, target } => {
                let flow = selector_value_to_flow(value)?;
                self.apply_factory(
                    project,
                    factory,
                    FactoryAction::Target(TargetAction::SetFlow { target, flow }),
                )
            }
            SelectorTarget::TargetTerm {
                factory,
                expression,
                term,
            } => {
                let flow = selector_value_to_flow(value)?;
                self.apply_factory(
                    project,
                    factory,
                    FactoryAction::TargetExpression(TargetExpressionAction::SetTermFlow {
                        expression,
                        term,
                        flow,
                    }),
                )
            }
            SelectorTarget::ExternalInput { factory, input } => {
                let flow = selector_value_to_flow(value)?;
                self.apply_factory(
                    project,
                    factory,
                    FactoryAction::ExternalInput(ExternalInputAction::SetFlow { input, flow }),
                )
            }
            SelectorTarget::Mechanic {
                factory,
                mechanic,
                kind,
            } => {
                let mechanic_kind = self.mechanic_kind(project, factory, mechanic)?;
                let action = selector_to_mechanic_action(mechanic_kind, kind, value)?;
                self.apply_factory(
                    project,
                    factory,
                    FactoryAction::Mechanic { mechanic, action },
                )
            }
            SelectorTarget::ModuleSlot {
                factory,
                mechanic,
                slot,
            } => {
                let module = match value {
                    SelectorValue::IdWithQuality(module) => Some(module),
                    _ => {
                        return Err(RuntimeError::InvalidOperation(
                            "module selector requires an item with quality",
                        ));
                    }
                };
                let mechanic_kind = self.mechanic_kind(project, factory, mechanic)?;
                let action = module_action_for_kind(
                    mechanic_kind,
                    ModuleAction::SetModuleSlot { slot, module },
                )?;
                self.apply_factory(
                    project,
                    factory,
                    FactoryAction::Mechanic { mechanic, action },
                )
            }
            SelectorTarget::Beacon {
                factory,
                mechanic,
                beacon,
            } => {
                let value = match value {
                    SelectorValue::IdWithQuality(value) => value,
                    _ => {
                        return Err(RuntimeError::InvalidOperation(
                            "beacon selector requires an entity",
                        ));
                    }
                };
                let mechanic_kind = self.mechanic_kind(project, factory, mechanic)?;
                let action =
                    module_action_for_kind(mechanic_kind, ModuleAction::SetBeacon { beacon, value })?;
                self.apply_factory(
                    project,
                    factory,
                    FactoryAction::Mechanic { mechanic, action },
                )
            }
            SelectorTarget::BeaconModule {
                factory,
                mechanic,
                beacon,
                module,
            } => {
                let value = match value {
                    SelectorValue::IdWithQuality(value) => value,
                    _ => {
                        return Err(RuntimeError::InvalidOperation(
                            "beacon module requires an item",
                        ));
                    }
                };
                let mechanic_kind = self.mechanic_kind(project, factory, mechanic)?;
                let action = module_action_for_kind(
                    mechanic_kind,
                    ModuleAction::SetBeaconModule {
                        beacon,
                        module,
                        value,
                    },
                )?;
                self.apply_factory(
                    project,
                    factory,
                    FactoryAction::Mechanic { mechanic, action },
                )
            }
            SelectorTarget::EnumeratedModule { .. } => {
                let module = match value {
                    SelectorValue::IdWithQuality(module) => module,
                    _ => {
                        return Err(RuntimeError::InvalidOperation(
                            "enumerated module requires an item",
                        ));
                    }
                };
                // Planning preferences are project-global.
                self.apply_project(
                    project,
                    ProjectAction::Planning(PlanningAction::AddEnumeratedModule { module }),
                )
            }
            SelectorTarget::EnumeratedBeacon { .. }
            | SelectorTarget::ProjectPlanet
            | SelectorTarget::ProjectSurface
            | SelectorTarget::ProjectQuality
            | SelectorTarget::Technology => Err(RuntimeError::InvalidOperation(
                "this selector must be committed through a field-specific message",
            )),
        }
    }

    fn mechanic_kind(
        &self,
        project: ProjectId,
        factory: FactoryId,
        mechanic: MechanicId,
    ) -> Result<MechanicKind, RuntimeError> {
        self.factory(project, factory)?
            .mechanics
            .iter()
            .find(|entry| entry.id == mechanic)
            .map(MechanicEntry::kind)
            .ok_or(RuntimeError::MechanicNotFound {
                project,
                factory,
                mechanic,
            })
    }

    fn new_factory(
        &mut self,
        id: FactoryId,
        name: String,
        template: FactoryTemplate,
    ) -> FactoryDocument {
        let mut factory = FactoryDocument {
            id,
            name: non_empty(name, "Unnamed factory"),
            ..FactoryDocument::default()
        };
        if matches!(template, FactoryTemplate::DefaultMechanics) {
            for kind in MechanicKind::ALL {
                let mechanic_id = self.allocate_id();
                factory
                    .mechanics
                    .push(MechanicEntry::new(mechanic_id, kind).unwrap());
            }
        }
        factory
    }

    fn clone_factory(&mut self, mut factory: FactoryDocument) -> FactoryDocument {
        factory.id = self.allocate_id();
        for target in &mut factory.targets {
            target.id = self.allocate_id();
        }
        for expression in &mut factory.target_expressions {
            expression.id = self.allocate_id();
            for term in &mut expression.terms {
                term.id = self.allocate_id();
            }
        }
        for input in &mut factory.external_inputs {
            input.id = self.allocate_id();
        }
        for mechanic in &mut factory.mechanics {
            mechanic.id = self.allocate_id();
        }
        factory.name.push_str(" copy");
        factory
    }

    fn remove_project(&mut self, id: ProjectId) -> Result<(), RuntimeError> {
        let index = self
            .document
            .projects
            .iter()
            .position(|project| project.id == id)
            .ok_or(RuntimeError::ProjectNotFound(id))?;
        self.document.projects.remove(index);
        self.dirty_projects.remove(&id);
        if self.ui.selected_project == Some(id) {
            self.ui.selected_project = self
                .document
                .projects
                .get(index.saturating_sub(1))
                .map(|p| p.id);
            self.ui.selected_factory = None;
        }
        Ok(())
    }

    fn project_mut(&mut self, id: ProjectId) -> Result<&mut ProjectDocument, RuntimeError> {
        self.document
            .projects
            .iter_mut()
            .find(|project| project.id == id)
            .ok_or(RuntimeError::ProjectNotFound(id))
    }

    /// 整体替换里程碑（"默认里程碑"等批量操作）。
    pub fn replace_milestones(
        &mut self,
        project_id: ProjectId,
        milestones: Vec<crate::document::Milestone>,
    ) -> Result<bool, RuntimeError> {
        let changed = replace(
            &mut self.project_mut(project_id)?.settings.milestones,
            milestones,
        );
        Ok(changed)
    }

    fn factory_mut(
        &mut self,
        project: ProjectId,
        factory: FactoryId,
    ) -> Result<&mut FactoryDocument, RuntimeError> {
        self.project_mut(project)?
            .factories
            .iter_mut()
            .find(|candidate| candidate.id == factory)
            .ok_or(RuntimeError::FactoryNotFound { project, factory })
    }

    pub fn allocate_id<T: From<u64>>(&mut self) -> T {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        T::from(id)
    }

    /// 把另一个文档的项目导入当前文档（追加，不替换现有项目）。
    ///
    /// 与现有项目 id 冲突时整体重分配：项目 → 工厂 → 目标/表达式/项 →
    /// 外部输入 → 机制（所有子 id 同步映射，保持引用一致）。
    pub fn import_projects(&mut self, imported: &AppDocument) {
        for project in &imported.projects {
            let mut project = project.clone();
            let project_id_collides =
                self.document.projects.iter().any(|p| p.id == project.id);
            // 工厂/机制等子 id 若与现有文档冲突也要重分配（全局唯一，
            // 避免跨项目引用歧义）。为简化，项目冲突时整套重分配。
            let remap_all = project_id_collides;
            if remap_all {
                project.id = self.allocate_id();
            }
            let mut factory_ids: HashMap<FactoryId, FactoryId> = HashMap::new();
            for factory in &mut project.factories {
                if remap_all
                    || self
                        .document
                        .projects
                        .iter()
                        .any(|p| p.factories.iter().any(|f| f.id == factory.id))
                {
                    let new_id = self.allocate_id();
                    factory_ids.insert(factory.id, new_id);
                    factory.id = new_id;
                }
            }
            for factory in &mut project.factories {
                for target in &mut factory.targets {
                    if remap_all || self.project_contains_id(target.id.0) {
                        target.id = self.allocate_id();
                    }
                }
                for expression in &mut factory.target_expressions {
                    if remap_all || self.project_contains_id(expression.id.0) {
                        expression.id = self.allocate_id();
                    }
                    for term in &mut expression.terms {
                        if remap_all || self.project_contains_id(term.id.0) {
                            term.id = self.allocate_id();
                        }
                    }
                }
                for input in &mut factory.external_inputs {
                    if remap_all || self.project_contains_id(input.id.0) {
                        input.id = self.allocate_id();
                    }
                }
                for mechanic in &mut factory.mechanics {
                    if remap_all || self.project_contains_id(mechanic.id.0) {
                        mechanic.id = self.allocate_id();
                    }
                }
            }
            self.document.projects.push(project);
        }
        self.refresh_next_id();
    }

    /// 当前文档任意项目是否已使用该 id（跨项目全局检查）。
    fn project_contains_id(&self, id: u64) -> bool {
        self.document.projects.iter().any(|p| {
            p.id.0 == id
                || p.factories.iter().any(|f| {
                    f.id.0 == id
                        || f.targets.iter().any(|t| t.id.0 == id)
                        || f.target_expressions.iter().any(|e| {
                            e.id.0 == id || e.terms.iter().any(|t| t.id.0 == id)
                        })
                        || f.external_inputs.iter().any(|i| i.id.0 == id)
                        || f.mechanics.iter().any(|m| m.id.0 == id)
                })
        })
    }

    fn refresh_next_id(&mut self) {
        let mut max_id = 0;
        for project in &self.document.projects {
            max_id = max_id.max(project.id.0);
            for factory in &project.factories {
                max_id = max_id.max(factory.id.0);
                for target in &factory.targets {
                    max_id = max_id.max(target.id.0);
                }
                for expression in &factory.target_expressions {
                    max_id = max_id.max(expression.id.0);
                    for term in &expression.terms {
                        max_id = max_id.max(term.id.0);
                    }
                }
                for input in &factory.external_inputs {
                    max_id = max_id.max(input.id.0);
                }
                for mechanic in &factory.mechanics {
                    max_id = max_id.max(mechanic.id.0);
                }
            }
        }
        self.next_id = max_id.saturating_add(1).max(1);
    }

    fn select_first_project(&mut self) {
        self.ui.selected_project = self.document.projects.first().map(|project| project.id);
        self.ui.selected_factory = self
            .document
            .projects
            .first()
            .and_then(|project| project.factories.first())
            .map(|factory| factory.id);
    }

    fn selector_mut(&mut self) -> Result<&mut SelectorState, RuntimeError> {
        self.ui
            .selector
            .as_mut()
            .ok_or(RuntimeError::InvalidOperation("no selector is open"))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UiState {
    pub selected_project: Option<ProjectId>,
    pub selected_factory: Option<FactoryId>,
    pub selected_mechanic: Option<MechanicId>,
    pub page: ProjectPage,
    pub selector: Option<SelectorState>,
    pub suggestion_mechanic: usize,
    pub suggestion_filter: String,
    pub logs_open: bool,
    pub font_filter: String,
    pub font: Option<String>,
    pub locale: Option<String>,
    pub close_requested: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            selected_project: None,
            selected_factory: None,
            selected_mechanic: None,
            page: ProjectPage::Preferences,
            selector: None,
            suggestion_mechanic: 0,
            suggestion_filter: String::new(),
            logs_open: false,
            font_filter: String::new(),
            font: None,
            locale: None,
            close_requested: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectorState {
    pub target: SelectorTarget,
    pub query: String,
    pub group: usize,
    pub subgroup: usize,
}

impl Default for SelectorState {
    fn default() -> Self {
        Self {
            target: SelectorTarget::ProjectQuality,
            query: String::new(),
            group: 0,
            subgroup: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DispatchResult {
    pub revision: u64,
    pub changed: bool,
    pub commands: Vec<RuntimeCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    ProjectNotFound(ProjectId),
    FactoryNotFound {
        project: ProjectId,
        factory: FactoryId,
    },
    MechanicNotFound {
        project: ProjectId,
        factory: FactoryId,
        mechanic: MechanicId,
    },
    TargetNotFound(TargetId),
    TargetExpressionNotFound(TargetExpressionId),
    TargetTermNotFound(TargetTermId),
    ExternalInputNotFound(ExternalInputId),
    DuplicateId(&'static str),
    UnsupportedMechanic,
    InvalidOperation(&'static str),
    InvalidValue(String),
    DataNotLoaded,
    DataLoad(String),
    ContextNotFound(String),
    Io(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProjectNotFound(id) => write!(f, "project {} was not found", id.0),
            Self::FactoryNotFound { project, factory } => {
                write!(
                    f,
                    "factory {} was not found in project {}",
                    factory.0, project.0
                )
            }
            Self::MechanicNotFound {
                project,
                factory,
                mechanic,
            } => write!(
                f,
                "mechanic {} was not found in factory {} of project {}",
                mechanic.0, factory.0, project.0
            ),
            Self::TargetNotFound(id) => write!(f, "target {} was not found", id.0),
            Self::TargetExpressionNotFound(id) => {
                write!(f, "target expression {} was not found", id.0)
            }
            Self::TargetTermNotFound(id) => write!(f, "target term {} was not found", id.0),
            Self::ExternalInputNotFound(id) => write!(f, "external input {} was not found", id.0),
            Self::DuplicateId(kind) => write!(f, "duplicate {kind} id"),
            Self::UnsupportedMechanic => f.write_str("unsupported mechanic variant"),
            Self::InvalidOperation(message) => f.write_str(message),
            Self::InvalidValue(message) => f.write_str(message),
            Self::DataNotLoaded => f.write_str("game data has not been loaded"),
            Self::DataLoad(message) => write!(f, "failed to load game data: {message}"),
            Self::ContextNotFound(id) => write!(f, "game context {id} is not loaded"),
            Self::Io(message) => write!(f, "I/O error: {message}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

#[derive(Debug, Default)]
struct Outcome {
    changed: bool,
    project: Option<ProjectId>,
    factory: Option<FactoryId>,
    recompute_all: bool,
    commands: Vec<RuntimeCommand>,
}

impl Outcome {
    fn none() -> Self {
        Self::default()
    }

    fn changed(project: ProjectId) -> Self {
        Self {
            changed: true,
            project: Some(project),
            ..Self::default()
        }
    }

    fn changed_without_project() -> Self {
        Self {
            changed: true,
            ..Self::default()
        }
    }

    fn changed_factory(project: ProjectId, factory: FactoryId) -> Self {
        Self {
            changed: true,
            project: Some(project),
            factory: Some(factory),
            ..Self::default()
        }
    }

    fn changed_factory_if(changed: bool, project: ProjectId, factory: FactoryId) -> Self {
        if changed {
            Self::changed_factory(project, factory)
        } else {
            Self::none()
        }
    }

    fn all_factories(project: ProjectId) -> Self {
        Self {
            changed: true,
            project: Some(project),
            recompute_all: true,
            ..Self::default()
        }
    }

    fn all_factories_if(changed: bool, project: ProjectId) -> Self {
        if changed {
            Self::all_factories(project)
        } else {
            Self::none()
        }
    }

    fn changed_if(changed: bool, project: ProjectId) -> Self {
        if changed {
            Self::changed(project)
        } else {
            Self::none()
        }
    }

    fn command(command: RuntimeCommand) -> Self {
        Self {
            commands: vec![command],
            ..Self::default()
        }
    }

    fn commands(commands: Vec<RuntimeCommand>) -> Self {
        Self {
            commands,
            ..Self::default()
        }
    }
}

fn apply_factory_context(
    settings: &mut crate::document::FactorySettings,
    action: FactoryContextAction,
) -> Result<bool, RuntimeError> {
    Ok(match action {
        FactoryContextAction::SetPlanet { planet } => replace(&mut settings.planet, planet),
        FactoryContextAction::SetSurface { surface } => replace(&mut settings.surface, surface),
        FactoryContextAction::SetMajorQuality { quality } => {
            if quality.is_empty() {
                return Err(RuntimeError::InvalidValue(
                    "quality cannot be empty".to_string(),
                ));
            }
            replace(&mut settings.major_quality, quality)
        }
        FactoryContextAction::SetDebug { enabled } => replace(&mut settings.debug, enabled),
    })
}

/// Build a kind-tagged mechanic action from a selector commit.  The target
/// mechanic's kind is known up front, so the produced action always matches
/// the mechanic — a mismatched (selector kind, mechanic kind) pair is a
/// protocol error instead of a runtime dispatch decision.
fn selector_to_mechanic_action(
    mechanic_kind: MechanicKind,
    selector: SelectorKind,
    value: SelectorValue,
) -> Result<MechanicAction, RuntimeError> {
    use MechanicKind as K;
    use SelectorKind as S;
    let idwq = |value: SelectorValue| match value {
        SelectorValue::IdWithQuality(id) => Ok(id),
        _ => Err(RuntimeError::InvalidOperation("需要带品质的物品")),
    };
    let name = |value: SelectorValue| match value {
        SelectorValue::Name(name) => Ok(name),
        _ => Err(RuntimeError::InvalidOperation("需要名称")),
    };
    match (mechanic_kind, selector) {
        (K::Recipe, S::Recipe) => idwq(value)
            .map(|recipe| MechanicAction::Recipe(RecipeMechanicAction::SetRecipe { recipe })),
        (K::Recipe | K::Mining, S::Entity) => {
            idwq(value).map(|machine| match mechanic_kind {
                K::Recipe => {
                    MechanicAction::Recipe(RecipeMechanicAction::SetMachine { machine })
                }
                _ => MechanicAction::Mining(MiningMechanicAction::SetMachine { machine }),
            })
        }
        (K::Mining, S::Item) => name(value).map(|resource| {
            MechanicAction::Mining(MiningMechanicAction::SetResource { resource })
        }),
        (K::Spoil, S::Item) => idwq(value)
            .map(|item| MechanicAction::Spoil(SpoilMechanicAction::SetItem { item })),
        (K::Plant, S::Item) => idwq(value)
            .map(|seed| MechanicAction::Plant(PlantMechanicAction::SetSeed { seed })),
        (K::ItemFuel, S::Item) => idwq(value)
            .map(|item| MechanicAction::ItemFuel(ItemFuelMechanicAction::SetItem { item })),
        (K::ItemLaunch, S::Item) => idwq(value)
            .map(|item| MechanicAction::ItemLaunch(ItemLaunchMechanicAction::SetItem { item })),
        (K::Generator, S::Entity) => idwq(value).map(|generator| {
            MechanicAction::Generator(GeneratorMechanicAction::SetGenerator { generator })
        }),
        (K::Generator | K::Boiler, S::Fluid) => {
            name(value).map(|fluid| match mechanic_kind {
                K::Generator => {
                    MechanicAction::Generator(GeneratorMechanicAction::SetFluid { fluid })
                }
                _ => MechanicAction::Boiler(BoilerMechanicAction::SetFluid { fluid }),
            })
        }
        (K::Boiler, S::Entity) => idwq(value)
            .map(|boiler| MechanicAction::Boiler(BoilerMechanicAction::SetBoiler { boiler })),
        (K::Reactor, S::Entity) => idwq(value).map(|reactor| {
            MechanicAction::Reactor(ReactorMechanicAction::SetReactor { reactor })
        }),
        _ => Err(RuntimeError::InvalidOperation("选择器类型与该机制不匹配")),
    }
}

/// Only recipe and mining mechanics carry a module configuration.
fn module_action_for_kind(
    mechanic_kind: MechanicKind,
    action: ModuleAction,
) -> Result<MechanicAction, RuntimeError> {
    match mechanic_kind {
        MechanicKind::Recipe => Ok(MechanicAction::Recipe(RecipeMechanicAction::Module(action))),
        MechanicKind::Mining => Ok(MechanicAction::Mining(MiningMechanicAction::Module(action))),
        _ => Err(RuntimeError::InvalidOperation("该机制不支持模块配置")),
    }
}

fn apply_mechanic_action(
    entry: &mut MechanicEntry,
    action: MechanicAction,
) -> Result<bool, RuntimeError> {
    // Each MechanicAction variant names its mechanic kind explicitly; the
    // dispatch validates that the target mechanic matches before touching it.
    match action {
        MechanicAction::Recipe(action) => {
            let Mechanic::Recipe(mechanic) = &mut entry.mechanic else {
                return Err(kind_mismatch("recipe"));
            };
            apply_recipe_action(mechanic, action)
        }
        MechanicAction::Mining(action) => {
            let Mechanic::Mining(mechanic) = &mut entry.mechanic else {
                return Err(kind_mismatch("mining"));
            };
            apply_mining_action(mechanic, action)
        }
        MechanicAction::Spoil(action) => {
            let Mechanic::Spoil(mechanic) = &mut entry.mechanic else {
                return Err(kind_mismatch("spoil"));
            };
            apply_spoil_action(mechanic, action)
        }
        MechanicAction::Plant(action) => {
            let Mechanic::Plant(mechanic) = &mut entry.mechanic else {
                return Err(kind_mismatch("plant"));
            };
            apply_plant_action(mechanic, action)
        }
        MechanicAction::ItemFuel(action) => {
            let Mechanic::ItemFuel(mechanic) = &mut entry.mechanic else {
                return Err(kind_mismatch("item-fuel"));
            };
            apply_item_fuel_action(mechanic, action)
        }
        MechanicAction::ItemLaunch(action) => {
            let Mechanic::ItemLaunch(mechanic) = &mut entry.mechanic else {
                return Err(kind_mismatch("item-launch"));
            };
            apply_item_launch_action(mechanic, action)
        }
        MechanicAction::Generator(action) => {
            let Mechanic::Generator(mechanic) = &mut entry.mechanic else {
                return Err(kind_mismatch("generator"));
            };
            apply_generator_action(mechanic, action)
        }
        MechanicAction::Boiler(action) => {
            let Mechanic::Boiler(mechanic) = &mut entry.mechanic else {
                return Err(kind_mismatch("boiler"));
            };
            apply_boiler_action(mechanic, action)
        }
        MechanicAction::Reactor(action) => {
            let Mechanic::Reactor(mechanic) = &mut entry.mechanic else {
                return Err(kind_mismatch("reactor"));
            };
            apply_reactor_action(mechanic, action)
        }
        MechanicAction::Solar(action) => {
            let Mechanic::Solar(mechanic) = &mut entry.mechanic else {
                return Err(kind_mismatch("solar"));
            };
            apply_solar_action(mechanic, action)
        }
        MechanicAction::FluidFuel(action) => {
            let Mechanic::FluidFuel(mechanic) = &mut entry.mechanic else {
                return Err(kind_mismatch("fluid-fuel"));
            };
            apply_fluid_fuel_action(mechanic, action)
        }
        MechanicAction::FluidHeat(action) => {
            let Mechanic::FluidHeat(mechanic) = &mut entry.mechanic else {
                return Err(kind_mismatch("fluid-heat"));
            };
            apply_fluid_heat_action(mechanic, action)
        }
    }
}

fn kind_mismatch(kind: &'static str) -> RuntimeError {
    let message: &'static str = match kind {
        "recipe" => "该机制不是 recipe 类型",
        "mining" => "该机制不是 mining 类型",
        "spoil" => "该机制不是 spoil 类型",
        "plant" => "该机制不是 plant 类型",
        "item-fuel" => "该机制不是 item-fuel 类型",
        "item-launch" => "该机制不是 item-launch 类型",
        "generator" => "该机制不是 generator 类型",
        "boiler" => "该机制不是 boiler 类型",
        "reactor" => "该机制不是 reactor 类型",
        "solar" => "该机制不是 solar 类型",
        "fluid-fuel" => "该机制不是 fluid-fuel 类型",
        "fluid-heat" => "该机制不是 fluid-heat 类型",
        _ => "机制类型不匹配",
    };
    RuntimeError::InvalidOperation(message)
}

fn apply_recipe_action(
    mechanic: &mut metatorio_core::RecipeMechanic,
    action: RecipeMechanicAction,
) -> Result<bool, RuntimeError> {
    match action {
        RecipeMechanicAction::SetRecipe { recipe } => Ok(replace(&mut mechanic.recipe, recipe)),
        RecipeMechanicAction::SetMachine { machine } => {
            Ok(replace(&mut mechanic.machine, machine))
        }
        RecipeMechanicAction::SetFuel { fuel } => Ok(replace(&mut mechanic.fuel, fuel)),
        RecipeMechanicAction::SetFuelTemperature { temperature } => {
            Ok(replace(&mut mechanic.fuel_temperature, temperature))
        }
        RecipeMechanicAction::Module(action) => {
            apply_module_action(&mut mechanic.module_config, action)
        }
    }
}

fn apply_mining_action(
    mechanic: &mut metatorio_core::MiningMechanic,
    action: MiningMechanicAction,
) -> Result<bool, RuntimeError> {
    match action {
        MiningMechanicAction::SetResource { resource } => {
            Ok(replace(&mut mechanic.resource, resource))
        }
        MiningMechanicAction::SetMachine { machine } => {
            Ok(replace(&mut mechanic.machine, machine))
        }
        MiningMechanicAction::SetFuel { fuel } => Ok(replace(&mut mechanic.fuel, fuel)),
        MiningMechanicAction::SetFuelTemperature { temperature } => {
            Ok(replace(&mut mechanic.fuel_temperature, temperature))
        }
        MiningMechanicAction::Module(action) => {
            apply_module_action(&mut mechanic.module_config, action)
        }
    }
}

fn apply_spoil_action(
    mechanic: &mut metatorio_core::SpoilMechanic,
    action: SpoilMechanicAction,
) -> Result<bool, RuntimeError> {
    match action {
        SpoilMechanicAction::SetItem { item } => Ok(replace(&mut mechanic.item, item)),
    }
}

fn apply_plant_action(
    mechanic: &mut metatorio_core::PlantMechanic,
    action: PlantMechanicAction,
) -> Result<bool, RuntimeError> {
    match action {
        PlantMechanicAction::SetSeed { seed } => Ok(replace(&mut mechanic.seed, seed)),
    }
}

fn apply_item_fuel_action(
    mechanic: &mut metatorio_core::ItemFuelMechanic,
    action: ItemFuelMechanicAction,
) -> Result<bool, RuntimeError> {
    match action {
        ItemFuelMechanicAction::SetItem { item } => Ok(replace(&mut mechanic.item, item)),
    }
}

fn apply_item_launch_action(
    mechanic: &mut metatorio_core::ItemLaunchMechanic,
    action: ItemLaunchMechanicAction,
) -> Result<bool, RuntimeError> {
    match action {
        ItemLaunchMechanicAction::SetItem { item } => Ok(replace(&mut mechanic.item, item)),
        ItemLaunchMechanicAction::SetWeightMode { weight_mode } => {
            Ok(replace(&mut mechanic.weight_mode, weight_mode))
        }
    }
}

fn apply_generator_action(
    mechanic: &mut metatorio_core::GeneratorMechanic,
    action: GeneratorMechanicAction,
) -> Result<bool, RuntimeError> {
    match action {
        GeneratorMechanicAction::SetGenerator { generator } => {
            Ok(replace(&mut mechanic.generator, generator))
        }
        GeneratorMechanicAction::SetFluid { fluid } => Ok(replace(&mut mechanic.fluid, fluid)),
        GeneratorMechanicAction::SetTemperature { temperature } => {
            Ok(replace(&mut mechanic.temperature, temperature))
        }
    }
}

fn apply_boiler_action(
    mechanic: &mut metatorio_core::BoilerMechanic,
    action: BoilerMechanicAction,
) -> Result<bool, RuntimeError> {
    match action {
        BoilerMechanicAction::SetBoiler { boiler } => Ok(replace(&mut mechanic.boiler, boiler)),
        BoilerMechanicAction::SetFluid { fluid } => Ok(replace(&mut mechanic.fluid, fluid)),
        BoilerMechanicAction::SetTemperature { temperature } => {
            Ok(replace(&mut mechanic.temperature, temperature))
        }
        BoilerMechanicAction::SetFuel { fuel } => Ok(replace(&mut mechanic.fuel, fuel)),
        BoilerMechanicAction::SetFuelTemperature { temperature } => {
            Ok(replace(&mut mechanic.fuel_temperature, temperature))
        }
        BoilerMechanicAction::SetMode { mode } => Ok(replace(&mut mechanic.mode, mode)),
    }
}

fn apply_reactor_action(
    mechanic: &mut metatorio_core::ReactorMechanic,
    action: ReactorMechanicAction,
) -> Result<bool, RuntimeError> {
    match action {
        ReactorMechanicAction::SetReactor { reactor } => {
            Ok(replace(&mut mechanic.reactor, reactor))
        }
        ReactorMechanicAction::SetFuel { fuel } => Ok(replace(&mut mechanic.fuel, fuel)),
        ReactorMechanicAction::SetNeighbours { neighbours } => {
            if neighbours > 8 {
                return Err(RuntimeError::InvalidValue(
                    "reactor neighbours must be <= 8".to_string(),
                ));
            }
            Ok(replace(&mut mechanic.neighbours, neighbours))
        }
    }
}

fn apply_solar_action(
    mechanic: &mut metatorio_core::SolarMechanic,
    action: SolarMechanicAction,
) -> Result<bool, RuntimeError> {
    match action {
        SolarMechanicAction::SetSolarPanel { solar_panel } => {
            Ok(replace(&mut mechanic.solar_panel, solar_panel))
        }
        SolarMechanicAction::SetAccumulator { accumulator } => {
            Ok(replace(&mut mechanic.accumulator, accumulator))
        }
    }
}

fn apply_fluid_fuel_action(
    mechanic: &mut metatorio_core::FluidFuelMechanic,
    action: FluidFuelMechanicAction,
) -> Result<bool, RuntimeError> {
    match action {
        FluidFuelMechanicAction::SetFluid { fluid } => {
            if fluid.is_empty() {
                return Err(RuntimeError::InvalidValue("流体未选择".to_string()));
            }
            Ok(replace(&mut mechanic.fluid, fluid))
        }
        FluidFuelMechanicAction::SetTemperature { temperature } => {
            Ok(replace(&mut mechanic.temperature, temperature))
        }
    }
}

fn apply_fluid_heat_action(
    mechanic: &mut metatorio_core::FluidHeatMechanic,
    action: FluidHeatMechanicAction,
) -> Result<bool, RuntimeError> {
    match action {
        FluidHeatMechanicAction::SetFluid { fluid } => {
            if fluid.is_empty() {
                return Err(RuntimeError::InvalidValue("流体未选择".to_string()));
            }
            Ok(replace(&mut mechanic.fluid, fluid))
        }
        FluidHeatMechanicAction::SetTemperature { temperature } => {
            Ok(replace(&mut mechanic.temperature, temperature))
        }
    }
}

fn apply_module_action(
    config: &mut ModuleConfig,
    action: ModuleAction,
) -> Result<bool, RuntimeError> {
    match action {
        ModuleAction::SetModuleSlot { slot, module } => {
            if let Some(module) = module {
                // 复刻旧行为：非首个槽位被设置时，前面的空槽填充相同的插件。
                while config.modules.len() <= slot {
                    config.modules.push(module.clone());
                }
                Ok(replace(&mut config.modules[slot], module))
            } else if slot < config.modules.len() {
                config.modules.remove(slot);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        ModuleAction::ClampModules { max } => {
            let changed = config.modules.len() > max;
            config.modules.truncate(max);
            Ok(changed)
        }
        ModuleAction::ClearModules => {
            let changed = !config.modules.is_empty() || !config.beacons.is_empty();
            config.modules.clear();
            config.beacons.clear();
            Ok(changed)
        }
        ModuleAction::AddBeacon { beacon } => {
            if beacon.id.is_empty() {
                return Err(RuntimeError::InvalidValue(
                    "信标未选择，无法添加".to_string(),
                ));
            }
            // 重复按 IdWithQuality 整体判等（id + 品质）：同种信标不同品质允许并存。
            if config.beacons.iter().any(|existing| existing.beacon == beacon) {
                return Err(RuntimeError::InvalidValue(format!(
                    "信标 {}（{}）已添加，不能重复",
                    beacon.id, beacon.quality
                )));
            }
            config.beacons.push(BeaconConfig {
                beacon,
                ..Default::default()
            });
            Ok(true)
        }
        ModuleAction::RemoveBeacon { beacon } => {
            if beacon >= config.beacons.len() {
                return Err(RuntimeError::InvalidValue(
                    "beacon index is out of range".to_string(),
                ));
            }
            config.beacons.remove(beacon);
            Ok(true)
        }
        ModuleAction::SetBeacon { beacon, value } => {
            if value.id.is_empty() {
                return Err(RuntimeError::InvalidValue("信标未选择".to_string()));
            }
            // 按 IdWithQuality 判等（排除自身索引）：同种不同品质不算重复。
            if config
                .beacons
                .iter()
                .enumerate()
                .any(|(index, existing)| index != beacon && existing.beacon == value)
            {
                return Err(RuntimeError::InvalidValue(format!(
                    "信标 {}（{}）已添加，不能重复",
                    value.id, value.quality
                )));
            }
            let beacon = config.beacons.get_mut(beacon).ok_or_else(|| {
                RuntimeError::InvalidValue("beacon index is out of range".to_string())
            })?;
            Ok(replace(&mut beacon.beacon, value))
        }
        ModuleAction::SetBeaconCount { beacon, count } => {
            if count == 0 {
                return Err(RuntimeError::InvalidValue(
                    "beacon count must be positive".to_string(),
                ));
            }
            let beacon = config.beacons.get_mut(beacon).ok_or_else(|| {
                RuntimeError::InvalidValue("beacon index is out of range".to_string())
            })?;
            Ok(replace(&mut beacon.count, count))
        }
        ModuleAction::SetBeaconShare { beacon, share } => {
            validate_positive("beacon share", share)?;
            let beacon = config.beacons.get_mut(beacon).ok_or_else(|| {
                RuntimeError::InvalidValue("beacon index is out of range".to_string())
            })?;
            Ok(replace(&mut beacon.share, share))
        }
        ModuleAction::AddBeaconModule { beacon, module } => {
            let beacon = config.beacons.get_mut(beacon).ok_or_else(|| {
                RuntimeError::InvalidValue("beacon index is out of range".to_string())
            })?;
            beacon.modules.push((module, 0));
            Ok(true)
        }
        ModuleAction::RemoveBeaconModule { beacon, module } => {
            let beacon = config.beacons.get_mut(beacon).ok_or_else(|| {
                RuntimeError::InvalidValue("beacon index is out of range".to_string())
            })?;
            if module >= beacon.modules.len() {
                return Err(RuntimeError::InvalidValue(
                    "beacon module index is out of range".to_string(),
                ));
            }
            beacon.modules.remove(module);
            Ok(true)
        }
        ModuleAction::SetBeaconModule {
            beacon,
            module,
            value,
        } => {
            let beacon = config.beacons.get_mut(beacon).ok_or_else(|| {
                RuntimeError::InvalidValue("beacon index is out of range".to_string())
            })?;
            let slot = beacon.modules.get_mut(module).ok_or_else(|| {
                RuntimeError::InvalidValue("beacon module index is out of range".to_string())
            })?;
            Ok(replace(&mut slot.0, value))
        }
        ModuleAction::SetBeaconModuleCount {
            beacon,
            module,
            count,
        } => {
            let beacon = config.beacons.get_mut(beacon).ok_or_else(|| {
                RuntimeError::InvalidValue("beacon index is out of range".to_string())
            })?;
            let slot = beacon.modules.get_mut(module).ok_or_else(|| {
                RuntimeError::InvalidValue("beacon module index is out of range".to_string())
            })?;
            Ok(replace(&mut slot.1, count))
        }
    }
}

fn apply_planning_action(
    planning: &mut PlanningPreferences,
    action: PlanningAction,
) -> Result<bool, RuntimeError> {
    match action {
        PlanningAction::SetAlternativeCount { count } => {
            if count == 0 {
                return Err(RuntimeError::InvalidValue(
                    "alternative count must be positive".to_string(),
                ));
            }
            Ok(replace(&mut planning.alternative_count, count))
        }
        PlanningAction::AddMachinePreference { machine } => {
            if planning.machine_preferences.contains(&machine) {
                return Ok(false);
            }
            planning.machine_preferences.push(machine);
            Ok(true)
        }
        PlanningAction::RemoveMachinePreference { machine } => {
            let before = planning.machine_preferences.len();
            planning
                .machine_preferences
                .retain(|candidate| candidate != &machine);
            Ok(before != planning.machine_preferences.len())
        }
        PlanningAction::ReorderMachinePreference { machine, position } => {
            let index = planning
                .machine_preferences
                .iter()
                .position(|candidate| candidate == &machine)
                .ok_or_else(|| {
                    RuntimeError::InvalidValue("machine preference was not found".to_string())
                })?;
            Ok(move_item(
                &mut planning.machine_preferences,
                index,
                position,
            ))
        }
        PlanningAction::AddEnumeratedModule { module } => {
            if planning.enumerate_modules.contains(&module) {
                return Ok(false);
            }
            planning.enumerate_modules.insert(0, module);
            Ok(true)
        }
        PlanningAction::RemoveEnumeratedModule { module } => {
            let before = planning.enumerate_modules.len();
            planning
                .enumerate_modules
                .retain(|candidate| candidate != &module);
            Ok(before != planning.enumerate_modules.len())
        }
        PlanningAction::UseBestModules => Ok(false),
        PlanningAction::AddEnumeratedBeacon => {
            planning.enumerate_beacons.push(AutoBeaconPlan::default());
            Ok(true)
        }
        PlanningAction::RemoveEnumeratedBeacon { beacon } => {
            if beacon >= planning.enumerate_beacons.len() {
                return Err(RuntimeError::InvalidValue(
                    "enumerated beacon index is out of range".to_string(),
                ));
            }
            planning.enumerate_beacons.remove(beacon);
            Ok(true)
        }
        PlanningAction::SetEnumeratedBeacon { beacon, plan } => {
            let current = planning.enumerate_beacons.get_mut(beacon).ok_or_else(|| {
                RuntimeError::InvalidValue("enumerated beacon index is out of range".to_string())
            })?;
            Ok(replace(current, plan))
        }
        PlanningAction::EnumeratedBeaconModule { beacon, action } => {
            let plan = planning.enumerate_beacons.get_mut(beacon).ok_or_else(|| {
                RuntimeError::InvalidValue("enumerated beacon index is out of range".to_string())
            })?;
            apply_module_action(&mut plan.module_config, action)
        }
    }
}

fn selector_value_to_flow(value: SelectorValue) -> Result<DualVar, RuntimeError> {
    match value {
        SelectorValue::IdWithQuality(value) => Ok(DualVar::Item(value)),
        SelectorValue::Name(value) if !value.is_empty() => Ok(DualVar::Custom { name: value }),
        SelectorValue::Name(_) => Err(RuntimeError::InvalidValue(
            "selector name cannot be empty".to_string(),
        )),
    }
}

fn ensure_unique_target(factory: &FactoryDocument, id: TargetId) -> Result<(), RuntimeError> {
    if factory.targets.iter().any(|target| target.id == id) {
        Err(RuntimeError::DuplicateId("target"))
    } else {
        Ok(())
    }
}

fn ensure_unique_expression(
    factory: &FactoryDocument,
    id: TargetExpressionId,
) -> Result<(), RuntimeError> {
    if factory
        .target_expressions
        .iter()
        .any(|expression| expression.id == id)
    {
        Err(RuntimeError::DuplicateId("target expression"))
    } else {
        Ok(())
    }
}

fn ensure_unique_external(
    factory: &FactoryDocument,
    id: ExternalInputId,
) -> Result<(), RuntimeError> {
    if factory.external_inputs.iter().any(|input| input.id == id) {
        Err(RuntimeError::DuplicateId("external input"))
    } else {
        Ok(())
    }
}

trait HasId {
    type Id;
    fn id(&self) -> Self::Id;
}

fn find_term_mut(
    factory: &mut FactoryDocument,
    expression: TargetExpressionId,
    term: TargetTermId,
) -> Result<&mut TargetTerm, RuntimeError> {
    let expression = factory
        .target_expressions
        .iter_mut()
        .find(|candidate| candidate.id == expression)
        .ok_or(RuntimeError::TargetExpressionNotFound(expression))?;
    expression
        .terms
        .iter_mut()
        .find(|candidate| candidate.id == term)
        .ok_or(RuntimeError::TargetTermNotFound(term))
}

fn find_target_mut(
    items: &mut [FlowTarget],
    id: TargetId,
) -> Result<&mut FlowTarget, RuntimeError> {
    items
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or(RuntimeError::TargetNotFound(id))
}

fn find_expression_mut(
    items: &mut [TargetExpression],
    id: TargetExpressionId,
) -> Result<&mut TargetExpression, RuntimeError> {
    items
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or(RuntimeError::TargetExpressionNotFound(id))
}

fn find_external_mut(
    items: &mut [ExternalInput],
    id: ExternalInputId,
) -> Result<&mut ExternalInput, RuntimeError> {
    items
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or(RuntimeError::ExternalInputNotFound(id))
}

fn index_by_id<T>(items: &[T], id: T::Id) -> Result<usize, RuntimeError>
where
    T: HasId,
    T::Id: Copy + PartialEq,
{
    items
        .iter()
        .position(|item| item.id() == id)
        .ok_or(RuntimeError::InvalidOperation("item was not found"))
}

fn remove_by_id<T>(items: &mut Vec<T>, id: T::Id) -> Result<bool, RuntimeError>
where
    T: HasId,
    T::Id: Copy + PartialEq,
{
    let index = index_by_id(items, id)?;
    items.remove(index);
    Ok(true)
}

impl HasId for FlowTarget {
    type Id = TargetId;

    fn id(&self) -> Self::Id {
        self.id
    }
}

impl HasId for TargetExpression {
    type Id = TargetExpressionId;

    fn id(&self) -> Self::Id {
        self.id
    }
}

impl HasId for ExternalInput {
    type Id = ExternalInputId;

    fn id(&self) -> Self::Id {
        self.id
    }
}

impl HasId for MechanicEntry {
    type Id = MechanicId;

    fn id(&self) -> Self::Id {
        self.id
    }
}

impl HasId for TargetTerm {
    type Id = TargetTermId;

    fn id(&self) -> Self::Id {
        self.id
    }
}

fn next_free_id(factory: &FactoryDocument) -> u64 {
    let mut next = 1;
    for target in &factory.targets {
        next = next.max(target.id.0.saturating_add(1));
    }
    for expression in &factory.target_expressions {
        next = next.max(expression.id.0.saturating_add(1));
        for term in &expression.terms {
            next = next.max(term.id.0.saturating_add(1));
        }
    }
    for input in &factory.external_inputs {
        next = next.max(input.id.0.saturating_add(1));
    }
    next
}

fn move_item<T>(items: &mut Vec<T>, index: usize, position: usize) -> bool {
    if index >= items.len() {
        return false;
    }
    let position = position.min(items.len().saturating_sub(1));
    if index == position {
        return false;
    }
    let item = items.remove(index);
    items.insert(position, item);
    true
}

fn replace<T: PartialEq>(current: &mut T, next: T) -> bool {
    if *current == next {
        false
    } else {
        *current = next;
        true
    }
}

fn non_empty(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

fn validate_finite(name: &str, value: f64) -> Result<(), RuntimeError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(RuntimeError::InvalidValue(format!("{name} must be finite")))
    }
}

fn validate_non_negative(name: &str, value: f64) -> Result<(), RuntimeError> {
    validate_finite(name, value)?;
    if value < 0.0 {
        Err(RuntimeError::InvalidValue(format!(
            "{name} must be non-negative"
        )))
    } else {
        Ok(())
    }
}

fn validate_positive(name: &str, value: f64) -> Result<(), RuntimeError> {
    validate_finite(name, value)?;
    if value <= 0.0 {
        Err(RuntimeError::InvalidValue(format!(
            "{name} must be positive"
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metatorio_core::IdWithQuality;
    use crate::message::{
        AppMessage, FactoryAction, FactoryTemplate, MechanicAction, MechanicListAction,
        ProjectAction,
    };

    fn state_with_factory() -> (RuntimeState, ProjectId, FactoryId) {
        let mut state = RuntimeState::default();
        state
            .dispatch(AppMessage::Application(ApplicationAction::NewProject {
                name: "project".to_string(),
            }))
            .unwrap();
        let project = state.ui.selected_project.unwrap();
        state
            .dispatch(AppMessage::Project {
                project,
                action: ProjectAction::AddFactory {
                    name: "factory".to_string(),
                    template: FactoryTemplate::Empty,
                },
            })
            .unwrap();
        let factory = state.ui.selected_factory.unwrap();
        (state, project, factory)
    }

    #[test]
    fn document_message_changes_state_and_schedules_side_effects() {
        let (mut state, project, factory) = state_with_factory();
        let result = state
            .dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::MechanicList(MechanicListAction::Add {
                    kind: MechanicKind::Recipe,
                }),
            })
            .unwrap();
        assert!(result.changed);
        assert!(result.commands.contains(&RuntimeCommand::Persist {
            project,
            path: None,
        }));
        assert!(
            result
                .commands
                .contains(&RuntimeCommand::Recompute { project, factory })
        );
        assert_eq!(state.factory(project, factory).unwrap().mechanics.len(), 1);
    }

    #[test]
    fn document_changes_request_quality_limit_check() {
        let (mut state, project, factory) = state_with_factory();
        let result = state
            .dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::Flow(FlowAction::AddToTarget {
                    flow: DualVar::Item(IdWithQuality::new("iron-plate", "uncommon")),
                    amount: 1.0,
                }),
            })
            .unwrap();
        assert!(result
            .commands
            .contains(&RuntimeCommand::EnsureQualityLimit { project }));
    }

    #[test]
    fn enumerated_module_removal_requires_exact_quality_match() {
        // 用户报告 bug：添加带品质的枚举插件后永远删不掉。根因是前端
        // removeEnumeratedModule 硬编码 quality="normal"，与存储的
        // IdWithQuality 精确匹配失败。本测试固化 runtime 语义：
        // RemoveEnumeratedModule 按完整 IdWithQuality 匹配（用 normal
        // 删 legendary 必须失败，用同品质删除必须成功）。
        let (mut state, project, _factory) = state_with_factory();
        let legendary = IdWithQuality::new("efficiency-module-3", "legendary");
        let normal = IdWithQuality::new("efficiency-module-3", "normal");
        let planning = |state: &mut RuntimeState, action| {
            state
                .dispatch(AppMessage::Project {
                    project,
                    action: ProjectAction::Planning(action),
                })
                .unwrap()
        };
        let modules = |state: &RuntimeState| {
            state
                .project(project)
                .unwrap()
                .planning
                .enumerate_modules
                .clone()
        };

        // 添加带品质的插件（applyBestModules 用主品质添加的场景）
        assert!(planning(&mut state, PlanningAction::AddEnumeratedModule {
            module: legendary.clone(),
        })
        .changed);
        assert!(modules(&state).contains(&legendary));
        // 重复添加同品质被拒
        assert!(!planning(&mut state, PlanningAction::AddEnumeratedModule {
            module: legendary.clone(),
        })
        .changed);

        // 前端旧逻辑：用 normal 删除 → 精确匹配失败，删不掉
        assert!(!planning(&mut state, PlanningAction::RemoveEnumeratedModule {
            module: normal.clone(),
        })
        .changed);
        assert!(
            modules(&state).contains(&legendary),
            "normal 品质删除不应影响 legendary 条目"
        );

        // 修复后的前端：用完整品质删除 → 成功
        assert!(planning(&mut state, PlanningAction::RemoveEnumeratedModule {
            module: legendary.clone(),
        })
        .changed);
        assert!(!modules(&state).contains(&legendary));
    }

    #[test]
    fn wrong_mechanic_action_is_rejected_without_mutating_the_document() {
        let (mut state, project, factory) = state_with_factory();
        state
            .dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::MechanicList(MechanicListAction::Add {
                    kind: MechanicKind::Mining,
                }),
            })
            .unwrap();
        let mechanic = state.factory(project, factory).unwrap().mechanics[0].id;
        let before = state.clone();
        let error = state.dispatch(AppMessage::Factory {
            project,
            factory,
            action: FactoryAction::Mechanic {
                mechanic,
                action: MechanicAction::Recipe(RecipeMechanicAction::SetRecipe {
                    recipe: IdWithQuality::new("iron-plate", "normal"),
                }),
            },
        });
        assert!(matches!(error, Err(RuntimeError::InvalidOperation(_))));
        assert_eq!(state.document, before.document);
        assert_eq!(state.revision, before.revision);
    }

    #[test]
    fn target_and_module_actions_use_stable_ids_and_validate_indices() {
        let (mut state, project, factory) = state_with_factory();
        state
            .dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::Flow(FlowAction::AddToTarget {
                    flow: DualVar::Electricity,
                    amount: 100.0,
                }),
            })
            .unwrap();
        let target = state.factory(project, factory).unwrap().targets[0].id;
        state
            .dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::Target(TargetAction::SetAmount {
                    target,
                    amount: 200.0,
                }),
            })
            .unwrap();
        assert_eq!(
            state.factory(project, factory).unwrap().targets[0].amount,
            200.0
        );

        state
            .dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::MechanicList(MechanicListAction::Add {
                    kind: MechanicKind::Recipe,
                }),
            })
            .unwrap();
        let mechanic = state.factory(project, factory).unwrap().mechanics[0].id;
        state
            .dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::Mechanic {
                    mechanic,
                    action: MechanicAction::Recipe(RecipeMechanicAction::Module(
                        ModuleAction::SetModuleSlot {
                            slot: 0,
                            module: Some(IdWithQuality::new("speed-module-1", "normal")),
                        },
                    )),
                },
            })
            .unwrap();
        let Mechanic::Recipe(recipe) =
            &state.factory(project, factory).unwrap().mechanics[0].mechanic
        else {
            panic!("expected recipe mechanic");
        };
        assert_eq!(recipe.module_config.modules.len(), 1);
        assert_eq!(recipe.module_config.modules[0].id, "speed-module-1");

        let error = state.dispatch(AppMessage::Factory {
            project,
            factory,
            action: FactoryAction::Mechanic {
                mechanic,
                action: MechanicAction::Recipe(RecipeMechanicAction::Module(
                    ModuleAction::RemoveBeacon { beacon: 1 },
                )),
            },
        });
        assert!(matches!(error, Err(RuntimeError::InvalidValue(_))));
    }

    #[test]
    fn add_beacon_requires_valid_and_unique_beacon() {
        let (mut state, project, factory) = state_with_factory();
        state
            .dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::MechanicList(MechanicListAction::Add {
                    kind: MechanicKind::Recipe,
                }),
            })
            .unwrap();
        let mechanic = state.factory(project, factory).unwrap().mechanics[0].id;
        let module = |action: ModuleAction| AppMessage::Factory {
            project,
            factory,
            action: FactoryAction::Mechanic {
                mechanic,
                action: MechanicAction::Recipe(RecipeMechanicAction::Module(action)),
            },
        };

        // 空 id 拒绝
        let error = state.dispatch(module(ModuleAction::AddBeacon {
            beacon: IdWithQuality::default(),
        }));
        assert!(matches!(error, Err(RuntimeError::InvalidValue(_))));

        // 合法添加：默认 count=1 / share=1.0
        state
            .dispatch(module(ModuleAction::AddBeacon {
                beacon: IdWithQuality::new("beacon", "normal"),
            }))
            .unwrap();
        let Mechanic::Recipe(recipe) =
            &state.factory(project, factory).unwrap().mechanics[0].mechanic
        else {
            panic!("expected recipe mechanic");
        };
        assert_eq!(recipe.module_config.beacons.len(), 1);
        assert_eq!(recipe.module_config.beacons[0].count, 1);
        assert_eq!(recipe.module_config.beacons[0].share, 1.0);

        // 重复信标（同 id 同品质）拒绝
        let error = state.dispatch(module(ModuleAction::AddBeacon {
            beacon: IdWithQuality::new("beacon", "normal"),
        }));
        assert!(matches!(error, Err(RuntimeError::InvalidValue(_))));

        // 同种信标不同品质：允许（重复按 IdWithQuality 判等，不是按 name）
        state
            .dispatch(module(ModuleAction::AddBeacon {
                beacon: IdWithQuality::new("beacon", "uncommon"),
            }))
            .unwrap();
        let Mechanic::Recipe(recipe) =
            &state.factory(project, factory).unwrap().mechanics[0].mechanic
        else {
            panic!("expected recipe mechanic");
        };
        assert_eq!(recipe.module_config.beacons.len(), 2);

        // SetBeacon 换到另一个已有信标（同 id 同品质）拒绝；
        // 换成同种但不同品质允许；换成新信标允许
        let error = state.dispatch(module(ModuleAction::SetBeacon {
            beacon: 1,
            value: IdWithQuality::new("beacon", "normal"),
        }));
        assert!(matches!(error, Err(RuntimeError::InvalidValue(_))));
        state
            .dispatch(module(ModuleAction::SetBeacon {
                beacon: 1,
                value: IdWithQuality::new("beacon", "rare"),
            }))
            .unwrap();
        state
            .dispatch(module(ModuleAction::SetBeacon {
                beacon: 1,
                value: IdWithQuality::new("another-beacon", "normal"),
            }))
            .unwrap();
    }

    #[test]
    fn fluid_fuel_and_fluid_heat_actions_validate_kind_and_value() {
        let (mut state, project, factory) = state_with_factory();
        let add_mechanic = |state: &mut RuntimeState, kind: MechanicKind| {
            state
                .dispatch(AppMessage::Factory {
                    project,
                    factory,
                    action: FactoryAction::MechanicList(MechanicListAction::Add { kind }),
                })
                .unwrap();
            state
                .factory(project, factory)
                .unwrap()
                .mechanics
                .last()
                .unwrap()
                .id
        };
        let act = |state: &mut RuntimeState, mechanic, action: MechanicAction| {
            state.dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::Mechanic { mechanic, action },
            })
        };

        let mechanic = add_mechanic(&mut state, MechanicKind::FluidFuel);
        // 流体燃料：设置流体与温度
        act(
            &mut state,
            mechanic,
            MechanicAction::FluidFuel(FluidFuelMechanicAction::SetFluid {
                fluid: "rocket-fuel".to_string(),
            }),
        )
        .unwrap();
        act(
            &mut state,
            mechanic,
            MechanicAction::FluidFuel(FluidFuelMechanicAction::SetTemperature {
                temperature: Some(25),
            }),
        )
        .unwrap();
        let Mechanic::FluidFuel(fuel) =
            &state.factory(project, factory).unwrap().mechanics[0].mechanic
        else {
            panic!("expected fluid-fuel mechanic");
        };
        assert_eq!(fuel.fluid, "rocket-fuel");
        assert_eq!(fuel.temperature, Some(25));

        // 空流体拒绝
        let mechanic = add_mechanic(&mut state, MechanicKind::FluidHeat);
        let error = act(
            &mut state,
            mechanic,
            MechanicAction::FluidHeat(FluidHeatMechanicAction::SetFluid {
                fluid: String::new(),
            }),
        );
        assert!(matches!(error, Err(RuntimeError::InvalidValue(_))));

        // 种类不匹配拒绝：对 fluid-heat 机制发 fluid-fuel 动作
        let error = act(
            &mut state,
            mechanic,
            MechanicAction::FluidFuel(FluidFuelMechanicAction::SetTemperature {
                temperature: None,
            }),
        );
        assert!(matches!(error, Err(RuntimeError::InvalidOperation(_))));
    }

    #[test]
    fn solar_actions_set_panel_and_accumulator() {
        let (mut state, project, factory) = state_with_factory();
        state
            .dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::MechanicList(MechanicListAction::Add {
                    kind: MechanicKind::Solar,
                }),
            })
            .unwrap();
        let mechanic = state.factory(project, factory).unwrap().mechanics[0].id;
        let act = |state: &mut RuntimeState, action: MechanicAction| {
            state.dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::Mechanic { mechanic, action },
            })
        };
        act(
            &mut state,
            MechanicAction::Solar(SolarMechanicAction::SetSolarPanel {
                solar_panel: IdWithQuality::new("solar-panel", "normal"),
            }),
        )
        .unwrap();
        act(
            &mut state,
            MechanicAction::Solar(SolarMechanicAction::SetAccumulator {
                accumulator: IdWithQuality::new("accumulator", "normal"),
            }),
        )
        .unwrap();
        let Mechanic::Solar(solar) =
            &state.factory(project, factory).unwrap().mechanics[0].mechanic
        else {
            panic!("expected solar mechanic");
        };
        assert_eq!(solar.solar_panel.id, "solar-panel");
        assert_eq!(solar.accumulator.id, "accumulator");

        // 种类不匹配拒绝：对 solar 机制发 reactor 动作
        let error = act(
            &mut state,
            MechanicAction::Reactor(ReactorMechanicAction::SetReactor {
                reactor: IdWithQuality::new("nuclear-reactor", "normal"),
            }),
        );
        assert!(matches!(error, Err(RuntimeError::InvalidOperation(_))));
    }
}

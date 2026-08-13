// Svelte 5 rune-based store bridging the frontend to the Tauri runtime.
//
// `runtime` is a singleton: the demo page (and later the real UI) reads its
// $state fields reactively and calls its methods to send AppMessages.  After
// every dispatch the store refreshes the document + UI snapshots, so the
// UI never keeps its own copy of backend data.

import {
  dispatch,
  getDocument,
  getUiState,
  loadBundledDump,
  onSolveError,
  onSolveResult,
} from "./client";
import type {
  AppMessage,
  FactoryId,
  MechanicId,
  MechanicKind,
  ProjectId,
  SolveResult,
} from "./types";

class RuntimeStore {
  document = $state<import("./types").AppDocument | null>(null);
  ui = $state<import("./types").UiState | null>(null);
  solve = $state<SolveResult | null>(null);
  solveError = $state<string | null>(null);
  revision = $state(0);
  busy = $state(false);
  solving = $state(false);
  lastError = $state<string | null>(null);
  ready = $state(false);

  /** Subscribe to backend events; call once at app start. */
  async init(): Promise<void> {
    onSolveResult((result) => {
      this.solve = result;
      this.solveError = null;
      this.solving = false;
    });
    onSolveError((message) => {
      this.solveError = message;
      this.solving = false;
    });
    try {
      await this.refresh();
    } catch (error) {
      this.lastError = String(error);
    }
    this.ready = true;
  }

  async refresh(): Promise<void> {
    const [document, ui] = await Promise.all([getDocument(), getUiState()]);
    this.document = document;
    this.ui = ui;
  }

  /** Send one AppMessage to the Rust runtime and refresh the snapshot. */
  async send(message: AppMessage): Promise<void> {
    this.busy = true;
    this.lastError = null;
    try {
      const result = await dispatch(message);
      this.revision = result.revision;
      await this.refresh();
    } catch (error) {
      this.lastError = String(error);
      throw error;
    } finally {
      this.busy = false;
    }
  }

  async loadDemoData(): Promise<void> {
    this.busy = true;
    this.lastError = null;
    try {
      await loadBundledDump();
      this.solveError = null;
    } catch (error) {
      this.lastError = String(error);
    } finally {
      this.busy = false;
    }
  }

  // ── Demo actions (grow into the real UI) ───────────────────────

  async newProject(name: string): Promise<void> {
    await this.send({ scope: "application", action: { "new-project": { name } } });
  }

  async addFactory(name: string): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "add-factory": { name, template: "empty" } } },
    });
  }

  async addMechanic(kind: MechanicKind): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { "mechanic-list": { add: { kind } } } },
    });
  }

  async setRecipe(mechanic: MechanicId, recipe: string): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: {
          mechanic: {
            mechanic,
            action: { "set-recipe": { recipe: { id: recipe, quality: "normal" } } },
          },
        },
      },
    });
  }

  async setMachine(mechanic: MechanicId, machine: string): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: {
          mechanic: {
            mechanic,
            action: { "set-machine": { machine: { id: machine, quality: "normal" } } },
          },
        },
      },
    });
  }

  async addTarget(itemId: string, amount: number): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: {
          flow: {
            "add-to-target": { flow: { Item: { id: itemId, quality: "normal" } }, amount },
          },
        },
      },
    });
  }

  async recompute(): Promise<void> {
    const { project, factory } = this.requireFactory();
    this.solving = true;
    try {
      await this.send({
        scope: "factory",
        action: { project, factory, action: { solve: "recompute" } },
      });
    } catch (error) {
      this.solving = false;
      throw error;
    }
  }

  // ── Derived helpers for the demo page ──────────────────────────

  get selectedProject() {
    return (
      this.document?.projects.find((project) => project.id === this.ui?.selected_project) ?? null
    );
  }

  get selectedFactory() {
    const project = this.selectedProject;
    return (
      project?.factories.find((factory) => factory.id === this.ui?.selected_factory) ?? null
    );
  }

  private requireProject(): ProjectId {
    const project = this.ui?.selected_project;
    if (project == null) throw new Error("没有选中的项目：先新建项目");
    return project;
  }

  private requireFactory(): { project: ProjectId; factory: FactoryId } {
    const project = this.requireProject();
    const factory = this.ui?.selected_factory;
    if (factory == null) throw new Error("没有选中的工厂：先新建工厂");
    return { project, factory };
  }
}

export const runtime = new RuntimeStore();

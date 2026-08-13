<script lang="ts">
  type MechanicKind = "Recipe" | "Mining" | "Generator" | "Boiler";
  type SelectorMode = "target" | "recipe" | "machine";
  type SolveState = "solved" | "running";
  type FlowTone = "input" | "output" | "neutral";

  type FlowRow = {
    label: string;
    amount: string;
    tone: FlowTone;
    meta?: string;
  };

  type Mechanic = {
    id: number;
    kind: MechanicKind;
    title: string;
    icon: string;
    recipe: string;
    recipeMeta: string;
    machine: string;
    machineMeta: string;
    quality: string;
    modules: string;
    rate: string;
    machineCount: string;
    expanded: boolean;
    warning?: string;
    inputs: FlowRow[];
    outputs: FlowRow[];
  };

  type CatalogEntry = {
    id: string;
    label: string;
    detail: string;
    type: "item" | "recipe" | "machine";
    icon: string;
    group: string;
  };

  const mechanicKinds: { kind: MechanicKind; label: string; detail: string }[] = [
    { kind: "Recipe", label: "Recipe", detail: "Craft an item or fluid" },
    { kind: "Mining", label: "Mining", detail: "Extract a resource" },
    { kind: "Generator", label: "Generator", detail: "Turn fluid into power" },
    { kind: "Boiler", label: "Boiler", detail: "Raise a fluid temperature" },
  ];

  const catalog: CatalogEntry[] = [
    {
      id: "processing-unit",
      label: "Processing unit",
      detail: "Intermediate product",
      type: "item",
      icon: "CPU",
      group: "Intermediate products",
    },
    {
      id: "iron-plate",
      label: "Iron plate",
      detail: "Smelting product",
      type: "item",
      icon: "Fe",
      group: "Raw materials",
    },
    {
      id: "copper-cable",
      label: "Copper cable",
      detail: "Intermediate product",
      type: "item",
      icon: "Cu",
      group: "Intermediate products",
    },
    {
      id: "electronic-circuit",
      label: "Electronic circuit",
      detail: "Intermediate product",
      type: "item",
      icon: "EC",
      group: "Intermediate products",
    },
    {
      id: "processing-unit-recipe",
      label: "Processing unit",
      detail: "Electronic circuit + cable + sulfuric acid",
      type: "recipe",
      icon: "CPU",
      group: "Intermediate products",
    },
    {
      id: "electronic-circuit-recipe",
      label: "Electronic circuit",
      detail: "Iron plate + copper cable",
      type: "recipe",
      icon: "EC",
      group: "Intermediate products",
    },
    {
      id: "assembling-machine-3",
      label: "Assembling machine 3",
      detail: "Crafting machine / 4 module slots",
      type: "machine",
      icon: "ASM",
      group: "Assembling machines",
    },
    {
      id: "assembling-machine-2",
      label: "Assembling machine 2",
      detail: "Crafting machine / 2 module slots",
      type: "machine",
      icon: "ASM",
      group: "Assembling machines",
    },
  ];

  let activeTab = $state("Nauvis starter");
  let activeView = $state<"factory" | "results">("factory");
  let selectedId = $state(1);
  let selectorOpen = $state(false);
  let selectorMode = $state<SelectorMode>("target");
  let selectorOwner = $state<number | null>(null);
  let search = $state("");
  let showAddMenu = $state(false);
  let solveState = $state<SolveState>("solved");
  let lastAction = $state("All flows balanced");
  let solvedAt = $state("just now");
  let target = $state({
    id: "processing-unit",
    label: "Processing unit",
    icon: "CPU",
    rate: "60",
    quality: "normal",
  });
  let externalInputs = $state([
    { label: "Iron ore", icon: "Fe", amount: "120 /s", cost: "external" },
    { label: "Copper ore", icon: "Cu", amount: "180 /s", cost: "external" },
  ]);
  let mechanics = $state<Mechanic[]>([
    {
      id: 1,
      kind: "Recipe",
      title: "Processing unit",
      icon: "CPU",
      recipe: "processing-unit",
      recipeMeta: "Electronic circuit + cable + acid",
      machine: "assembling-machine-3",
      machineMeta: "4 module slots",
      quality: "normal",
      modules: "4 x Speed module 3",
      rate: "60.0 /s",
      machineCount: "30.0",
      expanded: true,
      inputs: [
        { label: "Electronic circuit", amount: "-120 /s", tone: "input", meta: "normal" },
        { label: "Sulfuric acid", amount: "-10 /s", tone: "input", meta: "25 C" },
      ],
      outputs: [{ label: "Processing unit", amount: "+60 /s", tone: "output", meta: "normal" }],
    },
    {
      id: 2,
      kind: "Recipe",
      title: "Electronic circuit",
      icon: "EC",
      recipe: "electronic-circuit",
      recipeMeta: "Iron plate + copper cable",
      machine: "assembling-machine-3",
      machineMeta: "4 module slots",
      quality: "normal",
      modules: "4 x Productivity module 3",
      rate: "120.0 /s",
      machineCount: "18.4",
      expanded: true,
      inputs: [
        { label: "Iron plate", amount: "-120 /s", tone: "input", meta: "normal" },
        { label: "Copper cable", amount: "-240 /s", tone: "input", meta: "normal" },
      ],
      outputs: [{ label: "Electronic circuit", amount: "+120 /s", tone: "output", meta: "normal" }],
    },
    {
      id: 3,
      kind: "Mining",
      title: "Copper ore extraction",
      icon: "Cu",
      recipe: "copper-ore",
      recipeMeta: "Resource / copper",
      machine: "electric-mining-drill",
      machineMeta: "Mining speed 2.5",
      quality: "normal",
      modules: "No modules",
      rate: "240.0 /s",
      machineCount: "12.0",
      expanded: false,
      warning: "Surface selection is inherited from project settings",
      inputs: [],
      outputs: [{ label: "Copper ore", amount: "+240 /s", tone: "output", meta: "Nauvis" }],
    },
  ]);

  let selectedMechanic = $derived(mechanics.find((mechanic) => mechanic.id === selectedId));
  let totalMachines = $derived(mechanics.reduce((sum, mechanic) => sum + Number(mechanic.machineCount), 0));
  let visibleCatalog = $derived.by(() => {
    const query = search.trim().toLowerCase();
    const requiredType = selectorMode === "target" ? "item" : selectorMode;
    return catalog.filter((entry) => {
      const matchesType = entry.type === requiredType;
      const matchesQuery =
        query.length === 0 ||
        entry.label.toLowerCase().includes(query) ||
        entry.id.toLowerCase().includes(query) ||
        entry.detail.toLowerCase().includes(query);
      return matchesType && matchesQuery;
    });
  });

  function openSelector(mode: SelectorMode, owner: number | null = null) {
    selectorMode = mode;
    selectorOwner = owner;
    search = "";
    selectorOpen = true;
  }

  function closeSelector() {
    selectorOpen = false;
    selectorOwner = null;
  }

  function chooseCatalog(entry: CatalogEntry) {
    if (selectorMode === "target") {
      target = { ...target, id: entry.id, label: entry.label, icon: entry.icon };
      lastAction = `Target changed to ${entry.label}`;
    } else if (selectorOwner !== null) {
      const mechanic = mechanics.find((candidate) => candidate.id === selectorOwner);
      if (mechanic) {
        if (selectorMode === "recipe") {
          mechanic.recipe = entry.id.replace(/-recipe$/, "");
          mechanic.title = entry.label;
          mechanic.icon = entry.icon;
          mechanic.recipeMeta = entry.detail;
        } else {
          mechanic.machine = entry.id;
          mechanic.machineMeta = entry.detail;
        }
        lastAction = `${entry.label} selected for mechanic #${mechanic.id}`;
      }
    }
    closeSelector();
  }

  function selectMechanic(id: number) {
    selectedId = id;
  }

  function toggleMechanic(id: number) {
    const mechanic = mechanics.find((candidate) => candidate.id === id);
    if (mechanic) mechanic.expanded = !mechanic.expanded;
    selectedId = id;
  }

  function addMechanic(kind: MechanicKind) {
    const id = Math.max(...mechanics.map((mechanic) => mechanic.id), 0) + 1;
    const isMining = kind === "Mining";
    const isGenerator = kind === "Generator";
    const isBoiler = kind === "Boiler";
    const mechanic: Mechanic = {
      id,
      kind,
      title: isMining ? "Iron ore extraction" : isGenerator ? "Steam power" : isBoiler ? "Steam boiler" : "New recipe",
      icon: isMining ? "Fe" : isGenerator ? "PW" : isBoiler ? "HT" : "NEW",
      recipe: isMining ? "iron-ore" : isGenerator ? "steam" : isBoiler ? "water" : "iron-plate",
      recipeMeta: isMining ? "Resource / iron" : isGenerator ? "Fluid input" : isBoiler ? "Heating profile" : "Choose a recipe",
      machine: isMining ? "electric-mining-drill" : isGenerator ? "steam-turbine" : isBoiler ? "boiler" : "assembling-machine-2",
      machineMeta: "Choose a machine",
      quality: "normal",
      modules: "No modules",
      rate: "0.0 /s",
      machineCount: "0.0",
      expanded: true,
      inputs: [],
      outputs: [],
    };
    mechanics.push(mechanic);
    selectedId = id;
    showAddMenu = false;
    lastAction = `${kind} mechanic #${id} added as a draft`;
  }

  function removeSelected() {
    if (mechanics.length <= 1) return;
    const index = mechanics.findIndex((mechanic) => mechanic.id === selectedId);
    mechanics = mechanics.filter((mechanic) => mechanic.id !== selectedId);
    selectedId = mechanics[Math.max(0, index - 1)]?.id ?? mechanics[0].id;
    lastAction = "Mechanic removed from the document";
  }

  function runSolve() {
    if (solveState === "running") return;
    solveState = "running";
    lastAction = "Recomputing from the latest document...";
    window.setTimeout(() => {
      solveState = "solved";
      solvedAt = "just now";
      lastAction = "All flows balanced";
    }, 650);
  }
</script>

<svelte:head>
  <title>Metatorio / Production Workbench</title>
  <meta
    name="description"
    content="A production planning workbench for Factorio factories."
  />
</svelte:head>

<div class="app-shell">
  <header class="topbar">
    <div class="brand-lockup">
      <div class="brand-mark" aria-hidden="true"><span></span><span></span><span></span></div>
      <div>
        <div class="brand-name">METATORIO</div>
        <div class="brand-caption">production systems lab</div>
      </div>
    </div>

    <nav class="project-tabs" aria-label="Factory projects">
      {#each ["Nauvis starter", "Space platform"] as tab}
        <button class:active={activeTab === tab} class="project-tab" onclick={() => (activeTab = tab)}>
          <span class="tab-dot"></span>
          {tab}
        </button>
      {/each}
      <button class="icon-button" aria-label="Add factory project" title="Add factory project">+</button>
    </nav>

    <div class="topbar-actions">
      <span class="prototype-chip">FRONTEND PROTOTYPE</span>
      <a class="demo-link" href="/demo">RUNTIME DEMO</a>
      <span class:running={solveState === "running"} class="save-state">
        <span class="status-dot"></span>
        {solveState === "running" ? "Solving" : "Saved locally"}
      </span>
      <button class="quiet-button" aria-label="Undo" title="Undo">UNDO</button>
      <button class="quiet-button" aria-label="Redo" title="Redo">REDO</button>
      <button class="avatar-button" aria-label="Open settings">M</button>
    </div>
  </header>

  <div class="workspace-heading">
    <div>
      <div class="eyebrow">FACTORY / {activeTab.toUpperCase()}</div>
      <h1>Processing unit line</h1>
    </div>
    <div class="heading-actions">
      <span class="revision-label">REV 014 <span class="separator">/</span> {solvedAt}</span>
      <button class="primary-button" onclick={runSolve} disabled={solveState === "running"}>
        <span class="button-pulse"></span>
        {solveState === "running" ? "Solving..." : "Recompute"}
      </button>
    </div>
  </div>

  <div class="view-switcher" role="tablist" aria-label="Workspace views">
    <button class:active={activeView === "factory"} role="tab" aria-selected={activeView === "factory"} onclick={() => (activeView = "factory")}>
      Factory editor
    </button>
    <button class:active={activeView === "results"} role="tab" aria-selected={activeView === "results"} onclick={() => (activeView = "results")}>
      Flow ledger <span class="count-pill">12</span>
    </button>
  </div>

  <main class="workspace">
    <aside class="goals-panel panel-surface">
      <div class="panel-heading">
        <div>
          <div class="eyebrow">PROJECT INPUT</div>
          <h2>Targets</h2>
        </div>
        <button class="panel-action" aria-label="Target panel options" title="Target panel options">...</button>
      </div>

      <section class="target-section">
        <div class="section-label-row">
          <span class="section-label">NET OUTPUT</span>
          <span class="section-note">per second</span>
        </div>
        <button class="target-card" onclick={() => openSelector("target")}>
          <span class="item-icon large cyan">{target.icon}</span>
          <span class="target-copy">
            <strong>{target.label}</strong>
            <small>{target.quality} quality</small>
          </span>
          <span class="target-rate"><strong>{target.rate}</strong><small>/ sec</small></span>
          <span class="chevron">></span>
        </button>
        <button class="text-action" onclick={() => openSelector("target")}>+ Add target</button>
      </section>

      <section class="target-section external-section">
        <div class="section-label-row">
          <span class="section-label">EXTERNAL INPUTS</span>
          <span class="section-note">unbounded</span>
        </div>
        {#each externalInputs as input}
          <div class="input-row">
            <span class="item-icon small amber">{input.icon}</span>
            <span class="input-copy"><strong>{input.label}</strong><small>{input.cost}</small></span>
            <span class="input-rate">{input.amount}</span>
            <button class="row-menu" aria-label={`Options for ${input.label}`}>...</button>
          </div>
        {/each}
        <button class="text-action">+ Add external input</button>
      </section>

      <section class="constraints-section">
        <div class="section-label-row"><span class="section-label">CONSTRAINTS</span></div>
        <label class="toggle-row">
          <input type="checkbox" checked />
          <span class="toggle-track"><span></span></span>
          <span><strong>Strict inputs</strong><small>Only listed inputs are allowed</small></span>
        </label>
        <label class="toggle-row">
          <input type="checkbox" />
          <span class="toggle-track"><span></span></span>
          <span><strong>Balance byproducts</strong><small>Do not discard surplus flows</small></span>
        </label>
      </section>

      <div class="panel-footnote">
        <span class="info-mark">i</span>
        <span>Targets are net flows. Intermediate materials are balanced automatically.</span>
      </div>
    </aside>

    <section class="factory-canvas">
      {#if activeView === "factory"}
        <div class="canvas-toolbar">
          <div>
            <span class="eyebrow">ORDERED PRODUCTION CHAIN</span>
            <span class="chain-meta">{mechanics.length} mechanics <span class="separator">/</span> {totalMachines.toFixed(1)} machines</span>
          </div>
          <div class="canvas-actions">
            <button class="sort-button">SORT: MANUAL <span class="chevron">v</span></button>
            <div class="add-wrapper">
              <button class="primary-button compact" onclick={() => (showAddMenu = !showAddMenu)}>+ Add mechanic</button>
              {#if showAddMenu}
                <div class="add-menu">
                  <div class="menu-title">ADD TO FACTORY</div>
                  {#each mechanicKinds as option}
                    <button onclick={() => addMechanic(option.kind)}>
                      <span class="menu-icon">{option.label.slice(0, 2).toUpperCase()}</span>
                      <span><strong>{option.label}</strong><small>{option.detail}</small></span>
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        </div>

        <div class="flow-summary">
          <div class="summary-segment"><span class="summary-key">TARGET</span><strong>+{target.rate} /s</strong><span>{target.label}</span></div>
          <div class="summary-line"></div>
          <div class="summary-segment"><span class="summary-key">POWER</span><strong>1.84 MW</strong><span class="positive">available</span></div>
          <div class="summary-line"></div>
          <div class="summary-segment"><span class="summary-key">COST</span><strong>786.34</strong><span>weighted units</span></div>
        </div>

        <div class="mechanic-stack">
          {#each mechanics as mechanic, index (mechanic.id)}
            <article class:selected={selectedId === mechanic.id} class="mechanic-card">
              <div class="mechanic-header">
                <button class="drag-handle" aria-label={`Reorder mechanic ${mechanic.id}`} title="Drag to reorder">
                  <span></span><span></span><span></span><span></span><span></span><span></span>
                </button>
                <button class="mechanic-title-button" onclick={() => selectMechanic(mechanic.id)}>
                  <span class="step-number">{String(index + 1).padStart(2, "0")}</span>
                  <span class="kind-chip">{mechanic.kind}</span>
                  <span class="mechanic-title">{mechanic.title}</span>
                  {#if mechanic.warning}<span class="warning-mark" title={mechanic.warning}>!</span>{/if}
                </button>
                <div class="mechanic-header-actions">
                  <span class="rate-chip">{mechanic.rate}</span>
                  <button class="icon-button subtle" aria-label={`Toggle mechanic ${mechanic.id}`} onclick={() => toggleMechanic(mechanic.id)}>{mechanic.expanded ? "-" : "+"}</button>
                  <button class="icon-button subtle" aria-label={`More options for mechanic ${mechanic.id}`}>...</button>
                </div>
              </div>

              {#if mechanic.expanded}
                <div class="mechanic-body">
                  <div class="field-grid">
                    <div class="field-block">
                      <span class="field-label">{mechanic.kind === "Recipe" ? "RECIPE" : "RESOURCE"}</span>
                      <button class="picker-button" onclick={() => openSelector("recipe", mechanic.id)}>
                        <span class="item-icon medium cyan">{mechanic.icon}</span>
                        <span class="picker-copy"><strong>{mechanic.recipe}</strong><small>{mechanic.recipeMeta}</small></span>
                        <span class="chevron">></span>
                      </button>
                    </div>
                    <div class="field-block">
                      <span class="field-label">MACHINE</span>
                      <button class="picker-button" onclick={() => openSelector("machine", mechanic.id)}>
                        <span class="item-icon medium steel">ASM</span>
                        <span class="picker-copy"><strong>{mechanic.machine}</strong><small>{mechanic.machineMeta}</small></span>
                        <span class="chevron">></span>
                      </button>
                    </div>
                    <div class="field-block compact-field">
                      <span class="field-label">QUALITY</span>
                      <button class="select-button"><span>{mechanic.quality}</span><span class="chevron">v</span></button>
                    </div>
                    <div class="field-block compact-field">
                      <span class="field-label">MODULES</span>
                      <button class="select-button"><span>{mechanic.modules}</span><span class="chevron">v</span></button>
                    </div>
                  </div>

                  <div class="card-footer">
                    <div class="flow-groups">
                      {#each mechanic.inputs as flow}
                        <div class="flow-pill input-flow"><span class="flow-sign">-</span><span>{flow.label}</span><small>{flow.amount.replace("-", "")}</small></div>
                      {/each}
                      {#each mechanic.outputs as flow}
                        <div class="flow-pill output-flow"><span class="flow-sign">+</span><span>{flow.label}</span><small>{flow.amount.replace("+", "")}</small></div>
                      {/each}
                    </div>
                    <div class="machine-count"><span>RUNNING</span><strong>{mechanic.machineCount}</strong><small>machines</small></div>
                  </div>
                  {#if mechanic.warning}
                    <div class="warning-strip"><span class="warning-mark">!</span>{mechanic.warning}</div>
                  {/if}
                </div>
              {/if}
            </article>
          {/each}
        </div>

        <button class="drop-zone" onclick={() => addMechanic("Recipe")}>
          <span class="drop-plus">+</span>
          <span><strong>Drop a mechanic here</strong><small>or use Add mechanic above</small></span>
        </button>
      {:else}
        <div class="ledger-view">
          <div class="canvas-toolbar">
            <div><span class="eyebrow">SOLVER OUTPUT</span><h2>Flow ledger</h2></div>
            <span class="solved-badge"><span class="status-dot"></span> Optimal solution</span>
          </div>
          <div class="ledger-table">
            <div class="ledger-row ledger-head"><span>FLOW</span><span>NET</span><span>SOURCE</span><span>DESTINATION</span></div>
            <div class="ledger-row"><span class="ledger-flow"><span class="item-icon small cyan">CPU</span><strong>Processing unit</strong></span><strong class="positive">+60.00 /s</strong><span>Mechanic #1</span><span>Target</span></div>
            <div class="ledger-row"><span class="ledger-flow"><span class="item-icon small steel">EC</span><strong>Electronic circuit</strong></span><strong class="positive">+0.00 /s</strong><span>Mechanic #2</span><span>Mechanic #1</span></div>
            <div class="ledger-row"><span class="ledger-flow"><span class="item-icon small amber">Fe</span><strong>Iron plate</strong></span><strong class="negative">-120.00 /s</strong><span>External</span><span>Mechanic #2</span></div>
            <div class="ledger-row"><span class="ledger-flow"><span class="item-icon small copper">Cu</span><strong>Copper ore</strong></span><strong class="negative">-180.00 /s</strong><span>External</span><span>Mechanic #3</span></div>
            <div class="ledger-row"><span class="ledger-flow"><span class="item-icon small blue">H2</span><strong>Sulfuric acid</strong></span><strong class="negative">-10.00 /s</strong><span>External</span><span>Mechanic #1</span></div>
            <div class="ledger-row"><span class="ledger-flow"><span class="item-icon small violet">MW</span><strong>Electricity</strong></span><strong class="negative">-1.84 MW</strong><span>Power network</span><span>Factory</span></div>
          </div>
          <div class="ledger-note"><span class="info-mark">i</span><span>Zero-net intermediate flows are retained here so every balance can be traced back to a mechanism.</span></div>
        </div>
      {/if}
    </section>

    <aside class="inspector panel-surface">
      <div class="panel-heading">
        <div><div class="eyebrow">INSPECTOR</div><h2>Selected unit</h2></div>
        <button class="panel-action" aria-label="Close inspector">x</button>
      </div>
      {#if selectedMechanic}
        <div class="inspector-hero">
          <span class="item-icon large cyan">{selectedMechanic.icon}</span>
          <div><strong>#{selectedMechanic.id} {selectedMechanic.title}</strong><small>{selectedMechanic.kind} mechanic</small></div>
        </div>
        <div class="inspector-section">
          <div class="section-label-row"><span class="section-label">PERFORMANCE</span><span class="section-note">solved</span></div>
          <div class="metric-grid">
            <div><span>RATE</span><strong>{selectedMechanic.rate}</strong></div>
            <div><span>MACHINES</span><strong>{selectedMechanic.machineCount}</strong></div>
            <div><span>QUALITY</span><strong>{selectedMechanic.quality}</strong></div>
            <div><span>POWER</span><strong>61.4 kW</strong></div>
          </div>
        </div>
        <div class="inspector-section">
          <div class="section-label-row"><span class="section-label">TEMPERATURE VARIANTS</span><span class="section-note">range model</span></div>
          <div class="temperature-card"><span class="temperature-line"><span class="temperature-dot"></span><strong>25 C</strong><span>selected endpoint</span></span><span class="temperature-bar"><i></i></span><span class="temperature-range"><span>15 C</span><span>500 C</span></span></div>
          <p class="helper-text">Fluid temperature is kept with the expanded variable. The solver may choose a different endpoint when the document changes.</p>
        </div>
        <div class="inspector-section">
          <div class="section-label-row"><span class="section-label">FLOW CONTRIBUTION</span></div>
          <div class="contribution-list">
            {#each [...selectedMechanic.inputs, ...selectedMechanic.outputs] as flow}
              <div class="contribution-row"><span class:output={flow.tone === "output"} class="contribution-mark"></span><span>{flow.label}</span><strong class:positive={flow.tone === "output"}>{flow.amount}</strong></div>
            {/each}
          </div>
        </div>
        <button class="danger-button" onclick={removeSelected} disabled={mechanics.length <= 1}>Remove mechanic</button>
      {:else}
        <div class="empty-inspector"><span class="empty-mark">--</span><strong>Select a mechanic</strong><small>Its rate, variants and flow contribution will appear here.</small></div>
      {/if}
    </aside>
  </main>

  <footer class="statusbar">
    <div class="status-message"><span class:running={solveState === "running"} class="status-dot"></span><span>{lastAction}</span></div>
    <div class="status-details"><span>DATASET <strong>VANILLA 2.1</strong></span><span>QUALITY <strong>NORMAL - RARE</strong></span><span>LP <strong>MICROLP</strong></span></div>
  </footer>
</div>

{#if selectorOpen}
  <div class="modal-backdrop" role="presentation" onclick={closeSelector} onkeydown={(event) => event.key === "Escape" && closeSelector()}>
    <div class="selector-modal" role="dialog" aria-modal="true" aria-label={`Select ${selectorMode}`} tabindex="-1" onclick={(event) => event.stopPropagation()} onkeydown={(event) => event.key === "Escape" && closeSelector()}>
      <div class="selector-heading">
        <div><div class="eyebrow">CATALOG SELECTOR</div><h2>Select {selectorMode}</h2><p>Choose a canonical prototype. The project stores its ID, not this label.</p></div>
        <button class="icon-button" aria-label="Close selector" onclick={closeSelector}>x</button>
      </div>
      <label class="search-box">
        <span class="search-mark">/</span>
        <input bind:value={search} placeholder={`Search ${selectorMode}s...`} aria-label={`Search ${selectorMode}s`} />
        <kbd>ESC</kbd>
      </label>
      <div class="selector-filters"><button class="filter-chip active">All groups</button><button class="filter-chip">Intermediate products</button><button class="filter-chip">Raw materials</button></div>
      <div class="catalog-list">
        {#each visibleCatalog as entry}
          <button class="catalog-row" onclick={() => chooseCatalog(entry)}>
            <span class="item-icon medium" class:cyan={entry.type === "item"} class:steel={entry.type === "machine"} class:amber={entry.type === "recipe"}>{entry.icon}</span>
            <span class="catalog-copy"><strong>{entry.label}</strong><small>{entry.detail}</small></span>
            <span class="catalog-group">{entry.group}</span>
            <span class="chevron">></span>
          </button>
        {:else}
          <div class="no-results"><span class="empty-mark">--</span><strong>No matching prototypes</strong><small>Try a shorter search term.</small></div>
        {/each}
      </div>
      <div class="selector-footer"><span><kbd>UP</kbd><kbd>DOWN</kbd> navigate</span><span><kbd>ENTER</kbd> select</span><button class="quiet-button" onclick={closeSelector}>CANCEL</button></div>
    </div>
  </div>
{/if}

<style>
  :global(*) {
    box-sizing: border-box;
  }

  :global(html) {
    background: #0d1012;
  }

  :global(body) {
    margin: 0;
    min-width: 320px;
    color: #e8ecea;
    background: #0d1012;
    font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
    font-size: 13px;
    font-weight: 450;
    letter-spacing: 0.01em;
    -webkit-font-smoothing: antialiased;
  }

  :global(button),
  :global(input) {
    font: inherit;
  }

  :global(button) {
    border: 0;
  }

  .app-shell {
    min-height: 100vh;
    background:
      linear-gradient(115deg, rgba(41, 64, 58, 0.08), transparent 30%),
      #0d1012;
  }

  .topbar {
    min-height: 64px;
    display: flex;
    align-items: center;
    gap: 28px;
    padding: 0 28px;
    border-bottom: 1px solid #252c2d;
    background: rgba(15, 19, 20, 0.94);
  }

  .brand-lockup {
    display: flex;
    align-items: center;
    gap: 11px;
    flex: 0 0 auto;
  }

  .brand-mark {
    width: 26px;
    height: 26px;
    display: flex;
    align-items: flex-end;
    gap: 3px;
    padding: 4px;
    border: 1px solid #7ae2c0;
    background: #162822;
    transform: skewY(-12deg);
  }

  .brand-mark span {
    display: block;
    width: 4px;
    background: #7ae2c0;
  }

  .brand-mark span:nth-child(1) { height: 8px; opacity: 0.45; }
  .brand-mark span:nth-child(2) { height: 13px; opacity: 0.72; }
  .brand-mark span:nth-child(3) { height: 17px; }

  .brand-name {
    color: #eef6f2;
    font-size: 13px;
    font-weight: 750;
    letter-spacing: 0.18em;
  }

  .brand-caption,
  .eyebrow,
  .field-label,
  .section-label,
  .summary-key,
  .ledger-head,
  .metric-grid span,
  .machine-count span,
  .menu-title {
    color: #72807c;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.13em;
    text-transform: uppercase;
  }

  .brand-caption {
    margin-top: 2px;
    font-size: 8px;
    letter-spacing: 0.09em;
  }

  .project-tabs {
    display: flex;
    align-self: stretch;
    align-items: stretch;
    gap: 4px;
    min-width: 0;
  }

  .project-tab {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 0 15px;
    color: #7e8986;
    background: transparent;
    border-bottom: 2px solid transparent;
    cursor: pointer;
    white-space: nowrap;
  }

  .project-tab:hover,
  .project-tab.active {
    color: #e5efeb;
    background: #151b1b;
    border-bottom-color: #7ae2c0;
  }

  .tab-dot,
  .status-dot {
    width: 6px;
    height: 6px;
    display: inline-block;
    border-radius: 50%;
    background: #6a7773;
  }

  .project-tab.active .tab-dot,
  .status-dot {
    background: #7ae2c0;
    box-shadow: 0 0 0 3px rgba(122, 226, 192, 0.08);
  }

  .topbar-actions {
    display: flex;
    align-items: center;
    gap: 13px;
    margin-left: auto;
    white-space: nowrap;
  }

  .prototype-chip {
    padding: 4px 7px;
    color: #c8a875;
    border: 1px solid #5a4930;
    background: #211c14;
    font-size: 8px;
    font-weight: 750;
    letter-spacing: 0.1em;
  }

  .demo-link {
    padding: 4px 7px;
    color: #a1e2ce;
    border: 1px solid #3b6959;
    background: #1d352d;
    font-size: 8px;
    font-weight: 750;
    letter-spacing: 0.1em;
    text-decoration: none;
  }

  .demo-link:hover {
    background: #24463b;
  }

  .save-state,
  .revision-label,
  .chain-meta,
  .section-note {
    color: #788581;
    font-size: 11px;
  }

  .save-state {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }

  .save-state.running .status-dot,
  .status-dot.running {
    background: #e0b56a;
  }

  .quiet-button,
  .panel-action,
  .icon-button,
  .sort-button,
  .text-action,
  .row-menu,
  .danger-button {
    color: #89938f;
    background: transparent;
    cursor: pointer;
  }

  .quiet-button {
    padding: 5px 0;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
  }

  .quiet-button:hover,
  .panel-action:hover,
  .icon-button:hover,
  .text-action:hover,
  .row-menu:hover {
    color: #d6e7e0;
  }

  .avatar-button {
    width: 27px;
    height: 27px;
    color: #0f1715;
    background: #7ae2c0;
    border-radius: 50%;
    font-size: 11px;
    font-weight: 800;
    cursor: pointer;
  }

  .workspace-heading {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: 18px;
    padding: 31px 30px 18px;
  }

  h1,
  h2,
  p {
    margin: 0;
  }

  h1 {
    margin-top: 8px;
    color: #f0f5f2;
    font-family: Georgia, "Times New Roman", serif;
    font-size: clamp(25px, 3vw, 36px);
    font-weight: 400;
    letter-spacing: -0.035em;
  }

  h2 {
    margin-top: 4px;
    color: #eff5f1;
    font-family: Georgia, "Times New Roman", serif;
    font-size: 19px;
    font-weight: 400;
    letter-spacing: -0.02em;
  }

  .heading-actions,
  .canvas-actions,
  .status-details,
  .section-label-row,
  .mechanic-header-actions,
  .card-footer,
  .flow-groups,
  .selector-footer,
  .selector-filters,
  .temperature-range {
    display: flex;
    align-items: center;
  }

  .heading-actions {
    gap: 18px;
  }

  .separator {
    color: #45504d;
    margin: 0 4px;
  }

  .primary-button {
    min-height: 35px;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 0 14px;
    color: #10201a;
    background: #7ae2c0;
    font-size: 11px;
    font-weight: 800;
    cursor: pointer;
    transition: background 0.15s, transform 0.15s;
  }

  .primary-button:hover { background: #a0f3d6; transform: translateY(-1px); }
  .primary-button:disabled { cursor: wait; opacity: 0.65; transform: none; }
  .primary-button.compact { min-height: 31px; padding: 0 11px; font-size: 10px; }

  .button-pulse {
    width: 7px;
    height: 7px;
    display: inline-block;
    border-radius: 50%;
    background: #254c40;
  }

  .view-switcher {
    display: flex;
    gap: 20px;
    margin: 0 30px;
    border-bottom: 1px solid #252c2d;
  }

  .view-switcher button {
    padding: 0 0 11px;
    color: #71807a;
    background: transparent;
    border-bottom: 2px solid transparent;
    cursor: pointer;
    font-size: 11px;
    font-weight: 700;
  }

  .view-switcher button:hover,
  .view-switcher button.active {
    color: #dce9e3;
    border-bottom-color: #7ae2c0;
  }

  .count-pill {
    display: inline-block;
    margin-left: 4px;
    padding: 2px 5px;
    color: #9bdcc6;
    background: #19372e;
    border-radius: 2px;
    font-size: 9px;
  }

  .workspace {
    display: grid;
    grid-template-columns: 252px minmax(480px, 1fr) 284px;
    gap: 14px;
    padding: 16px 30px 25px;
    align-items: start;
  }

  .panel-surface,
  .factory-canvas {
    border: 1px solid #252d2d;
    background: #121718;
  }

  .panel-surface {
    position: sticky;
    top: 16px;
    padding: 19px 16px;
  }

  .panel-heading {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding-bottom: 17px;
    border-bottom: 1px solid #283031;
  }

  .panel-action {
    padding: 0 2px;
    font-size: 15px;
    line-height: 1;
  }

  .target-section,
  .inspector-section {
    padding: 19px 0;
    border-bottom: 1px solid #252e2e;
  }

  .section-label-row {
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 10px;
  }

  .section-note {
    font-size: 10px;
  }

  .target-card,
  .picker-button,
  .select-button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 9px;
    text-align: left;
    color: #dde8e3;
    background: #192121;
    border: 1px solid #303c3a;
    cursor: pointer;
  }

  .target-card {
    min-height: 63px;
    padding: 9px;
  }

  .target-card:hover,
  .picker-button:hover,
  .select-button:hover {
    border-color: #6aab97;
    background: #1d2a28;
  }

  .target-copy,
  .picker-copy,
  .input-copy,
  .catalog-copy {
    display: grid;
    min-width: 0;
    gap: 3px;
  }

  .target-copy strong,
  .picker-copy strong,
  .input-copy strong,
  .catalog-copy strong {
    overflow: hidden;
    color: #dfe9e5;
    font-size: 11px;
    font-weight: 700;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .target-copy small,
  .picker-copy small,
  .input-copy small,
  .catalog-copy small,
  .helper-text,
  .panel-footnote,
  .drop-zone small,
  .empty-inspector small,
  .no-results small {
    color: #788580;
    font-size: 10px;
    line-height: 1.4;
  }

  .target-rate {
    display: grid;
    margin-left: auto;
    text-align: right;
  }

  .target-rate strong { color: #80dfbe; font-size: 13px; }
  .target-rate small { color: #6f817a; font-size: 9px; }

  .chevron {
    color: #78918a;
    font-size: 14px;
    line-height: 1;
  }

  .text-action {
    padding: 12px 0 0;
    font-size: 10px;
    font-weight: 700;
  }

  .external-section { padding-bottom: 15px; }

  .input-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 34px;
    border-bottom: 1px solid #202827;
  }

  .input-rate {
    margin-left: auto;
    color: #c6d3ce;
    font-family: "SFMono-Regular", Consolas, monospace;
    font-size: 10px;
  }

  .row-menu {
    padding: 4px 0 4px 3px;
    font-size: 12px;
  }

  .constraints-section {
    padding: 19px 0 11px;
  }

  .toggle-row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 9px;
    min-height: 43px;
    cursor: pointer;
  }

  .toggle-row input {
    position: absolute;
    opacity: 0;
  }

  .toggle-track {
    width: 25px;
    height: 14px;
    flex: 0 0 auto;
    padding: 2px;
    background: #303a39;
    border-radius: 10px;
    transition: background 0.15s;
  }

  .toggle-track span {
    width: 10px;
    height: 10px;
    display: block;
    background: #77837f;
    border-radius: 50%;
    transition: transform 0.15s, background 0.15s;
  }

  .toggle-row input:checked + .toggle-track { background: #235d4c; }
  .toggle-row input:checked + .toggle-track span { background: #83e3c2; transform: translateX(11px); }

  .toggle-row > span:last-child {
    display: grid;
    gap: 2px;
  }

  .toggle-row strong { color: #cbd8d3; font-size: 10px; }
  .toggle-row small { color: #6f7d79; font-size: 9px; }

  .panel-footnote {
    display: flex;
    gap: 7px;
    padding-top: 15px;
    line-height: 1.5;
  }

  .info-mark,
  .empty-mark {
    width: 15px;
    height: 15px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    color: #7da996;
    border: 1px solid #396254;
    border-radius: 50%;
    font-family: Georgia, serif;
    font-size: 10px;
  }

  .factory-canvas {
    min-width: 0;
    padding: 20px;
  }

  .canvas-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 13px;
    margin-bottom: 17px;
  }

  .chain-meta {
    margin-left: 12px;
    font-size: 10px;
  }

  .canvas-actions { gap: 8px; }

  .sort-button {
    padding: 8px 4px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
  }

  .sort-button .chevron { margin-left: 4px; font-size: 10px; }

  .add-wrapper { position: relative; }

  .add-menu {
    position: absolute;
    z-index: 10;
    top: calc(100% + 8px);
    right: 0;
    width: 236px;
    padding: 8px;
    border: 1px solid #3e5c53;
    background: #17201f;
    box-shadow: 0 14px 30px rgba(0, 0, 0, 0.35);
  }

  .menu-title { padding: 7px 8px 6px; }

  .add-menu button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 8px;
    color: #d5e1dc;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }

  .add-menu button:hover { background: #22322e; }
  .add-menu button > span:last-child { display: grid; gap: 2px; }
  .add-menu strong { font-size: 11px; }
  .add-menu small { color: #788983; font-size: 9px; }

  .menu-icon {
    width: 25px;
    height: 25px;
    display: grid;
    place-items: center;
    color: #a1e2ce;
    border: 1px solid #3b6959;
    background: #1d352d;
    font-family: "SFMono-Regular", Consolas, monospace;
    font-size: 8px;
    font-weight: 700;
  }

  .flow-summary {
    display: grid;
    grid-template-columns: 1fr 1px 1fr 1px 1fr;
    gap: 12px;
    align-items: center;
    margin-bottom: 15px;
    padding: 12px 14px;
    border: 1px solid #283533;
    background: #151d1c;
  }

  .summary-segment { display: grid; gap: 4px; min-width: 0; }
  .summary-segment strong { color: #d9e5df; font-family: "SFMono-Regular", Consolas, monospace; font-size: 12px; }
  .summary-segment > span:last-child { overflow: hidden; color: #71807b; font-size: 9px; text-overflow: ellipsis; white-space: nowrap; }
  .summary-segment .positive { color: #78cdae; }
  .summary-line { width: 1px; height: 29px; background: #30403c; }

  .mechanic-stack { display: grid; gap: 9px; }

  .mechanic-card {
    overflow: hidden;
    border: 1px solid #293434;
    background: #151c1c;
    transition: border-color 0.15s, background 0.15s;
  }

  .mechanic-card:hover,
  .mechanic-card.selected {
    border-color: #4e7569;
  }

  .mechanic-card.selected { background: #17211f; box-shadow: inset 3px 0 #7ae2c0; }

  .mechanic-header {
    min-height: 49px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px 0 6px;
  }

  .drag-handle {
    width: 13px;
    display: grid;
    grid-template-columns: repeat(2, 3px);
    gap: 3px 3px;
    padding: 5px 0;
    background: transparent;
    cursor: grab;
  }

  .drag-handle span { width: 3px; height: 3px; background: #56625e; border-radius: 50%; }
  .drag-handle:hover span { background: #9ab5aa; }

  .mechanic-title-button {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    color: #d9e4df;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }

  .step-number {
    color: #70817b;
    font-family: "SFMono-Regular", Consolas, monospace;
    font-size: 10px;
  }

  .kind-chip {
    padding: 3px 5px;
    color: #99b1a8;
    background: #26322f;
    font-size: 8px;
    font-weight: 750;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .mechanic-title {
    overflow: hidden;
    font-size: 12px;
    font-weight: 700;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .warning-mark {
    width: 15px;
    height: 15px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    color: #e3b873;
    border: 1px solid #84673b;
    border-radius: 50%;
    font-size: 9px;
    font-weight: 800;
  }

  .mechanic-header-actions { gap: 7px; margin-left: auto; }

  .rate-chip {
    padding: 5px 7px;
    color: #9ddbc5;
    background: #1e362f;
    font-family: "SFMono-Regular", Consolas, monospace;
    font-size: 9px;
  }

  .icon-button {
    width: 25px;
    height: 25px;
    display: inline-grid;
    place-items: center;
    color: #899590;
    background: transparent;
    border: 1px solid transparent;
    cursor: pointer;
  }

  .icon-button.subtle { width: 23px; height: 23px; border-color: #2c3836; }
  .icon-button.subtle:hover { border-color: #628c7d; }

  .mechanic-body {
    padding: 0 13px 13px 37px;
    border-top: 1px solid #273231;
  }

  .field-grid {
    display: grid;
    grid-template-columns: minmax(160px, 1.1fr) minmax(160px, 1.1fr) minmax(90px, 0.6fr) minmax(110px, 0.9fr);
    gap: 8px;
    padding: 13px 0 12px;
  }

  .field-block { min-width: 0; }
  .field-label { display: block; margin-bottom: 6px; font-size: 8px; }

  .picker-button { min-height: 43px; padding: 5px 7px; }
  .select-button { min-height: 43px; justify-content: space-between; padding: 0 9px; font-size: 10px; }
  .select-button span:first-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .card-footer {
    justify-content: space-between;
    gap: 10px;
    padding-top: 10px;
    border-top: 1px solid #273231;
  }

  .flow-groups { flex-wrap: wrap; gap: 5px; min-width: 0; }

  .flow-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 190px;
    padding: 5px 7px;
    font-size: 9px;
  }

  .flow-pill span:not(.flow-sign) { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .flow-pill small { color: #a0b0aa; font-family: "SFMono-Regular", Consolas, monospace; font-size: 8px; }
  .flow-sign { font-weight: 800; }
  .input-flow { color: #d2b37b; background: #2a2419; border: 1px solid #4c4029; }
  .output-flow { color: #a4e2ce; background: #1b312a; border: 1px solid #315d4d; }

  .machine-count {
    display: grid;
    flex: 0 0 auto;
    gap: 2px;
    min-width: 63px;
    text-align: right;
  }

  .machine-count strong { color: #e2ece7; font-family: "SFMono-Regular", Consolas, monospace; font-size: 13px; }
  .machine-count small { color: #75837e; font-size: 9px; }

  .warning-strip {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 11px;
    padding: 7px 8px;
    color: #c8aa73;
    background: #252016;
    border: 1px solid #4c3e27;
    font-size: 9px;
  }

  .warning-strip .warning-mark { width: 14px; height: 14px; }

  .drop-zone {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 11px;
    margin-top: 12px;
    padding: 16px;
    color: #8ba098;
    background: transparent;
    border: 1px dashed #3a5149;
    text-align: left;
    cursor: pointer;
  }

  .drop-zone:hover { color: #b9e5d5; border-color: #6da58f; background: #14201d; }
  .drop-zone > span:last-child { display: grid; gap: 3px; }
  .drop-zone strong { font-size: 11px; }
  .drop-plus { color: #7ae2c0; font-size: 21px; font-weight: 300; }

  .inspector-hero {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 17px 0 3px;
  }

  .inspector-hero > div { display: grid; gap: 4px; min-width: 0; }
  .inspector-hero strong { overflow: hidden; color: #dce8e2; font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
  .inspector-hero small { color: #72807b; font-size: 10px; }

  .metric-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1px;
    background: #2a3432;
    border: 1px solid #2a3432;
  }

  .metric-grid > div { display: grid; gap: 5px; padding: 10px 8px; background: #18201f; }
  .metric-grid strong { color: #d6e4dd; font-family: "SFMono-Regular", Consolas, monospace; font-size: 11px; }

  .temperature-card {
    padding: 10px;
    background: #18201f;
    border: 1px solid #2e4440;
  }

  .temperature-line { display: flex; align-items: center; gap: 7px; }
  .temperature-line strong { color: #dcebe4; font-family: "SFMono-Regular", Consolas, monospace; font-size: 11px; }
  .temperature-line span:last-child { margin-left: auto; color: #7b8e87; font-size: 9px; }
  .temperature-dot { width: 7px; height: 7px; background: #70b8df; border-radius: 50%; box-shadow: 0 0 0 3px rgba(112, 184, 223, 0.12); }
  .temperature-bar { display: block; height: 4px; margin: 13px 2px 6px; background: linear-gradient(90deg, #33566a, #70b8df 31%, #d5ae69 70%, #733d3e); }
  .temperature-bar i { width: 31%; height: 8px; display: block; position: relative; top: -2px; margin-left: 31%; background: #c4f0e0; border: 1px solid #173b32; }
  .temperature-range { justify-content: space-between; color: #71837d; font-family: "SFMono-Regular", Consolas, monospace; font-size: 8px; }

  .helper-text { margin-top: 9px; line-height: 1.55; }

  .contribution-list { display: grid; gap: 8px; }
  .contribution-row { display: grid; grid-template-columns: 7px 1fr auto; align-items: center; gap: 7px; color: #a7b5af; font-size: 10px; }
  .contribution-row strong { color: #c8b27e; font-family: "SFMono-Regular", Consolas, monospace; font-size: 9px; }
  .contribution-row strong.positive { color: #8bd8bd; }
  .contribution-mark { width: 5px; height: 5px; background: #c8a66b; border-radius: 50%; }
  .contribution-mark.output { background: #7ae2c0; }

  .danger-button {
    width: 100%;
    margin-top: 18px;
    padding: 9px;
    color: #c78e8e;
    border: 1px solid #4d3030;
    background: #211819;
    font-size: 10px;
    cursor: pointer;
  }

  .danger-button:hover { color: #f0b3ad; border-color: #8a4c4c; }
  .danger-button:disabled { cursor: not-allowed; opacity: 0.45; }

  .empty-inspector,
  .no-results {
    display: grid;
    justify-items: center;
    gap: 8px;
    padding: 40px 12px;
    text-align: center;
  }

  .empty-inspector strong,
  .no-results strong { color: #b8c7c0; font-size: 11px; }
  .empty-mark { width: 25px; height: 25px; color: #70867d; border-color: #40584e; font-size: 11px; }

  .statusbar {
    display: flex;
    justify-content: space-between;
    gap: 18px;
    min-height: 35px;
    padding: 0 30px;
    color: #83908b;
    border-top: 1px solid #252d2d;
    background: #101516;
    font-size: 10px;
  }

  .status-message,
  .status-details { display: flex; align-items: center; gap: 8px; }
  .status-details { gap: 15px; font-size: 8px; letter-spacing: 0.08em; }
  .status-details strong { color: #a5b6ae; font-weight: 700; }

  .ledger-view { min-height: 580px; }
  .ledger-view .canvas-toolbar { padding-bottom: 3px; border-bottom: 1px solid #273231; }
  .solved-badge { display: inline-flex; align-items: center; gap: 7px; padding: 6px 8px; color: #a8dfcc; background: #1b302a; font-size: 10px; }
  .ledger-table { border: 1px solid #293634; }
  .ledger-row { display: grid; grid-template-columns: minmax(170px, 1.4fr) 0.75fr 0.8fr 0.8fr; align-items: center; gap: 12px; min-height: 53px; padding: 0 13px; border-bottom: 1px solid #273130; color: #8d9b95; font-size: 10px; }
  .ledger-row:last-child { border-bottom: 0; }
  .ledger-row:not(.ledger-head):hover { background: #182320; }
  .ledger-head { min-height: 34px; background: #18201f; font-size: 8px; }
  .ledger-flow { display: flex; align-items: center; gap: 8px; color: #cddbd5; }
  .ledger-row strong { color: #d5e2dc; font-size: 10px; }
  .ledger-row > strong { color: #cbaf76; font-family: "SFMono-Regular", Consolas, monospace; }
  .ledger-row > strong.positive { color: #87d5b9; }
  .ledger-row > strong.negative { color: #d2a96a; }
  .ledger-note { display: flex; gap: 8px; padding: 15px 3px; color: #7d8c86; font-size: 10px; line-height: 1.5; }

  .modal-backdrop {
    position: fixed;
    z-index: 30;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 20px;
    background: rgba(4, 7, 8, 0.75);
    backdrop-filter: blur(5px);
  }

  .selector-modal {
    width: min(680px, 100%);
    max-height: min(720px, calc(100vh - 40px));
    display: flex;
    flex-direction: column;
    padding: 21px;
    border: 1px solid #496b5f;
    background: #151d1c;
    box-shadow: 0 25px 70px rgba(0, 0, 0, 0.52);
  }

  .selector-heading { display: flex; justify-content: space-between; gap: 16px; padding-bottom: 17px; }
  .selector-heading p { margin-top: 7px; color: #7b8b85; font-size: 10px; }
  .search-box { display: flex; align-items: center; gap: 9px; min-height: 42px; padding: 0 10px; background: #0f1515; border: 1px solid #334944; }
  .search-mark { color: #7ae2c0; font-family: "SFMono-Regular", Consolas, monospace; font-size: 17px; }
  .search-box input { width: 100%; color: #e1ebe6; background: transparent; border: 0; outline: 0; font-size: 12px; }
  .search-box input::placeholder { color: #667771; }
  kbd { padding: 3px 5px; color: #84938d; background: #202b29; border: 1px solid #394743; font-family: "SFMono-Regular", Consolas, monospace; font-size: 8px; white-space: nowrap; }
  .selector-filters { gap: 6px; padding: 14px 0 10px; overflow-x: auto; }
  .filter-chip { padding: 6px 8px; color: #81908a; background: #1b2523; border: 1px solid #2e3c38; font-size: 9px; cursor: pointer; white-space: nowrap; }
  .filter-chip:hover, .filter-chip.active { color: #c8e8dc; border-color: #598674; background: #20352e; }
  .catalog-list { overflow: auto; border-top: 1px solid #293633; border-bottom: 1px solid #293633; }
  .catalog-row { width: 100%; display: flex; align-items: center; gap: 10px; padding: 10px 7px; color: #d5e2dc; background: transparent; border-bottom: 1px solid #25302e; text-align: left; cursor: pointer; }
  .catalog-row:last-child { border-bottom: 0; }
  .catalog-row:hover { background: #20322d; }
  .catalog-copy { flex: 1; }
  .catalog-group { width: 130px; overflow: hidden; color: #6f8079; font-size: 9px; text-overflow: ellipsis; white-space: nowrap; }
  .selector-footer { justify-content: space-between; gap: 10px; padding-top: 14px; color: #71817a; font-size: 9px; }
  .selector-footer > span { display: inline-flex; align-items: center; gap: 4px; }

  .item-icon {
    width: 25px;
    height: 25px;
    display: inline-grid;
    place-items: center;
    flex: 0 0 auto;
    color: #b9e9db;
    background: #1e3931;
    border: 1px solid #3d7562;
    font-family: "SFMono-Regular", Consolas, monospace;
    font-size: 8px;
    font-weight: 800;
    letter-spacing: -0.05em;
  }

  .item-icon.small { width: 20px; height: 20px; font-size: 7px; }
  .item-icon.medium { width: 29px; height: 29px; font-size: 8px; }
  .item-icon.large { width: 36px; height: 36px; font-size: 9px; }
  .item-icon.cyan { color: #b6f1e0; background: #1b3c32; border-color: #4a8d73; }
  .item-icon.amber { color: #e4c78f; background: #3b2d1b; border-color: #806437; }
  .item-icon.copper { color: #e5aa80; background: #40261c; border-color: #8d5740; }
  .item-icon.steel { color: #b9cbd0; background: #26343a; border-color: #58717a; }
  .item-icon.blue { color: #a7d4e8; background: #1d3540; border-color: #4d7e95; }
  .item-icon.violet { color: #cabce8; background: #302844; border-color: #75689b; }

  @media (max-width: 1240px) {
    .workspace { grid-template-columns: 222px minmax(430px, 1fr) 245px; padding-left: 20px; padding-right: 20px; }
    .topbar { padding-left: 20px; padding-right: 20px; gap: 16px; }
    .workspace-heading { padding-left: 20px; padding-right: 20px; }
    .view-switcher { margin-left: 20px; margin-right: 20px; }
    .prototype-chip, .quiet-button { display: none; }
  }

  @media (max-width: 1000px) {
    .workspace { grid-template-columns: 210px minmax(420px, 1fr); }
    .inspector { display: none; }
    .field-grid { grid-template-columns: 1fr 1fr; }
    .compact-field { min-width: 0; }
  }

  @media (max-width: 720px) {
    .topbar { min-height: auto; flex-wrap: wrap; padding-top: 12px; padding-bottom: 10px; }
    .brand-lockup { width: 100%; }
    .topbar-actions { position: absolute; top: 15px; right: 15px; }
    .save-state { display: none; }
    .project-tabs { width: 100%; min-height: 39px; overflow-x: auto; }
    .project-tab { padding: 0 11px; }
    .workspace-heading { align-items: flex-start; flex-direction: column; padding: 24px 15px 15px; }
    .heading-actions { width: 100%; justify-content: space-between; }
    .view-switcher { margin: 0 15px; }
    .workspace { grid-template-columns: 1fr; gap: 10px; padding: 10px 15px 18px; }
    .goals-panel { position: static; }
    .goals-panel .panel-footnote { display: none; }
    .constraints-section { display: grid; grid-template-columns: 1fr 1fr; gap: 5px; }
    .constraints-section .section-label-row { grid-column: 1 / -1; }
    .factory-canvas { padding: 13px; }
    .canvas-toolbar { align-items: flex-start; flex-direction: column; }
    .canvas-actions { width: 100%; justify-content: space-between; }
    .flow-summary { gap: 7px; padding: 10px 8px; }
    .summary-segment strong { font-size: 10px; }
    .summary-segment > span:last-child { font-size: 8px; }
    .mechanic-body { padding-left: 13px; }
    .mechanic-header { padding-left: 7px; }
    .drag-handle { display: none; }
    .mechanic-title { max-width: 140px; }
    .rate-chip { display: none; }
    .card-footer { align-items: flex-end; flex-direction: column; }
    .flow-groups { width: 100%; }
    .machine-count { width: 100%; text-align: left; }
    .statusbar { align-items: flex-start; flex-direction: column; gap: 6px; padding: 9px 15px; }
    .status-details { flex-wrap: wrap; }
    .catalog-group { display: none; }
  }

  @media (max-width: 480px) {
    .workspace-heading h1 { font-size: 28px; }
    .target-card { grid-template-columns: auto 1fr auto; }
    .target-rate { display: none; }
    .field-grid { grid-template-columns: 1fr; }
    .flow-summary { grid-template-columns: 1fr; gap: 8px; }
    .summary-line { width: 100%; height: 1px; }
    .selector-modal { padding: 15px; }
    .selector-footer > span { display: none; }
  }
</style>

<script lang="ts">
  import { dispatch, type AppViewState, type MechanicKind } from "./lib/api";

  // Phase 2：渲染投影（Rust reducer 的镜像），全部修改走 dispatch
  let view = $state<AppViewState>({
    factory_name: "",
    mechanics: [],
    selected: null,
  });
  let factoryName = $state("");

  const addable: MechanicKind[] = [
    "recipe",
    "mining",
    "spoil",
    "plant",
    "item-fuel",
    "item-launch",
    "generator",
    "boiler",
    "reactor",
  ];

  $effect(() => {
    // 初始加载渲染投影（get_view 无副作用）
    import("./lib/api").then(({ getView }) => getView().then((v) => (view = v)));
  });

  async function add(kind: MechanicKind) {
    view = await dispatch({ type: "add-mechanic", kind });
  }
  async function remove(id: number) {
    view = await dispatch({ type: "remove-mechanic", id });
  }
  async function select(id: number) {
    view = await dispatch({ type: "select-mechanic", id });
  }
  async function rename() {
    view = await dispatch({ type: "set-factory-name", name: factoryName });
  }
</script>

<main>
  <header>
    <h1>Metatorio</h1>
    <div class="factory">
      <input bind:value={factoryName} placeholder="factory name" />
      <button onclick={rename}>rename</button>
      <span class="name">{view.factory_name}</span>
    </div>
  </header>

  <section class="toolbar">
    {#each addable as kind (kind)}
      <button onclick={() => add(kind)} class="add">+ {kind}</button>
    {/each}
  </section>

  <section class="cards">
    {#each view.mechanics as m (m.id)}
      <div
        class="card"
        class:selected={view.selected === m.id}
        onclick={() => select(m.id)}
      >
        <span class="kind">{m.kind}</span>
        <span class="summary">{m.summary}</span>
        <button onclick={(e) => { e.stopPropagation(); remove(m.id); }}>×</button>
      </div>
    {:else}
      <p class="muted">no mechanics yet — add one above</p>
    {/each}
  </section>
</main>

<style>
  main {
    font-family: system-ui, sans-serif;
    max-width: 48rem;
    margin: 2rem auto;
    padding: 0 1rem;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 1rem;
  }
  .factory {
    display: flex;
    gap: 0.4rem;
    align-items: center;
  }
  .name {
    color: #666;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin: 1rem 0;
  }
  .cards {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .card {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    padding: 0.5rem 0.8rem;
    border: 1px solid #ccc;
    border-radius: 6px;
    cursor: pointer;
  }
  .card.selected {
    border-color: #2e7d32;
    background: #e8f5e9;
  }
  .kind {
    font-size: 0.75rem;
    color: #555;
    background: #eee;
    padding: 0.1rem 0.4rem;
    border-radius: 3px;
  }
  .summary {
    flex: 1;
  }
  .muted {
    color: #999;
  }
</style>

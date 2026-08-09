<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  // hello-world IPC 桥验证：Phase 1 里程碑
  let greeting = $state("(not invoked yet)");
  let input = $state("Metatorio");

  async function callHello() {
    greeting = await invoke<string>("hello", { name: input });
  }
</script>

<main>
  <h1>Metatorio</h1>
  <p>Tauri + Svelte 5 脚手架</p>

  <input bind:value={input} placeholder="name" />
  <button onclick={callHello}>invoke hello</button>
  <p>{greeting}</p>
</main>

<style>
  main {
    font-family: system-ui, sans-serif;
    max-width: 40rem;
    margin: 3rem auto;
    padding: 0 1rem;
  }
  input,
  button {
    font-size: 1rem;
    padding: 0.4rem 0.6rem;
    margin-right: 0.5rem;
  }
</style>

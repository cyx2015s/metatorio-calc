<script lang="ts">
  import type { Location } from '$lib/runtime/types';
  import { runtime } from '$lib/runtime/store.svelte';
  import Icon from './Icon.svelte';
  let {location, size=24}: {location:Location;size?:number}=$props();
</script>
{#if location.kind==='planet'}
  <Icon type="planet" name={location.name} {size} title={runtime.localizedName('planet',location.name)} />
{:else}
  <Icon type="item" name="space-platform-starter-pack" {size} title="太空" />
{/if}

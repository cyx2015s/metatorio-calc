<script module lang="ts">
  import type {
    CatalogKind,
    IdWithQuality,
    Location,
    Mechanic,
    MechanicEntry,
    SolarBalance,
    UniverseMechanicEntry,
  } from "$lib/runtime/types";
  import { locationKey } from "$lib/mechanics/located";

  export type BomRawEntry = MechanicEntry | UniverseMechanicEntry;

  export interface BomSolution {
    amount: number;
    cost: number;
  }

  export interface UnwrappedBomEntry {
    id: number;
    enabled: boolean;
    location?: Location;
    transport?: "rocket-export" | "orbital-drop";
    transportItem?: IdWithQuality;
    mechanic: Mechanic;
    amount: number;
  }

  export interface BomBuildingRow {
    key: string;
    kind: CatalogKind;
    id: string;
    quality: string;
    exact: number;
    ceil: number;
    estimated?: boolean;
  }

  export interface BomModuleRow {
    key: string;
    id: string;
    quality: string;
    exact: number;
    ceil: number;
  }

  export interface BomTransportRow {
    id: number;
    transport: "rocket-export" | "orbital-drop";
    location?: Location;
    item: IdWithQuality;
    rate: number;
  }

  export interface BomBucket {
    buildings: BomBuildingRow[];
    machineModules: BomModuleRow[];
    beacons: BomModuleRow[];
    beaconModules: BomModuleRow[];
    transports: BomTransportRow[];
  }

  /** 将浮点台数安全向上取整（过滤 LP 浮点毛刺如 4.000000001）。 */
  export function ceilBuildingCount(exact: number): number {
    if (!Number.isFinite(exact) || exact <= 0) return 0;
    const nearest = Math.round(exact);
    if (Math.abs(exact - nearest) < 1e-6) return Math.max(1, nearest);
    return Math.max(1, Math.ceil(exact));
  }

  /** 兼容普通 MechanicEntry、universeView 投影与原始 UniverseMechanicEntry。 */
  export function unwrapBomEntries(
    mechanics: readonly BomRawEntry[],
    solutions: ReadonlyMap<number, BomSolution>,
  ): UnwrappedBomEntry[] {
    const out: UnwrappedBomEntry[] = [];
    for (const entry of mechanics) {
      if (!entry.enabled) continue;
      const sol = solutions.get(entry.id);
      if (!sol || !Number.isFinite(sol.amount) || sol.amount <= 1e-9) continue;
      const raw = entry.mechanic as
        | Mechanic
        | { type: "local"; location: Location; mechanic: Mechanic }
        | { type: "rocket-export" | "orbital-drop"; location: Location; item: IdWithQuality };
      if (raw.type === "local") {
        out.push({
          id: entry.id,
          enabled: entry.enabled,
          location: ("location" in entry ? entry.location : undefined) ?? raw.location,
          mechanic: raw.mechanic,
          amount: sol.amount,
        });
      } else if (raw.type === "rocket-export" || raw.type === "orbital-drop") {
        out.push({
          id: entry.id,
          enabled: entry.enabled,
          location: ("location" in entry ? entry.location : undefined) ?? raw.location,
          transport: raw.type,
          transportItem: raw.item,
          mechanic: { type: "item-launch", item: raw.item, weight_mode: true },
          amount: sol.amount,
        });
      } else {
        const mechanic = raw as Mechanic;
        const transport = "transport" in entry ? entry.transport : undefined;
        out.push({
          id: entry.id,
          enabled: entry.enabled,
          location: "location" in entry ? entry.location : undefined,
          transport,
          transportItem: transport ? mechanic.item : undefined,
          mechanic,
          amount: sol.amount,
        });
      }
    }
    return out;
  }

  function addItemCount(
    map: Map<string, { id: string; quality: string; exact: number }>,
    id: string | undefined,
    quality: string | undefined,
    delta: number,
  ) {
    if (!id || !Number.isFinite(delta) || delta <= 0) return;
    const q = quality || "normal";
    const key = `${id}\0${q}`;
    const existing = map.get(key);
    if (existing) existing.exact += delta;
    else map.set(key, { id, quality: q, exact: delta });
  }

  function finalizeModuleMap(
    map: Map<string, { id: string; quality: string; exact: number }>,
  ): BomModuleRow[] {
    return [...map.entries()]
      .map(([key, item]) => ({
        key,
        id: item.id,
        quality: item.quality,
        exact: item.exact,
        ceil: ceilBuildingCount(item.exact),
      }))
      .sort((a, b) => b.exact - a.exact || b.ceil - a.ceil || a.id.localeCompare(b.id) || a.quality.localeCompare(b.quality));
  }

  /** 按原型类别与品质聚合一套机制条目。 */
  export function aggregateBomBucket(
    entries: readonly UnwrappedBomEntry[],
    solarBalances: Record<number, SolarBalance | null> = {},
  ): BomBucket {
    const buildingMap = new Map<
      string,
      { kind: CatalogKind; id: string; quality: string; exact: number; estimated?: boolean }
    >();
    const machineModuleMap = new Map<string, { id: string; quality: string; exact: number }>();
    const beaconMap = new Map<string, { id: string; quality: string; exact: number }>();
    const beaconModuleMap = new Map<string, { id: string; quality: string; exact: number }>();
    const transports: BomTransportRow[] = [];

    const addBuilding = (
      kind: CatalogKind,
      ref: IdWithQuality | undefined,
      exact: number,
      estimated = false,
    ) => {
      if (!ref?.id || !Number.isFinite(exact) || exact <= 0) return;
      const quality = ref.quality || "normal";
      const key = `${kind}\0${ref.id}\0${quality}`;
      const existing = buildingMap.get(key);
      if (existing) {
        existing.exact += exact;
        if (estimated) existing.estimated = existing.estimated ?? true;
      } else {
        buildingMap.set(key, { kind, id: ref.id, quality, exact, ...(estimated ? { estimated: true } : {}) });
      }
    };

    for (const entry of entries) {
      if (entry.transport && entry.transportItem) {
        transports.push({
          id: entry.id,
          transport: entry.transport,
          location: entry.location,
          item: entry.transportItem,
          rate: entry.amount,
        });
        continue;
      }

      const m = entry.mechanic;
      switch (m.type) {
        case "recipe":
          addBuilding("machine", m.machine, entry.amount);
          break;
        case "mining":
          addBuilding("mining-machine", m.machine, entry.amount);
          break;
        case "tile-extract":
          addBuilding("offshore-pump", m.machine, entry.amount);
          break;
        case "generator":
          addBuilding("generator", m.generator, entry.amount);
          break;
        case "fusion-generator":
          addBuilding("fusion-generator", m.generator, entry.amount);
          break;
        case "boiler":
          addBuilding("boiler", m.boiler, entry.amount);
          break;
        case "reactor":
          addBuilding("reactor", m.reactor, entry.amount);
          break;
        case "fusion-reactor":
          addBuilding("fusion-reactor", m.reactor, entry.amount);
          break;
        case "solar": {
          addBuilding("solar-panel", m.solar_panel, entry.amount);
          const bal = solarBalances[entry.id];
          if (
            bal &&
            !bal.continuous_daylight &&
            bal.recommended_accumulators != null &&
            bal.recommended_accumulators > 1e-9 &&
            m.accumulator?.id
          ) {
            addBuilding("accumulator", m.accumulator, entry.amount * bal.recommended_accumulators, true);
          }
          break;
        }
      }

      if ((m.type === "recipe" || m.type === "mining") && m.module_config) {
        for (const mod of m.module_config.modules ?? []) {
          addItemCount(machineModuleMap, mod.id, mod.quality, entry.amount);
        }
        for (const beacon of m.module_config.beacons ?? []) {
          if (!beacon.beacon?.id || !beacon.count || beacon.count <= 0) continue;
          const share = Math.max(beacon.share ?? 1, 1);
          addItemCount(beaconMap, beacon.beacon.id, beacon.beacon.quality, (entry.amount * beacon.count) / share);
          for (const [mod, count] of beacon.modules ?? []) {
            if (!mod?.id || !count || count <= 0) continue;
            addItemCount(beaconModuleMap, mod.id, mod.quality, (entry.amount * count) / share);
          }
        }
      }
    }

    const buildings: BomBuildingRow[] = [...buildingMap.entries()]
      .map(([key, row]) => ({
        key,
        kind: row.kind,
        id: row.id,
        quality: row.quality,
        exact: row.exact,
        ceil: ceilBuildingCount(row.exact),
        estimated: row.estimated,
      }))
      .sort((a, b) => b.exact - a.exact || b.ceil - a.ceil || a.id.localeCompare(b.id) || a.quality.localeCompare(b.quality));

    transports.sort((a, b) => b.rate - a.rate || a.id - b.id);

    return {
      buildings,
      machineModules: finalizeModuleMap(machineModuleMap),
      beacons: finalizeModuleMap(beaconMap),
      beaconModules: finalizeModuleMap(beaconModuleMap),
      transports,
    };
  }

  /** 按地点分组（先行星后太空）。 */
  export function groupEntriesByLocation(entries: readonly UnwrappedBomEntry[]): {
    key: string;
    location: Location;
    entries: UnwrappedBomEntry[];
  }[] {
    const groups = new Map<string, { key: string; location: Location; entries: UnwrappedBomEntry[] }>();
    for (const entry of entries) {
      if (!entry.location) continue;
      const key = locationKey(entry.location);
      const group = groups.get(key);
      if (group) group.entries.push(entry);
      else groups.set(key, { key, location: entry.location, entries: [entry] });
    }
    return [...groups.values()].sort((a, b) => {
      if (a.location.kind !== b.location.kind) return a.location.kind === "planet" ? -1 : 1;
      return a.key.localeCompare(b.key);
    });
  }
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import { runtime } from "$lib/runtime/store.svelte";
  import { solarBalance } from "$lib/runtime/client";
  import { compactNumber } from "$lib/format";
  import { networkRate } from "$lib/mechanics/network";
  import { locationLabel, transportLabel } from "$lib/mechanics/located";
  import type { DualVar, TimeScale } from "$lib/runtime/types";
  import HoverIcon from "./HoverIcon.svelte";
  import LocationIcon from "./LocationIcon.svelte";
  import RichText from "./RichText.svelte";

  let {
    mechanics = [],
    solutions,
    flows = [],
    isUniverse = false,
    projectId,
    factoryId,
    timeScale = "seconds",
  }: {
    mechanics: readonly BomRawEntry[];
    solutions: ReadonlyMap<number, BomSolution>;
    flows?: readonly { flow: DualVar; location?: Location; amount: number }[];
    isUniverse?: boolean;
    projectId?: number | null;
    factoryId?: number | null;
    timeScale?: TimeScale;
  } = $props();

  let groupByLocation = $state(false);
  let solarBalances = $state<Record<number, SolarBalance | null>>({});
  let unitElectricity = $state<Record<number, number>>({});

  const effectiveProjectId = $derived(projectId ?? runtime.selectedProjectId ?? null);
  const effectiveFactoryId = $derived(
    factoryId ?? runtime.selectedUniverseId ?? runtime.selectedFactoryId ?? null,
  );
  const activeEntries = $derived(unwrapBomEntries(mechanics, solutions));
  const universeMode = $derived(
    isUniverse || activeEntries.some((entry) => entry.location != null),
  );
  const environmentKey = $derived(
    effectiveProjectId != null && effectiveFactoryId != null
      ? runtime.mechanicEnvironmentKey({
          project: effectiveProjectId,
          factory: effectiveFactoryId,
          kind: universeMode ? "universe" : undefined,
        })
      : "",
  );

  // 按需拉取太阳能蓄电器配平与各机制单位电功率（仅限 Tauri 环境且静默容错）。
  $effect(() => {
    const pid = effectiveProjectId;
    const fid = effectiveFactoryId;
    const isUni = universeMode;
    const entries = activeEntries;
    void environmentKey;
    void runtime.flowRevision;

    if (
      pid == null ||
      fid == null ||
      entries.length === 0 ||
      typeof window === "undefined" ||
      !("__TAURI_INTERNALS__" in window)
    ) {
      solarBalances = {};
      unitElectricity = {};
      return;
    }

    let alive = true;
    if (isUni) {
      untrack(() => runtime.loadUniversePreview(pid, fid))
        .then((preview) => {
          if (!alive) return;
          const nextSolar: Record<number, SolarBalance | null> = {};
          const nextElec: Record<number, number> = {};
          for (const item of preview.mechanics) {
            if (item.solar_balance) nextSolar[item.id] = item.solar_balance;
            let elec = 0;
            for (const f of item.flows) {
              if (f.flow.flow === "Electricity" && Number.isFinite(f.amount ?? NaN)) {
                elec += f.amount ?? 0;
              }
            }
            if (Math.abs(elec) > 1e-9) nextElec[item.id] = elec;
          }
          solarBalances = nextSolar;
          unitElectricity = nextElec;
        })
        .catch(() => {
          if (alive) {
            solarBalances = {};
            unitElectricity = {};
          }
        });
    } else {
      const solarEntries = entries.filter((e) => e.mechanic.type === "solar");
      Promise.all([
        Promise.all(
          solarEntries.map(async (entry) => {
            const info = await solarBalance(pid, fid, entry.id).catch(() => null);
            return [entry.id, info] as const;
          }),
        ),
        Promise.all(
          entries.map(async (entry) => {
            const list = await untrack(() =>
              runtime.getMechanicFlow(pid, fid, entry.id, entry.mechanic),
            ).catch(() => null);
            let elec = 0;
            if (list) {
              for (const f of list) {
                if (f.flow === "Electricity" && Number.isFinite(f.amount)) elec += f.amount;
              }
            }
            return [entry.id, elec] as const;
          }),
        ),
      ]).then(([solarPairs, elecPairs]) => {
        if (!alive) return;
        const nextSolar: Record<number, SolarBalance | null> = {};
        for (const [id, info] of solarPairs) {
          if (info) nextSolar[id] = info;
        }
        const nextElec: Record<number, number> = {};
        for (const [id, elec] of elecPairs) {
          if (Math.abs(elec) > 1e-9) nextElec[id] = elec;
        }
        solarBalances = nextSolar;
        unitElectricity = nextElec;
      });
    }

    return () => {
      alive = false;
    };
  });

  const globalBucket = $derived(aggregateBomBucket(activeEntries, solarBalances));
  const locationGroups = $derived.by(() => {
    if (!universeMode || !groupByLocation) return [];
    return groupEntriesByLocation(activeEntries)
      .map((group) => ({
        ...group,
        label: locationLabel(group.location, (k, n) => runtime.localizedName(k, n)),
        bucket: aggregateBomBucket(group.entries, solarBalances),
      }))
      .filter(
        (group) =>
          group.bucket.buildings.length > 0 ||
          group.bucket.machineModules.length > 0 ||
          group.bucket.beacons.length > 0,
      );
  });

  const powerOverview = $derived.by(() => {
    let flowNetElec = 0;
    let hasFlowElec = false;
    let netHeat = 0;
    let hasFlowHeat = false;
    const locElec = new Map<string, { location: Location; net: number }>();

    for (const row of flows) {
      if (!Number.isFinite(row.amount)) continue;
      if (row.flow === "Electricity") {
        flowNetElec += row.amount;
        hasFlowElec = true;
        if (row.location) {
          const k = locationKey(row.location);
          const cur = locElec.get(k);
          if (cur) cur.net += row.amount;
          else locElec.set(k, { location: row.location, net: row.amount });
        }
      } else if (row.flow === "Heat") {
        netHeat += row.amount;
        hasFlowHeat = true;
      }
    }

    let grossGen = 0;
    let grossUse = 0;
    let hasUnitElec = false;
    for (const entry of activeEntries) {
      const unit = unitElectricity[entry.id];
      if (unit == null || !Number.isFinite(unit)) continue;
      hasUnitElec = true;
      const total = unit * entry.amount;
      if (total > 0) grossGen += total;
      else if (total < 0) grossUse += Math.abs(total);
    }

    const netElec = hasFlowElec ? flowNetElec : grossGen - grossUse;
    const perLocation = [...locElec.values()]
      .filter((item) => Math.abs(item.net) >= 1e-3)
      .sort((a, b) => Math.abs(b.net) - Math.abs(a.net));

    return {
      hasPower:
        hasUnitElec ||
        (hasFlowElec && Math.abs(flowNetElec) >= 1e-3) ||
        (hasFlowHeat && Math.abs(netHeat) >= 1e-3),
      hasUnitElec,
      grossGen,
      grossUse,
      netElec,
      netHeat,
      hasHeat: hasFlowHeat && Math.abs(netHeat) >= 1e-3,
      perLocation,
    };
  });

  const logisticsSummary = $derived.by(() => {
    const exports = globalBucket.transports.filter((t) => t.transport === "rocket-export");
    const drops = globalBucket.transports.filter((t) => t.transport === "orbital-drop");
    const sumRate = (rows: readonly BomTransportRow[]) => rows.reduce((s, r) => s + r.rate, 0);
    const distinctItems = (rows: readonly BomTransportRow[]) =>
      new Set(rows.map((r) => `${r.item.id}\0${r.item.quality || "normal"}`)).size;
    return {
      total: globalBucket.transports.length,
      exports,
      drops,
      exportRate: sumRate(exports),
      dropRate: sumRate(drops),
      exportItems: distinctItems(exports),
      dropItems: distinctItems(drops),
    };
  });

  const timeFactor = $derived(
    timeScale === "hours" ? 3600 : timeScale === "minutes" ? 60 : 1,
  );
  const timeSuffix = $derived(
    timeScale === "hours" ? "/h" : timeScale === "minutes" ? "/min" : "/s",
  );

  function formatWatts(watts: number): string {
    return networkRate("Electricity", watts, timeScale);
  }
</script>

{#snippet moduleList(
  rows: readonly BomModuleRow[],
  kind: "module" | "beacon",
  unitLabel: string,
  approx = false,
  iconSize = 20,
)}
  <div class="bom-rows">
    {#each rows as row (row.key)}
      <div class="bom-row">
        <HoverIcon
          type={kind === "beacon" ? "entity" : "item"}
          name={row.id}
          detailKind={kind}
          quality={row.quality}
          size={iconSize}
        />
        <span class="bom-name">
          <RichText text={runtime.richName(kind, row.id)} />
          {#if row.quality !== "normal"}
            <span class="bom-tag">{runtime.localizedName("quality", row.quality)}</span>
          {/if}
        </span>
        <span class="bom-counts mono">
          <span class="bom-exact">{compactNumber(row.exact)}</span>
          <span class="bom-ceil">{approx ? `~${row.ceil}` : row.ceil} {unitLabel}</span>
        </span>
      </div>
    {/each}
  </div>
{/snippet}

{#snippet renderBucket(bucket: BomBucket, compactEmpty = false)}
  {#if bucket.buildings.length > 0}
    <div class="bom-block">
      <div class="bom-head">
        <span>机器与装置</span>
        <span class="count">{bucket.buildings.length} 种</span>
      </div>
      <div class="bom-rows">
        {#each bucket.buildings as row (row.key)}
          <div class="bom-row">
            <HoverIcon
              type="entity"
              name={row.id}
              detailKind={row.kind}
              quality={row.quality}
              size={22}
            />
            <span class="bom-name">
              <RichText text={runtime.richName(row.kind, row.id)} />
              {#if row.quality !== "normal"}
                <span class="bom-tag">{runtime.localizedName("quality", row.quality)}</span>
              {/if}
              {#if row.estimated}
                <span class="bom-tag muted">估算</span>
              {/if}
            </span>
            <span class="bom-counts mono">
              <span class="bom-exact">{compactNumber(row.exact)}</span>
              <span class="bom-ceil">{row.ceil} 台</span>
            </span>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  {#if bucket.machineModules.length > 0 || bucket.beacons.length > 0}
    <div class="bom-block">
      <div class="bom-head">
        <span>插件与插件塔</span>
        <span class="count">
          {bucket.machineModules.length + bucket.beacons.length} 项
        </span>
      </div>
      {#if bucket.machineModules.length > 0}
        <div class="bom-subhead">机器插件需求</div>
        {@render moduleList(bucket.machineModules, "module", "个")}
      {/if}
      {#if bucket.beacons.length > 0}
        <div class="bom-subhead">
          <span>插件塔</span>
          <span class="bom-estimate-note">按共享系数 / 机台估算</span>
        </div>
        {@render moduleList(bucket.beacons, "beacon", "座", true)}
        {#if bucket.beaconModules.length > 0}
          <div class="bom-subhead">
            <span>塔内插件</span>
            <span class="bom-estimate-note">按共享系数估算</span>
          </div>
          {@render moduleList(bucket.beaconModules, "module", "个", true, 18)}
        {/if}
      {/if}
    </div>
  {:else if !compactEmpty}
    <div class="bom-empty-note">未使用机器插件或插件塔。</div>
  {/if}
{/snippet}

<div class="bom-panel">
  {#if universeMode}
    <div class="bom-toolbar">
      <div class="bom-seg" role="group" aria-label="建筑清单分组方式">
        <button
          type="button"
          class="seg-btn"
          class:active={!groupByLocation}
          aria-pressed={!groupByLocation}
          onclick={() => (groupByLocation = false)}
        >全局汇总</button>
        <button
          type="button"
          class="seg-btn"
          class:active={groupByLocation}
          aria-pressed={groupByLocation}
          onclick={() => (groupByLocation = true)}
        >按地点分组</button>
      </div>
      <span class="count">{activeEntries.length} 项启用</span>
    </div>
  {/if}

  {#if activeEntries.length === 0}
    <div class="empty-hint">暂无有效建筑需求（所有机制未启用或求解用量为 0）。</div>
  {:else}
    {#if universeMode && groupByLocation}
      {#if locationGroups.length > 0}
        <div class="bom-locations">
          {#each locationGroups as group (group.key)}
            <section class="bom-loc-card">
              <div class="bom-loc-head">
                <LocationIcon location={group.location} size={18} />
                <strong>{group.label}</strong>
                <span class="count">{group.bucket.buildings.length} 种建筑</span>
              </div>
              {@render renderBucket(group.bucket, true)}
            </section>
          {/each}
        </div>
      {:else}
        <div class="bom-empty-note">当前启用机制无需放置本地制造机或发电设备。</div>
      {/if}
    {:else}
      {#if globalBucket.buildings.length === 0 && globalBucket.machineModules.length === 0 && globalBucket.beacons.length === 0}
        <div class="bom-empty-note">当前启用机制无需放置制造机或发电设备。</div>
      {:else}
        {@render renderBucket(globalBucket)}
      {/if}
    {/if}

    <div class="bom-block">
      <div class="bom-head">
        <span>能源与星际物流</span>
      </div>
      <div class="bom-stats">
        {#if powerOverview.hasUnitElec && (powerOverview.grossGen > 1e-3 || powerOverview.grossUse > 1e-3)}
          <div class="bom-stat">
            <span>总发电</span>
            <strong class="mono ok">{formatWatts(powerOverview.grossGen)}</strong>
          </div>
          <div class="bom-stat">
            <span>总耗电</span>
            <strong class="mono">{formatWatts(powerOverview.grossUse)}</strong>
          </div>
        {/if}
        {#if powerOverview.hasPower}
          <div class="bom-stat">
            <span>净电力</span>
            <strong
              class="mono"
              class:ok={powerOverview.netElec > 1e-3}
              class:neg={powerOverview.netElec < -1e-3}
            >
              {Math.abs(powerOverview.netElec) < 1e-3
                ? "0 W"
                : `${powerOverview.netElec > 0 ? "+" : "-"}${formatWatts(powerOverview.netElec)}`}
            </strong>
          </div>
        {:else}
          <div class="bom-empty-note">当前无净发电或耗电记录。</div>
        {/if}
        {#if powerOverview.hasHeat}
          <div class="bom-stat">
            <span>净热量</span>
            <strong class="mono" class:ok={powerOverview.netHeat > 1e-3}>
              {powerOverview.netHeat > 0 ? "+" : "-"}{networkRate("Heat", powerOverview.netHeat, timeScale)}
            </strong>
          </div>
        {/if}
      </div>

      {#if universeMode && powerOverview.perLocation.length > 0}
        <div class="bom-subhead">各地点净电力</div>
        <div class="bom-rows">
          {#each powerOverview.perLocation as loc (locationKey(loc.location))}
            <div class="bom-row">
              <LocationIcon location={loc.location} size={16} />
              <span class="bom-name">{locationLabel(loc.location, (k, n) => runtime.localizedName(k, n))}</span>
              <span class="bom-ceil mono" class:ok={loc.net > 1e-3}>
                {loc.net > 0 ? "+" : "-"}{formatWatts(loc.net)}
              </span>
            </div>
          {/each}
        </div>
      {/if}

      {#if logisticsSummary.total > 0}
        <div class="bom-subhead">星际物流汇总</div>
        <div class="bom-stats">
          {#if logisticsSummary.exports.length > 0}
            <div class="bom-stat">
              <span>火箭发运</span>
              <strong class="mono">
                {logisticsSummary.exports.length} 项 · {logisticsSummary.exportItems} 种 · {compactNumber(logisticsSummary.exportRate * timeFactor)} 件{timeSuffix}
              </strong>
            </div>
          {/if}
          {#if logisticsSummary.drops.length > 0}
            <div class="bom-stat">
              <span>轨道投送</span>
              <strong class="mono">
                {logisticsSummary.drops.length} 项 · {logisticsSummary.dropItems} 种 · {compactNumber(logisticsSummary.dropRate * timeFactor)} 件{timeSuffix}
              </strong>
            </div>
          {/if}
        </div>
        <div class="bom-rows">
          {#each globalBucket.transports as row (row.id)}
            <div class="bom-row">
              {#if row.location}<LocationIcon location={row.location} size={16} />{/if}
              <HoverIcon
                type="item"
                name={row.item.id}
                detailKind="item"
                quality={row.item.quality}
                size={18}
              />
              <span class="bom-name">
                <RichText text={runtime.richName("item", row.item.id)} />
                <span class="bom-tag">{transportLabel(row.transport)}</span>
              </span>
              <span class="bom-ceil mono">{compactNumber(row.rate * timeFactor)} 件{timeSuffix}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .bom-panel {
    display: grid;
    gap: 12px;
  }
  .bom-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .bom-seg {
    display: inline-flex;
    padding: 2px;
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }
  .seg-btn {
    padding: 3px 8px;
    color: var(--muted);
    background: transparent;
    border: 0;
    border-radius: 3px;
    font-size: 10.5px;
    font-weight: 600;
    cursor: pointer;
    transition: color var(--transition-fast, 120ms ease), background-color var(--transition-fast, 120ms ease);
  }
  .seg-btn:hover {
    color: var(--text);
  }
  .seg-btn.active {
    color: var(--text);
    background: var(--card);
    box-shadow: var(--shadow-sm, none);
  }
  .bom-block {
    display: grid;
    gap: 6px;
  }
  .bom-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--line);
    color: var(--text);
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.03em;
  }
  .bom-subhead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    margin-top: 2px;
    color: var(--muted);
    font-size: 10px;
    font-weight: 600;
  }
  .bom-estimate-note {
    color: var(--faint);
    font-weight: 450;
    font-size: 9.5px;
  }
  .bom-rows {
    display: grid;
    gap: 2px;
  }
  .bom-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    padding: 3px 5px;
    border-radius: var(--radius-sm);
    transition: background-color var(--transition-fast, 120ms ease);
  }
  .bom-row:hover {
    background: var(--card-hover);
  }
  .bom-name {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
  }
  .bom-name :global(.rich-text) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bom-tag {
    flex: 0 0 auto;
    padding: 0 4px;
    border: 1px solid var(--line);
    border-radius: 3px;
    color: var(--muted);
    font-size: 9px;
    line-height: 1.35;
  }
  .bom-tag.muted {
    color: var(--faint);
  }
  .bom-counts {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    flex: 0 0 auto;
    white-space: nowrap;
  }
  .bom-exact {
    color: var(--muted);
    font-size: 10.5px;
  }
  .bom-ceil {
    color: var(--text);
    font-size: 11px;
    font-weight: 700;
    white-space: nowrap;
  }
  .bom-ceil.ok, .bom-stat strong.ok {
    color: var(--ok);
  }
  .bom-stat strong.neg {
    color: var(--text);
  }
  .bom-locations {
    display: grid;
    gap: 10px;
  }
  .bom-loc-card {
    display: grid;
    gap: 8px;
    padding: 8px;
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }
  .bom-loc-head {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
  }
  .bom-loc-head strong {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bom-stats {
    display: grid;
    gap: 4px;
  }
  .bom-stat {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    padding: 3px 6px;
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 10.5px;
  }
  .bom-stat > span {
    color: var(--muted);
    flex: 0 0 auto;
  }
  .bom-stat > strong {
    text-align: right;
    font-size: 11px;
    font-weight: 650;
    overflow-wrap: anywhere;
  }
  .count {
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    font-size: 10px;
    color: var(--faint);
  }
  .mono {
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
  }
  .bom-empty-note, .empty-hint {
    padding: 6px 0;
    color: var(--faint);
    font-size: 10.5px;
    line-height: 1.45;
  }
  .empty-hint {
    padding: 16px 0;
    text-align: center;
  }
</style>

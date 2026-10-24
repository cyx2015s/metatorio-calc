<script lang="ts">
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { iconTypeForKind } from "./icon-types";
  import { compactNumber } from "$lib/format";
  import { detailStats, formatStat, statLabels } from "./hover-model";
  import { accessibleKind, accessibleKindFor, accessibleName, type DualVar, type FlowAmount, type PrototypeDetail } from "$lib/runtime/types";
  import { flowName } from "$lib/display-labels";
  import { isEnergyFlow, presentFlow, type FlowMatch } from "$lib/dual-var";
  import Icon from "./Icon.svelte";
  import RichText from "./RichText.svelte";

  let { kind, detail, x, y, flow, quality = "normal", flowRate, rateBasis, match, onEnter, onLeave }: {
    kind: string; detail: PrototypeDetail | null; x: number; y: number;
    flow?: DualVar; quality?: string; flowRate?: number; rateBasis?: "solution" | "unit"; onEnter?: () => void; onLeave?: () => void;
    match?: FlowMatch;
  } = $props();
  const labels: Record<string, string> = {item:"物品",fluid:"流体",recipe:"配方",module:"插件",machine:"制造机","mining-machine":"采矿机","offshore-pump":"抽水机",generator:"发电机",boiler:"锅炉",reactor:"反应堆",beacon:"插件塔",resource:"资源",entity:"实体",technology:"科技",planet:"星球",surface:"地表",quality:"品质",tile:"地格","solar-panel":"太阳能板",accumulator:"蓄电器"};
  const flowHelp: Record<string,string> = {
    Electricity:"电力按功率（W = J/s）结算，不随项目时间刻度改变。", Heat:"热量按热功率（W = J/s）配平。",
    ItemFuel:"抽象燃料需求按热值结算，并要求燃料类别匹配。", ItemFuelSupply:"抽象燃料供给按热值与匹配类别参与配平，不是具体物品数量。",
    FluidFuel:"流体燃料按燃烧热值结算。", FluidHeat:"流体热量取决于比热与相对默认温度的温差。",
    RocketSlotCapacity:"堆叠槽位运力，按物品堆叠数量消耗；与重量运力分别配平。", RocketWeightCapacity:"重量运力以克（g）结算，使用原型指定或由配方推导的重量；与堆叠槽位运力分别配平。",
    VirtualScience:"科研单位由科技瓶品质和科研容量换算，不包含实验室速度、品质或产能。", Pollution:"污染流按项目时间刻度显示；排放与可用的消耗途径参与配平。",
  };
  let flowKey = $derived(typeof flow === "string" ? flow : flow ? Object.keys(flow)[0] : "");
  let flowInfo = $derived(flow ? presentFlow(flow, (kind, name) => runtime.localizedName(kind, name), match) : null);
  let inner = $derived(flow && typeof flow === "object" ? (flow as Record<string, unknown>)[flowKey] as Record<string, unknown> | string : undefined);
  let virtualScience = $derived(flowKey === "VirtualScience");
  let heading = $derived(flow ? flowName(flow, (kind, name) => runtime.richName(kind, name)) : detail?.localized_name || detail?.name || "未指定");
  let iconName = $derived(detail?.name || (virtualScience ? String(inner) : flowKey === "ItemFuelSupply" ? "ItemFuel" : flowKey));
  let iconType = $derived(detail ? iconTypeForKind(detail.kind) : virtualScience ? "item" : "flow");
  let stats = $derived(detailStats(detail, quality));
  const propertyLabels: Record<string,string> = { gravity:"重力",pressure:"气压","magnetic-field":"磁场","solar-power":"太阳能系数","day-night-cycle":"昼夜周期","robot-energy-usage":"机器人能耗系数" };
  let research = $derived(detail?.calculation?.research);
  let properties = $derived(Object.entries(detail?.calculation?.surface_properties ?? {}));
  let tile = $derived(detail?.calculation?.tile);
  let actualPower = $derived(flow ? isEnergyFlow(flow) : false);
  let accessibleState = $state<"accessible" | "inaccessible" | "unknown">("unknown");
  $effect(() => {
    accessibleState = "unknown";
    const current = detail;
    if (!current) return;
    if (runtime.allAccessible) { accessibleState = "accessible"; return; }
    const mapped = accessibleKindFor(current.kind);
    if (!mapped) return;
    let alive = true;
    runtime.ensureAccessibility().then((nodes) => {
      if (alive && nodes) accessibleState = nodes.some((node) => `${accessibleKind(node)}/${accessibleName(node)}` === `${mapped}/${current.name}`) ? "accessible" : "inaccessible";
    });
    return () => { alive = false; };
  });
  let viewport = $state({ width: typeof window === "undefined" ? 1024 : window.innerWidth, height: typeof window === "undefined" ? 768 : window.innerHeight });
  let card = $state<HTMLDivElement>();
  let dimensions = $state({ width: 440, height: 0 });
  $effect(() => {
    if (!card) return;
    const element = card;
    const observer = new ResizeObserver(() => { dimensions = {width:element.offsetWidth,height:element.offsetHeight}; });
    observer.observe(element);
    return () => observer.disconnect();
  });
  // Keep the panel on one side of the pointer; clamping a tall panel over the
  // source icon would make that icon impossible to click.
  let above = $derived(viewport.height - y - 22 < 200 && y > viewport.height - y);
  let maxHeight = $derived(Math.max(80, above ? y - 22 : viewport.height - y - 22));
  let left = $derived(Math.max(8,
    x + 14 + dimensions.width <= viewport.width - 8 ? x + 14 : x - dimensions.width - 14 >= 8 ? x - dimensions.width - 14 : viewport.width - dimensions.width - 8));
  let top = $derived(Math.max(8, above ? y - Math.min(dimensions.height, maxHeight) - 14 : y + 14));
  function portal(node: HTMLElement) { document.body.appendChild(node); return {destroy:() => node.remove()}; }
  function flowNotes(entry: FlowAmount) {
    const notes: string[] = [];
    const qmin = entry.quality_min ? runtime.localizedName("quality", entry.quality_min) : "";
    const qmax = entry.quality_max ? runtime.localizedName("quality", entry.quality_max) : "";
    if (qmin && qmax) notes.push(`品质 ${qmin}${qmax !== qmin ? `～${qmax}` : ""}`);
    else if (qmin) notes.push(`品质下限 ${qmin}`);
    else if (qmax) notes.push(`品质上限 ${qmax}`);
    if (entry.quality_change) notes.push(`品质偏移 ${entry.quality_change > 0 ? "+" : ""}${entry.quality_change}`);
    if (entry.temperature != null) notes.push(`${entry.temperature}℃`);
    if (entry.min_temperature != null || entry.max_temperature != null) notes.push(`${entry.min_temperature ?? "不限"}～${entry.max_temperature ?? "不限"}℃`);
    return notes.join(" · ");
  }
  let scale = $derived(runtime.selectedProject?.settings.time_scale ?? "seconds");
  let factor = $derived(scale === "minutes" ? 60 : scale === "hours" ? 3600 : 1);
  let unit = $derived(scale === "minutes" ? "/min" : scale === "hours" ? "/h" : "/s");
</script>

<svelte:window onresize={() => viewport = {width:window.innerWidth,height:window.innerHeight}} />

{#if detail || flow}
  <!-- The panel can be entered to scroll or select long calculation details. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div use:portal bind:this={card} class="hover-card" role="tooltip" style:left={`${left}px`} style:top={`${top}px`} style:max-height={`${maxHeight}px`} onmouseenter={onEnter} onmouseleave={onLeave}>
    <header>
      <Icon type={iconType} name={iconName} size={30} />
      <div class="heading"><strong>{#if flowKey === "Custom"}{heading}{:else}<RichText text={heading} />{/if}</strong><code>{flowInfo?.identifier || detail?.name || flowKey}</code></div>
      <div class="badges"><span>{flowInfo?.kindLabel || labels[kind] || "计算流"}</span>
        {#if match}<span>筛选条件</span>{/if}
        {#if quality !== "normal"}<span><RichText text={runtime.richName("quality",quality)} /></span>{/if}
        {#if detail?.hidden}<span>隐藏</span>{/if}
        {#if accessibleState !== "unknown"}<span class:unavailable={accessibleState === "inaccessible"}>{accessibleState === "accessible" ? "可达" : "不可达"}</span>{/if}
      </div>
    </header>
    {#if flowRate != null}
      <div class="actual"><span>{rateBasis === "unit" ? "单位机制" : "实际"}{flowRate < 0 || Object.is(flowRate,-0) ? "消耗" : "产出"}</span><strong>{actualPower ? formatStat({key:"flow",value:Math.abs(flowRate),unit:"watts"}) : `${compactNumber(Math.abs(flowRate) * factor)} ${unit}`}</strong></div>
    {/if}
    {#if flowHelp[flowKey]}<p class="note">{flowHelp[flowKey]}</p>{/if}
    {#if flowInfo?.facts.length}
      <div class="facts">
        {#each flowInfo.facts as fact}<span>{fact.label}</span><strong>{fact.value}</strong>{/each}
      </div>
    {/if}
    {#if tile}
      <section class="tile-capabilities"><strong>地格用途</strong>
        <div><span>抽取流体</span>{#if tile.fluid}<Icon type="fluid" name={tile.fluid} size={20} /><b><RichText text={runtime.richName("fluid",tile.fluid)} /></b>{:else}<b>不支持</b>{/if}</div>
        <div><span>销毁物品</span><b>{tile.destroys_dropped_items ? "支持" : "不支持"}</b></div>
      </section>
    {/if}
    {#if properties.length}
      <section><h4>表面属性<small>用于环境条件匹配的原型值</small></h4><div class="stats">{#each properties as [key,value]}<div><span><RichText text={runtime.richName("surface-property",key) !== key ? runtime.richName("surface-property",key) : propertyLabels[key] || key} /></span><strong>{compactNumber(value)}</strong></div>{/each}</div></section>
    {/if}
    {#if research}
      <section><h4>科研需求<small>未计实验室速度、品质与产能</small></h4>
        <div class="facts"><span>研究单位数</span><strong>{research.units == null ? "由等级公式决定" : compactNumber(research.units)}</strong><span>每单位时间（速度 1）</span><strong>{compactNumber(research.unit_seconds)} s</strong></div>
        {#each research.ingredients as ingredient}<div class="flow-row"><Icon type="item" name={ingredient.name} size={22} /><span class="flow-copy"><RichText text={runtime.richName("item",ingredient.name)} /></span><span class="quantity">{ingredient.per_unit}/单位{#if ingredient.total != null}<small>共 {compactNumber(ingredient.total)}</small>{/if}</span></div>{/each}
        {#if research.prerequisites.length}<div class="facts"><span>前置科技</span><strong>{research.prerequisites.map((name) => runtime.localizedName("technology",name)).join(" / ")}</strong></div>{/if}
      </section>
    {/if}
    {#if detail?.calculation?.item_weight}
      {@const weight = detail.calculation.item_weight}
      <div class="facts"><span>重量来源</span><strong>{weight.source === "explicit" ? "原型指定" : weight.source === "cursor" ? "光标生成物品（零重量）" : weight.source === "recipe" ? "由配方推导" : "默认重量"}</strong></div>
      {#if weight.recipe}<div class="flow-row"><Icon type="recipe" name={weight.recipe} size={22} /><RichText text={runtime.richName("recipe", weight.recipe)} /></div>{/if}
      {#if weight.fallback}<div class="facts"><span>回退原因</span><strong>{({"no-recipe":"无可用于重量计算的配方", "recipe-cycle":"配方依赖循环", "zero-recipe-weight":"原料总重量为零", "zero-product-count":"物品产物期望数量为零", "invalid-recipe-weight":"无法推导有效重量"} as Record<string,string>)[weight.fallback] ?? weight.fallback}</strong></div>{/if}
      <p class="weight-note">重量不随品质变化；载量按默认火箭运力估算，发射时按火箭仓实际运力结算。</p>
    {/if}
    {#if stats.length}
      <section><h4>{detail?.calculation?.qualities.some((entry) => entry.quality === quality) ? `${runtime.localizedName("quality", quality)}基础值` : "原型基础值"}<small>{match && flowInfo?.wildcard && flowInfo.kind !== "Fluid" ? "普通品质参考值；匹配不限品质" : "未计插件与工厂环境"}</small></h4>
        <div class="stats">{#each stats as stat (stat.key)}<div><span>{statLabels[stat.key] || stat.key}</span><strong>{formatStat(stat)}{stat.key.includes("temperature") ? "℃" : ""}</strong></div>{/each}</div>
      </section>
    {/if}
    {#if detail}
      <div class="facts">
        {#if detail.categories?.length}<span>类别</span><strong><RichText text={detail.categories.map(id=>runtime.richName(kind === "resource" || kind === "mining-machine" ? "resource-category" : "recipe-category", id)).join(" / ")} /></strong>{/if}
        {#if detail.surface_conditions?.length}<span>表面条件</span><strong>{detail.surface_conditions.join("；")}</strong>{/if}
        {#if detail.allowed_module_categories?.length}<span>允许插件类别</span><strong><RichText text={detail.allowed_module_categories.map(id=>runtime.richName("module-category",id)).join(" / ")} /></strong>{/if}
        {#if detail.uses_module_effects != null}<span>机器插件 / 插件塔</span><strong>{detail.uses_module_effects ? "接受" : "不接受"} / {detail.uses_beacon_effects !== false ? "接受" : "不接受"}</strong>{/if}
        {#if detail.fuel_categories?.length}<span>燃料类别</span><strong><RichText text={detail.fuel_categories.map(id=>runtime.richName("fuel-category",id)).join(" / ")} /></strong>{/if}
        {#if detail.burner_fuel_categories?.length}<span>可用燃料</span><strong><RichText text={detail.burner_fuel_categories.map(id=>runtime.richName("fuel-category",id)).join(" / ")} /></strong>{/if}
        {#if detail.fluid_filter}<span>流体限制</span><strong><RichText text={runtime.richName("fluid",detail.fluid_filter)} /></strong>{/if}
        {#if detail.boiler_mode}<span>锅炉模式</span><strong>{detail.boiler_mode === "heat-fluid-inside" ? "加热流体" : "独立输出"}</strong>{/if}
        {#if detail.quality_next}<span>下一品质</span><strong><RichText text={runtime.richName("quality",detail.quality_next)} /> · {compactNumber((detail.quality_next_probability ?? 0) * 100)}%</strong>{/if}
      </div>
      {#each [{label:"燃烧产物",kind:"item",name:detail.burnt_result},{label:"变质产物",kind:"item",name:detail.spoil_result},{label:"种植实体",kind:"entity",name:detail.plant_result},...(detail.rocket_launch_products ?? []).map((name) => ({label:"火箭产物",kind:"item",name}))] as reference}
        {#if reference.name}<div class="reference"><span>{reference.label}</span><Icon type={reference.kind} name={reference.name} size={20} /><strong><RichText text={runtime.richName(reference.kind,reference.name)} /></strong></div>{/if}
      {/each}
      {#each [{label:"原料",items:detail.ingredients ?? [],output:false},{label:"产物",items:detail.results ?? [],output:true}] as section}
        {#if section.items.length}
          <section class="recipe-flows"><h4>{section.label}<small>期望量 / 次 · 速度 1 时的基准速率</small></h4>
            <div class="flow-table">
              {#each section.items as entry, i (`${entry.kind}/${entry.name}#${i}`)}
                <div class="flow-row" title={flowNotes(entry)}>
                  <Icon type={entry.kind} name={entry.name} size={22} />
                  <div class="flow-copy"><strong><RichText text={runtime.richName(entry.kind, entry.name)} /></strong>{#if flowNotes(entry)}<small>{flowNotes(entry)}</small>{/if}</div>
                  <div class="quantity"><strong>{#if section.output}{compactNumber(entry.amount - entry.productivity)} + {compactNumber(entry.productivity)}{:else}{compactNumber(entry.amount)}{/if}</strong>{#if entry.base_rate != null}<small>{compactNumber(entry.base_rate)}/s</small>{/if}</div>
                </div>
              {/each}
            </div>
          </section>
        {/if}
      {/each}
      {#if detail.results?.length}<p class="note">产物数量 = 不受产能加成部分 + 可享受产能加成部分，均为期望数量；右侧速率为两部分之和。</p>{/if}
    {/if}
  </div>
{/if}

<style>
  .hover-card { position: fixed; z-index: 80; width: min(440px, calc(100vw - 16px)); max-height: calc(100dvh - 16px); overflow: auto; padding: 10px; color: var(--text); background: var(--panel); border: 1px solid var(--line-strong); border-radius: var(--radius); box-shadow: var(--shadow-strong); text-align: left; font-size: 11px; line-height: 1.35; }
  header { display: flex; align-items: center; gap: 8px; padding-bottom: 7px; border-bottom: 1px solid var(--line); }
  .heading { min-width: 0; flex: 1; display: grid; gap: 2px; }
  .heading strong { font-size: 13px; overflow-wrap: anywhere; }
  code { font: 10px var(--mono); color: var(--muted); overflow-wrap: anywhere; }
  .badges { display: grid; gap: 3px; flex: 0 0 auto; font-size: 10px; text-align: right; color: var(--muted); }
  .unavailable { color: var(--warn); }
  section { margin-top: 8px; }
  h4 { display: flex; align-items: baseline; flex-wrap: wrap; gap: 6px; margin: 0 0 5px; font-size: 11px; }
  h4 small { color: var(--muted); font-size: 10px; font-weight: 400; }
  .weight-note { margin: 3px 0 6px; color: var(--muted); font-size: 11px; line-height: 1.4; }
  .stats { display: grid; grid-template-columns: repeat(2, minmax(0,1fr)); gap: 3px 10px; }
  .stats > div { display: flex; align-items: baseline; justify-content: space-between; gap: 5px; padding: 3px 0; border-bottom: 1px solid var(--line); min-width: 0; }
  .stats span, .facts > span, .reference > span, .tile-capabilities span { color: var(--muted); }
  .stats strong, .quantity, .actual strong { font-family: var(--mono); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .facts { display: grid; grid-template-columns: auto minmax(0,1fr); gap: 4px 12px; margin-top: 7px; }
  .facts strong { text-align: right; overflow-wrap: anywhere; font-weight: 500; }
  .actual { display: flex; justify-content: space-between; margin-top: 7px; padding: 5px 7px; background: var(--accent-dim); border-radius: var(--radius-sm); }
  .tile-capabilities > div, .reference { display: flex; align-items: center; gap: 6px; margin-top: 5px; }
  .tile-capabilities span, .reference > span { min-width: 72px; }
  .flow-table { display: grid; }
  .flow-row { display: flex; align-items: center; gap: 6px; padding: 4px 0; border-top: 1px solid var(--line); }
  .flow-copy { flex: 1; min-width: 0; display: grid; gap: 2px; }
  .flow-copy strong { font-weight: 500; overflow-wrap: anywhere; }
  .flow-copy small { color: var(--muted); font-size: 10px; overflow-wrap: anywhere; }
  .quantity { display: grid; gap: 2px; text-align: right; }
  .quantity small, .note { color: var(--muted); font-size: 10px; }
  .note { margin: 6px 0 0; }
  @media (max-width: 460px) { .stats { grid-template-columns: 1fr; } }
</style>

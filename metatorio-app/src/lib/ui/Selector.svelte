<script lang="ts">
  // 原型/流选择器：
  // - 目录模式（flowMode=false）：复刻原始实现——大组分 tab、小组换行，
  //   筛选/分组/排序全部前端本地（索引由后端一次性下发并缓存），悬停显示卡片。
  // - 流模式（flowMode=true）：选择 DualVar 流（物品/流体/实体走目录，
  //   电/热/火箭运力直接添加，自定义/污染输入名称）。
  //
  // 带品质的场景（物品/配方/实体类）支持"物品 + 品质"任意顺序选择：
  // 点击条目只是选中（高亮，不退出），点品质 chips 设定品质，最后按
  // 「确定」提交（双击条目 = 立即提交）；「取消」/Esc/点背景 = 提前退出。
  import { onDestroy, onMount, tick, untrack } from "svelte";
  import { createBackdropClose } from "./modal-backdrop";
  import {
    dismissHover,
    hoverOpenDelay,
    markHoverActive,
    markHoverInactive,
    registerHoverDismiss,
  } from "./hover-controller";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import HoverIcon from "./HoverIcon.svelte";
  import Icon from "./Icon.svelte";
  import HoverCard from "./HoverCard.svelte";
  import RichText from "./RichText.svelte";
  import FlowRef from "./FlowRef.svelte";
  import { normalizeSearch, richTextPlain } from "$lib/rich-text";
  import type { CatalogKind, DualVar, IndexEntry } from "$lib/runtime/types";
  import {
    accessibleKind,
    accessibleKindFor as catalogAccessibleKindFor,
    accessibleName,
  } from "$lib/runtime/types";

  let {
    kind,
    title,
    kindOptions = [],
    flowMode = false,
    categoryFilter,
    tilePurpose,
    allowedNames,
    respectAccessibility = true,
    multiLevelTechOnly = false,
    initialTab,
    initialName,
    initialQuality,
    onSelect,
    onSelectFlow,
    onPickKind,
    onClose,
  }: {
    kind: CatalogKind;
    title: string;
    kindOptions?: { kind: CatalogKind; label: string }[];
    flowMode?: boolean;
    /** 机器/资源类目录的类别过滤（如配方 categories）；空数组=不过滤。 */
    categoryFilter?: string[];
    tilePurpose?: "extract" | "dispose";
    /** 精确名称过滤（如发电机/锅炉的流体箱过滤）；空数组=不过滤。 */
    allowedNames?: string[];
    /** 按项目可达性过滤（不可达条目隐藏）；项目"无视可达性"时自动跳过。
     *  需要完全不受可达性约束的调用方置 false。 */
    respectAccessibility?: boolean;
    /** 仅显示"可多次研究"的科技（无限科技选择器用）：max_level 为
     *  null（无限）或 >= 2；普通/单次研究科技隐藏。 */
    multiLevelTechOnly?: boolean;
    /** 流模式初始页签（编辑目标/外部输入时预选当前流类型）。 */
    initialTab?: string;
    /** 目录模式初始选中（编辑时预选当前条目）。 */
    initialName?: string;
    /** 初始品质。 */
    initialQuality?: string;
    onSelect?: (name: string, quality: string) => void;
    onSelectFlow?: (flow: DualVar) => void;
    /** 目录模式（含多分类页签）选中时回调，额外返回当前分类 kind——
     *  用于"选择里程碑/标记可达"这类需要按原型类型构造对象的场景；
     *  存在时优先于 onSelect。 */
    onPickKind?: (kind: CatalogKind, name: string, quality: string) => void;
    onClose: () => void;
  } = $props();

  // 支持品质的目录 kind（物品/配方/实体类）
  const qualityKinds = new Set([
    "item",
    "recipe",
    "entity",
    "machine",
    "mining-machine",
    "generator",
    "boiler",
    "reactor",
    "fusion-reactor",
    "fusion-generator",
    "solar-panel",
    "accumulator",
    "beacon",
    "resource",
    "module",
  ]);

  const flowTabs = [
    { id: "item", label: "物品" },
    { id: "virtual-science", label: "虚拟科研" },
    { id: "fluid", label: "流体" },
    { id: "entity", label: "实体" },
    { id: "electricity", label: "电" },
    { id: "heat", label: "热" },
    { id: "rocket-slot", label: "火箭·堆叠" },
    { id: "rocket-weight", label: "火箭·重量" },
    { id: "custom", label: "自定义" },
  ] as const;
  type FlowTab = (typeof flowTabs)[number]["id"];

  const directFlows: Partial<Record<FlowTab, string>> = {
    electricity: "Electricity",
    heat: "Heat",
    "rocket-slot": "RocketSlotCapacity",
    "rocket-weight": "RocketWeightCapacity",
  };

  // 每次挂载的选择器对应一次独立编辑会话：props 仅初始化一次本地草稿，切换编辑对象需通过 {#key} 重建。
  let activeKind = $state<CatalogKind>(untrack(() => kind));
  let flowTab = $state<FlowTab>(
    untrack(() => (flowTabs.some((tab) => tab.id === initialTab) ? initialTab : "item") as FlowTab),
  );
  let customVariant = $state<"Pollution" | "Custom">("Custom");
  let customName = $state("");
  let quality = $state(untrack(() => initialQuality || "normal"));
  /** 用户是否主动点过品质 chips（区别于默认 normal）——点过再点条目即自动确认。 */
  let qualityPicked = $state(false);
  let query = $state("");
  let activeGroup = $state<string | null>(null);
  let loading = $state(false);
  let searchBox = $state<HTMLInputElement | null>(null);
  let bodyEl = $state<HTMLDivElement | null>(null);
  /** 已选中（待确认）的条目名；null = 未选。 */
  let selected = $state<string | null>(untrack(() => initialName || null));
  let highlightedIndex = $state(-1);

  let hover = $state<{ kind: string; name: string } | null>(null);
  let hoverPos = $state({ x: 0, y: 0 });
  let hoverDetail = $state<import("$lib/runtime/types").PrototypeDetail | null>(null);
  const selectorBackdrop = createBackdropClose(() => onClose());

  // 目录模式下用 activeKind；流模式下仅 item/fluid/entity 走目录
  let catalogKind = $derived(
    flowMode
      ? flowTab === "virtual-science" ? "item" : flowTab === "item" || flowTab === "fluid" || flowTab === "entity"
        ? (flowTab as CatalogKind)
        : null
      : activeKind,
  );
  let entries = $derived(
    catalogKind
      ? (runtime.catalogIndex?.entries ?? []).filter((entry) => entry.kind === catalogKind
          && (!flowMode || flowTab !== "virtual-science" || entry.categories.includes("science-pack")))
      : [],
  );
  let groups = $derived([...new Set(entries.map((entry) => entry.group))]);
  let searching = $derived(query.trim().length > 0);
  // 仅在目录或筛选上下文变化时重建索引，避免每次按键重复遍历。
  let searchIndex = $derived(entries.map((entry) => ({ entry, keys: [
    normalizeSearch(entry.name),
    normalizeSearch(entry.localized_name, (kind, name) => runtime.localizedName(kind, name)),
    normalizeSearch(entry.localized_name), // 内嵌原型 ID 也作为别名参与检索。
    entry.tile?.fluid ? normalizeSearch(`${entry.tile.fluid} ${runtime.localizedName("fluid", entry.tile.fluid)}`) : "",
  ] })));
  let searchNeedle = $derived(normalizeSearch(query));

  // ── 可达性过滤 ──────────────────────────────────────────────────
  // 选中项目"无视可达性"时不拉取也不过滤；否则惰性拉取可达性快照
  // （`kind/name` 集合）。未就绪（null）时不过滤，避免误隐藏。
  let accessibleKeys = $state<Set<string> | null>(null);
  $effect(() => {
    if (respectAccessibility && !runtime.allAccessible) {
      runtime.ensureAccessibility().then((nodes) => {
        accessibleKeys =
          nodes == null
            ? null
            : new Set(
                nodes.map((node) => `${accessibleKind(node)}/${accessibleName(node)}`),
              );
      });
    } else {
      accessibleKeys = null;
    }
  });
  /** 目录 kind → 可达性 kind（见 types.accessibleKindFor；无对应则不过滤）。 */
  const accessibleKindFor = catalogAccessibleKindFor;

  let allowedNameSet = $derived(allowedNames && allowedNames.length > 0 ? new Set(allowedNames) : null);
  let categoryFilterSet = $derived(categoryFilter && categoryFilter.length > 0 ? new Set(categoryFilter) : null);

  let visible = $derived(
    (searching
      ? searchIndex.filter(({ keys }) => !!searchNeedle && keys.some((key) => key.includes(searchNeedle))).map(({ entry }) => entry)
      : activeGroup
        ? entries.filter((entry) => entry.group === activeGroup)
        : entries
    ).filter((entry) => {
      if (tilePurpose && entry.kind === "tile") {
        if (tilePurpose === "extract" ? !entry.tile?.fluid : !entry.tile?.destroys_dropped_items) return false;
      }
      if (allowedNameSet && !allowedNameSet.has(entry.name)) {
        return false;
      }
      // 无限科技选择器：只保留可多次研究的科技——无限（max=null），
      // 或 max_level > 最低等级（有实际多级研究空间）。
      if (multiLevelTechOnly && entry.kind === "technology") {
        const max = entry.technology_max_level;
        const base = entry.technology_base_level;
        if (max != null && max <= base) return false;
      }
      if (respectAccessibility && accessibleKeys != null) {
        const mapped = accessibleKindFor(entry.kind);
        if (mapped != null && !accessibleKeys.has(`${mapped}/${entry.name}`)) return false;
      }
      if (!categoryFilterSet || !catalogKind) return true;
      // 与后端 categories_overlap 一致：空类别条目在过滤时视为不匹配
      return entry.categories.some((category) => categoryFilterSet.has(category));
    }),
  );
  let bySubgroup = $derived(groupBySubgroup(visible));
  let flatVisible = $derived(
    searching || catalogKind === "tile"
      ? visible
      : bySubgroup.flatMap((section) => section.items),
  );
  let highlightedEntry = $derived(
    highlightedIndex >= 0 && highlightedIndex < flatVisible.length
      ? flatVisible[highlightedIndex]
      : null,
  );
  let isDirect = $derived(flowMode && flowTab in directFlows);
  let isCustom = $derived(flowMode && flowTab === "custom");
  // 品质适用的场景：目录模式且 kind 支持品质；流模式下 item/entity 页签。
  let qualityApplies = $derived(
    (flowMode
      ? flowTab === "item" || flowTab === "entity"
      : catalogKind != null && qualityKinds.has(catalogKind)),
  );
  /** 真正"有品质可选"：品质适用且品质链多于 normal 一种。
   *  只有 normal 时隐藏品质行与确认按钮，点击条目直接确认。 */
  let hasQuality = $derived(
    qualityApplies && (runtime.catalogIndex?.qualities ?? []).length > 1,
  );
  /** 编辑模式（更改目标/外部输入时预置了初始条目/品质）：
   *  预置不算"点过"，自动确认按"两个都没点过"判定。 */
  let presetMode = $derived(initialName != null || initialQuality != null);
  let selectedEntry = $derived(
    selected ? visible.find((entry) => entry.name === selected) ?? null : null,
  );
  /** 当前品质的本地化名（normal 或未选中时不展示）。 */
  let qualityLabel = $derived(
    quality !== "normal" ? runtime.localizedName("quality", quality) : "",
  );

  function groupBySubgroup(list: IndexEntry[]): { subgroup: string; items: IndexEntry[] }[] {
    const map = new Map<string, IndexEntry[]>();
    for (const entry of list) {
      const key = entry.subgroup || "其他";
      const arr = map.get(key) ?? [];
      arr.push(entry);
      map.set(key, arr);
    }
    return [...map.entries()].map(([subgroup, items]) => ({ subgroup, items }));
  }

  $effect(() => {
    searchBox?.focus();
  });

  $effect(() => {
    const len = flatVisible.length;
    if (untrack(() => highlightedIndex) >= len) {
      highlightedIndex = len > 0 ? len - 1 : -1;
    }
  });

  // 上下文变化时重新拉取索引。
  $effect(() => {
    const ctxId = runtime.activeContext?.id;
    loading = true;
    let alive = true;
    runtime.loadCatalogIndex().then(() => {
      if (alive) loading = false;
    });
    return () => {
      alive = false;
    };
  });

  // 悬停详情（防抖由缓存兜底，首次 hover 拉取一次；仅依赖条目本身，不随坐标变化重拉）。
  $effect(() => {
    if (!hover) {
      hoverDetail = null;
      return;
    }
    const { kind: entryKind, name: entryName } = hover;
    let alive = true;
    hoverDetail = null;
    runtime.getDetail(entryKind, entryName).then((detail) => {
      if (alive) hoverDetail = detail;
    });
    return () => {
      alive = false;
    };
  });

  function resetView() {
    activeGroup = null;
    query = "";
    selected = null;
    highlightedIndex = -1;
  }

  function labelOf(entry: IndexEntry): string {
    return richTextPlain(entry.localized_name || entry.name, (kind, name) => runtime.localizedName(kind, name)).trim();
  }

  /** 点击条目：无品质 → 点击即确认；有品质 → 仅选中（品质主动点过且
   *  非编辑模式时"物品+品质"齐了直接确认）；编辑模式按"两个都没点过"。 */
  function toggleSelect(name: string) {
    if (selected === name) {
      selected = null;
      return;
    }
    selected = name;
    if (!hasQuality || (!presetMode && qualityPicked)) {
      confirm();
    }
  }

  /** 点击品质 chip（仅在有品质时出现）：设定品质；条目已选中且非编辑
   *  模式时"物品+品质"齐了，直接确认。 */
  function pickQuality(name: string) {
    quality = name;
    qualityPicked = true;
    if (selected && !presetMode) {
      confirm();
    }
  }

  /** 提交当前选择（条目 + 当前品质）。 */
  async function confirm() {
    if (!selected || !selectedEntry) return;
    if (flowMode && onSelectFlow) {
      const flow = await buildCatalogFlow(selected);
      if (flow) {
        onSelectFlow(flow);
        onClose();
      }
      return;
    }
    if (onPickKind) {
      onPickKind(activeKind, selected, quality);
    } else {
      onSelect?.(selected, quality);
    }
    onClose();
  }

  /** 双击：立即提交。 */
  function commitFast(name: string) {
    selected = name;
    confirm();
  }

  async function buildCatalogFlow(name: string): Promise<DualVar | null> {
    switch (flowTab) {
      case "virtual-science":
        return { VirtualScience: name };
      case "item":
        return { Item: { id: name, quality } };
      case "fluid": {
        const detail = await runtime.getDetail("fluid", name);
        // 默认取该流体的**完整可行温度区间** [default_temperature, max_temperature]：
        // 下游任何落在区间内的产出都能满足它（靠转换图的 [T,T] ⊆ [T1,T2] 子类型规则），
        // 所以新建流体流不必先猜一个温度点——猜错会让目标变成"没人能产出的那个温度"。
        const low =
          detail?.default_temperature != null ? Math.round(detail.default_temperature) : 0;
        const high =
          detail?.maximum_temperature != null ? Math.round(detail.maximum_temperature) : low;
        return { Fluid: { name, temperature: [low, Math.max(low, high)] } };
      }
      case "entity":
        return { Entity: { id: name, quality } };
      default:
        return null;
    }
  }

  function commitDirect(flow: string) {
    if (!onSelectFlow) return;
    onSelectFlow(flow as DualVar);
    onClose();
  }

  function commitCustom() {
    const name = customName.trim();
    if (!name || !onSelectFlow) return;
    onSelectFlow(
      customVariant === "Pollution" ? { Pollution: { name } } : { Custom: { name } },
    );
    onClose();
  }

  const hoverToken = Symbol("selector-hover");
  let hoverOpenTimer: ReturnType<typeof setTimeout> | undefined;
  let hoverLeaveTimer: ReturnType<typeof setTimeout> | undefined;
  let hoverRafId: number | undefined;
  let pendingHoverPos: { x: number; y: number } | null = null;
  let pendingHoverTarget: { kind: string; name: string } | null = null;
  let inHoverLeaveBuffer = false;
  let insideHoverCard = false;

  function cancelHoverRaf() {
    if (hoverRafId != null) {
      cancelAnimationFrame(hoverRafId);
      hoverRafId = undefined;
    }
    pendingHoverPos = null;
  }

  function closeSelectorHover() {
    clearTimeout(hoverOpenTimer);
    hoverOpenTimer = undefined;
    clearTimeout(hoverLeaveTimer);
    hoverLeaveTimer = undefined;
    cancelHoverRaf();
    pendingHoverTarget = null;
    inHoverLeaveBuffer = false;
    insideHoverCard = false;
    if (hover) {
      hover = null;
      markHoverInactive(hoverToken);
    }
  }

  function keepHover() {
    clearTimeout(hoverLeaveTimer);
    hoverLeaveTimer = undefined;
    inHoverLeaveBuffer = false;
    insideHoverCard = true;
  }

  function leaveHoverCard() {
    insideHoverCard = false;
    leaveHover();
  }

  function leaveHover() {
    clearTimeout(hoverOpenTimer);
    hoverOpenTimer = undefined;
    pendingHoverTarget = null;
    cancelHoverRaf();
    if (!hover) return;
    inHoverLeaveBuffer = true;
    clearTimeout(hoverLeaveTimer);
    hoverLeaveTimer = setTimeout(() => {
      hoverLeaveTimer = undefined;
      closeSelectorHover();
    }, 180);
  }

  onDestroy(closeSelectorHover);
  onMount(() => {
    dismissHover();
    return registerHoverDismiss(closeSelectorHover);
  });

  function scheduleHoverPos(x: number, y: number) {
    pendingHoverPos = { x, y };
    if (hoverRafId == null) {
      hoverRafId = requestAnimationFrame(() => {
        hoverRafId = undefined;
        if (pendingHoverPos && !inHoverLeaveBuffer && !insideHoverCard) {
          hoverPos = pendingHoverPos;
        }
        pendingHoverPos = null;
      });
    }
  }

  function showHover(event: MouseEvent, entry: IndexEntry) {
    clearTimeout(hoverLeaveTimer);
    hoverLeaveTimer = undefined;
    inHoverLeaveBuffer = false;
    insideHoverCard = false;

    if (hover?.kind === entry.kind && hover.name === entry.name) {
      scheduleHoverPos(event.clientX, event.clientY);
      return;
    }

    if (pendingHoverTarget?.kind === entry.kind && pendingHoverTarget.name === entry.name) {
      scheduleHoverPos(event.clientX, event.clientY);
      return;
    }

    clearTimeout(hoverOpenTimer);
    hoverOpenTimer = undefined;
    pendingHoverTarget = { kind: entry.kind, name: entry.name };
    cancelHoverRaf();
    hoverPos = { x: event.clientX, y: event.clientY };

    const delay = hoverOpenDelay();
    if (delay <= 0) {
      pendingHoverTarget = null;
      dismissHover();
      hover = { kind: entry.kind, name: entry.name };
      markHoverActive(hoverToken);
    } else {
      hoverOpenTimer = setTimeout(() => {
        hoverOpenTimer = undefined;
        const target = pendingHoverTarget;
        pendingHoverTarget = null;
        if (!target) return;
        dismissHover();
        hover = target;
        markHoverActive(hoverToken);
      }, delay);
    }
  }

  function scrollHighlightedIntoView() {
    void tick().then(() => {
      bodyEl
        ?.querySelector<HTMLElement>('[data-kbd-active="true"]')
        ?.scrollIntoView({ block: "nearest", inline: "nearest" });
    });
  }

  function handleQualityShortcut(event: KeyboardEvent): boolean {
    if (!event.altKey || event.ctrlKey || event.metaKey || !hasQuality) return false;
    const qualities = runtime.catalogIndex?.qualities ?? [];
    const digit = /^[1-5]$/.test(event.key)
      ? Number(event.key)
      : /^Digit([1-5])$/.test(event.code)
        ? Number(event.code.slice(5))
        : 0;
    if (digit < 1 || digit > qualities.length) return false;
    event.preventDefault();
    event.stopPropagation();
    quality = qualities[digit - 1];
    qualityPicked = true;
    return true;
  }

  function onSearchKeydown(event: KeyboardEvent) {
    if (event.isComposing) return;
    if (handleQualityShortcut(event)) return;
    if (event.key === "ArrowDown") {
      if (flatVisible.length === 0) return;
      event.preventDefault();
      highlightedIndex =
        highlightedIndex < 0 ? 0 : (highlightedIndex + 1) % flatVisible.length;
      scrollHighlightedIntoView();
      return;
    }
    if (event.key === "ArrowUp") {
      if (flatVisible.length === 0) return;
      event.preventDefault();
      highlightedIndex =
        highlightedIndex <= 0 ? flatVisible.length - 1 : highlightedIndex - 1;
      scrollHighlightedIntoView();
      return;
    }
    if (event.key === "Enter") {
      const target =
        highlightedEntry ??
        (searching ? flatVisible[0] : selectedEntry ?? flatVisible[0]);
      if (target) {
        event.preventDefault();
        event.stopPropagation();
        commitFast(target.name);
      }
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (handleQualityShortcut(event)) return;
    const target = event.target as HTMLElement | null;
    const typing =
      !!target &&
      (target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target instanceof HTMLSelectElement);
    if (event.key === "Escape") {
      event.stopPropagation();
      onClose();
      return;
    }
    // E 键确认（与游戏一致）：输入框里 e 是打字字符，不触发。
    // 品质+物品都点过时默认已自动确认，E 用于只选了物品/直接确认的场景；
    // 无品质模式下点击即确认，E 确认当前悬停（或第一条）条目。
    if (event.key === "e" || event.key === "E") {
      if (typing) return;
      event.stopPropagation();
      if (isDirect) {
        const flow = directFlows[flowTab]!;
        commitDirect(flow);
      } else if (isCustom) {
        commitCustom();
      } else if (selected) {
        confirm();
      } else if (!hasQuality && visible.length > 0) {
        const hovered = hover && visible.some((entry) => entry.name === hover?.name);
        selected = hovered ? hover!.name : visible[0].name;
        confirm();
      }
      return;
    }
    if (
      event.key === "Enter" &&
      !typing &&
      !(target instanceof HTMLButtonElement) &&
      selected &&
      !isDirect &&
      !isCustom
    ) {
      event.stopPropagation();
      confirm();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="backdrop" role="button" tabindex="0" {...selectorBackdrop}>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
  >
    {#if tilePurpose}<div class="filter-hint">{tilePurpose === "extract" ? "仅显示能产出流体的地格" : "仅显示允许销毁物品的地格"} · {visible.length} 项</div>{/if}
    <div class="modal-head">
      <div class="modal-title">
        {title}
        {#if loading}<span class="spinner"></span>{/if}
      </div>
      <button class="btn ghost" title="关闭选择器（Esc）" onclick={onClose}>关闭</button>
    </div>

    {#if flowMode}
      <div class="kind-tabs">
        {#each flowTabs as option (option.id)}
          <button
            class:active={flowTab === option.id}
            class="tab"
            title={`选择${option.label}类流`}
            onclick={() => {
              flowTab = option.id;
              resetView();
            }}
          >{option.label}</button>
        {/each}
      </div>
    {:else if kindOptions.length > 0}
      <div class="kind-tabs">
        {#each kindOptions as option (option.kind)}
          <button
            class:active={activeKind === option.kind}
            class="tab"
            title={`浏览${option.label}目录`}
            onclick={() => {
              activeKind = option.kind;
              resetView();
            }}
          >{option.label}</button>
        {/each}
      </div>
    {/if}

    {#if !isDirect && !isCustom}
      <input
        bind:this={searchBox}
        bind:value={query}
        class="search"
        placeholder="搜索内部 id 或中文名…"
        title="按内部 id 或中文名搜索原型"
        aria-label="搜索原型"
        spellcheck="false"
        oninput={() => (highlightedIndex = -1)}
        onkeydown={onSearchKeydown}
      />

      {#if hasQuality}
        <div class="quality-row">
          <span class="q-label">品质</span>
          {#each runtime.catalogIndex!.qualities as q (q)}
            <button
              class:active={quality === q}
              class="q-chip"
              aria-label={runtime.localizedName("quality", q)}
              onclick={() => pickQuality(q)}
            >
              <HoverIcon type="quality" name={q} size={22} detailKind="quality" title={runtime.localizedName("quality", q)} />
              <span class="q-name"><RichText text={runtime.richName("quality", q)} /></span>
            </button>
          {/each}
        </div>
      {/if}

      {#if !searching && groups.length > 1}
        <!-- 大组用**物品大组图标**（2 倍于条目图标 34px）而不是文字：文字是内部 id
             （`intermediate-products` 这种），图标才是游戏里那套。中文名放 tooltip。 -->
        <div class="group-tabs">
          {#each groups as group (group)}
            <button
              class:active={activeGroup === group}
              class="gtab"
              title={`${runtime.localizedName("item-group", group)}（${group}）`}
              aria-label={runtime.localizedName("item-group", group)}
              onclick={() => {
                activeGroup = activeGroup === group ? null : group;
                highlightedIndex = -1;
              }}
            >
              <Icon
                type="item-group"
                name={group}
                size={68}
                title={runtime.localizedName("item-group", group)}
              />
            </button>
          {/each}
        </div>
      {/if}
    {/if}

    <div class="body" bind:this={bodyEl}>
      {#if isDirect}
        {@const flow = directFlows[flowTab]!}
        <div class="direct-panel">
          <FlowRef flow={flow as DualVar} size={40} />
          <div class="direct-copy">
            <small>点击添加该流</small>
          </div>
          <button class="btn primary" onclick={() => commitDirect(flow)}>添加</button>
        </div>
      {:else if isCustom}
        <div class="custom-panel">
          <select bind:value={customVariant} title="选择自定义流类型" aria-label="自定义流类型">
            <option value="Custom">自定义</option>
            <option value="Pollution">污染</option>
          </select>
          <input bind:value={customName} placeholder="流名称" title="自定义流名称" aria-label="自定义流名称" spellcheck="false" />
          <button class="btn primary" onclick={commitCustom} disabled={!customName.trim()}>添加</button>
        </div>
      {:else if visible.length === 0}
        <div class="empty">
          {loading
            ? "加载中…"
            : categoryFilter && categoryFilter.length > 0 && entries.length > 0
              ? "没有适配当前配方/资源的选项"
              : "没有匹配的原型"}
        </div>
      {:else if searching || catalogKind === "tile"}
        <div class="search-list">
          {#each visible as entry (entry.kind + entry.name)}
            {@const isKbdActive = highlightedEntry?.kind === entry.kind && highlightedEntry?.name === entry.name}
            <button
              class="row"
              aria-label={labelOf(entry)}
              class:active={selected === entry.name}
              class:kbd-active={isKbdActive}
              data-kbd-active={isKbdActive ? "true" : undefined}
              onclick={() => toggleSelect(entry.name)}
              ondblclick={() => commitFast(entry.name)}
              onmouseenter={(event) => showHover(event, entry)}
              onmousemove={(event) => showHover(event, entry)}
              onmouseleave={leaveHover}
            >
              <Icon type={entry.icon_type} name={entry.name} size={28} suppressTitle />
              <span class="row-name"><RichText text={entry.localized_name || entry.name} /></span>
              {#if entry.tile}
                <span class="tile-use">
                  {#if entry.tile.fluid}<Icon type="fluid" name={entry.tile.fluid} size={20} suppressTitle /><span>→ {runtime.localizedName("fluid",entry.tile.fluid)}</span>{/if}
                  {#if entry.tile.destroys_dropped_items}<span>可销毁物品</span>{/if}
                </span>
              {/if}
              {#if entry.localized_name}<span class="row-sub">{entry.name}</span>{/if}
              {#if entry.subgroup && !entry.localized_name}
                <span class="row-sub"><RichText text={runtime.richName("item-subgroup", entry.subgroup)} /></span>
              {/if}
            </button>
          {/each}
        </div>
      {:else}
        {#each bySubgroup as section (section.subgroup)}
          <div class="subgroup">
            <div class="sg-label"><RichText text={runtime.richName("item-subgroup", section.subgroup)} /></div>
            <div class="sg-items">
              {#each section.items as entry (entry.kind + "/" + entry.name)}
                {@const isKbdActive = highlightedEntry?.kind === entry.kind && highlightedEntry?.name === entry.name}
                <button
                  class="icon-btn"
                  class:active={selected === entry.name}
                  class:kbd-active={isKbdActive}
                  data-kbd-active={isKbdActive ? "true" : undefined}
                  aria-label={labelOf(entry)}
                  onclick={() => toggleSelect(entry.name)}
                  ondblclick={() => commitFast(entry.name)}
                  onmouseenter={(event) => showHover(event, entry)}
                  onmousemove={(event) => showHover(event, entry)}
                  onmouseleave={leaveHover}
                >
                  <Icon type={entry.icon_type} name={entry.name} size={34} suppressTitle />
                </button>
              {/each}
            </div>
          </div>
        {/each}
      {/if}
    </div>

    {#if hasQuality}
      <div class="modal-foot">
        <span class="foot-hint">
          {selectedEntry
            ? `已选：${labelOf(selectedEntry)}${qualityLabel ? `（${qualityLabel}）` : ""}`
            : "点击条目选中；物品+品质选齐即确认；E 键确认"}
        </span>
        <div class="foot-actions">
          <button class="btn ghost" onclick={onClose}>取消</button>
          <button
            class="btn primary"
            title="按 E 或 Enter 确认"
            onclick={confirm}
            disabled={!selectedEntry}
          >
            确定{selectedEntry
              ? ` · ${labelOf(selectedEntry)}${qualityLabel ? `（${qualityLabel}）` : ""}`
              : ""}
          </button>
        </div>
      </div>
    {/if}
  </div>
</div>

{#if hover}
  <HoverCard
    kind={hover.kind}
    detail={hoverDetail}
    x={hoverPos.x}
    y={hoverPos.y}
    {quality}
    hint={hasQuality ? "点击选中 · 双击或 Enter 确认 · Alt+1~5 切换品质" : "点击选择 · Enter/E 确认"}
    onEnter={keepHover}
    onLeave={leaveHoverCard}
  />
{/if}

<style>
  .tile-use { display: inline-flex; align-items: center; gap: 5px; font-size: 11px; color: var(--muted); }

  .filter-hint { font-size: 11px; color: var(--muted); padding-bottom: 8px; }

  .backdrop {
    position: fixed;
    z-index: 40;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 24px;
    background: var(--backdrop);
    backdrop-filter: blur(6px);
  }

  .modal {
    width: min(700px, 100%);
    max-height: min(760px, calc(100vh - 48px));
    display: flex;
    flex-direction: column;
    padding: 16px;
    background: var(--surface-glass, var(--panel));
    backdrop-filter: blur(12px);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
  }

  .modal-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
    padding-bottom: 11px;
    border-bottom: 1px solid var(--line);
  }

  .modal-title {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    overflow: hidden;
    font-size: 14px;
    font-weight: 700;
    letter-spacing: 0.02em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .spinner {
    width: 10px;
    height: 10px;
    border: 1px solid var(--line-strong);
    border-top-color: var(--text);
    border-radius: 50%;
    animation: spin 700ms linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .kind-tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    padding: 3px;
    margin-bottom: 8px;
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .tab {
    min-height: 27px;
    padding: 0 9px;
    color: var(--muted);
    background: transparent;
    border: 1px solid transparent;
    border-radius: 3px;
    font-size: 10.5px;
    cursor: pointer;
    transition: color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }

  .tab:hover {
    color: var(--text);
    background: var(--card-hover);
  }

  .tab.active {
    color: var(--accent-ink);
    background: var(--accent);
    font-weight: 700;
  }

  .search {
    width: 100%;
    min-height: 34px;
    padding: 0 10px;
    margin-bottom: 9px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-size: 12px;
    transition: border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      box-shadow var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }

  .search::placeholder {
    color: var(--faint);
  }

  .search:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }

  .quality-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 9px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--line);
  }

  .q-label {
    margin-right: 4px;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .q-chip {
    min-height: 28px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 1px 8px 1px 2px;
    color: var(--muted);
    background: transparent;
    border: 1px solid transparent;
    border-radius: 999px;
    cursor: pointer;
    transition: color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }

  .q-chip:hover {
    color: var(--text);
    background: var(--card-hover);
    border-color: var(--line);
  }

  .q-chip.active {
    color: var(--text);
    background: var(--accent-dim);
    border-color: var(--accent-line);
  }

  .q-name {
    font-size: 10.5px;
  }

  .group-tabs {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
    padding: 6px 0 8px;
    margin-bottom: 4px;
    border-bottom: 1px solid var(--line);
  }

  .gtab {
    width: 74px;
    height: 74px;
    display: grid;
    place-items: center;
    padding: 3px;
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      box-shadow var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }

  .gtab:hover {
    border-color: var(--line-strong);
    background: var(--card-hover);
  }

  .gtab.active {
    background: var(--accent-dim);
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .body {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    padding-right: 2px;
  }

  .subgroup {
    padding: 8px 0 10px;
  }

  .sg-label {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--line);
    color: var(--faint);
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .sg-items {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(42px, 1fr));
    gap: 5px;
  }

  .sg-items .icon-btn {
    width: 100%;
    height: 46px;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: var(--input-bg);
    transition: border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      box-shadow var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }

  .sg-items .icon-btn:hover {
    border-color: var(--line-strong);
    background: var(--card-hover);
  }

  .sg-items .icon-btn.active {
    background: var(--accent-dim);
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent), 0 0 0 1px var(--accent);
  }

  .sg-items .icon-btn.kbd-active {
    border-color: var(--focus);
    background: var(--card-hover);
    box-shadow: 0 0 0 1.5px var(--focus);
  }

  .sg-items .icon-btn.active.kbd-active {
    box-shadow: inset 0 0 0 1px var(--accent), 0 0 0 2px var(--focus);
  }

  .search-list {
    display: grid;
    gap: 2px;
  }

  .row {
    width: 100%;
    min-height: 40px;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 4px 7px;
    color: var(--text);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    text-align: left;
    cursor: pointer;
    transition: background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }

  .row:hover {
    background: var(--card-hover);
    border-color: var(--line);
  }

  .row.active {
    background: var(--accent-dim);
    border-color: var(--accent-line);
  }

  .row.kbd-active {
    background: var(--card-hover);
    border-color: var(--focus);
    box-shadow: inset 2px 0 0 var(--focus);
  }

  .row-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-size: 11.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-sub {
    flex: 0 1 auto;
    overflow: hidden;
    color: var(--faint);
    font-family: var(--mono);
    font-size: 9px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty {
    padding: 36px 12px;
    color: var(--faint);
    font-size: 11.5px;
    text-align: center;
  }

  .direct-panel,
  .custom-panel {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px;
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .direct-copy {
    flex: 1;
    display: grid;
    gap: 2px;
  }


  .direct-copy small {
    color: var(--muted);
    font-size: 10.5px;
  }

  .custom-panel {
    display: grid;
    grid-template-columns: 120px 1fr auto;
    gap: 6px;
  }

  .custom-panel select,
  .custom-panel input {
    min-height: 32px;
    padding: 0 8px;
    color: var(--text);
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    transition: border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }

  .custom-panel select:focus,
  .custom-panel input:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }

  .modal-foot {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: 12px;
    padding: 10px 4px 0;
    border-top: 1px solid var(--line);
  }

  .foot-hint {
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: 10.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .foot-actions {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  @media (max-width: 640px) {
    .custom-panel {
      grid-template-columns: 1fr;
    }
    .foot-hint {
      display: none;
    }
  }

</style>

<script lang="ts">
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import {
    accessibleKind,
    accessibleName,
    type Accessible,
    type CatalogKind,
    type LpSolver,
    type ProjectDocument,
    type TimeScale,
  } from "$lib/runtime/types";
  import { flowName } from "$lib/display-labels";
  import HoverIcon from "$lib/ui/HoverIcon.svelte";
  import RichText from "$lib/ui/RichText.svelte";
  import ProjectEstimates from "$lib/ui/ProjectEstimates.svelte";
  import type { OpenSelectorFn } from "$lib/ui/PlanningPreferencesModal.svelte";

  let {
    project,
    onOpenSelector,
    onOpenPrefs,
  }: {
    project: ProjectDocument | null;
    onOpenSelector: OpenSelectorFn;
    onOpenPrefs: () => void;
  } = $props();

  let planning = $derived(project?.planning ?? null);

  // 里程碑 / 显式标记可达 / 不可达统一的分类页签（与可达性对象一一对应；
  // 目录无 space-location 条目，故不含太空地点）。
  const markKinds: { kind: CatalogKind; label: string }[] = [
    { kind: "item", label: "物品" },
    { kind: "recipe", label: "配方" },
    { kind: "entity", label: "实体" },
    { kind: "technology", label: "科技" },
    { kind: "fluid", label: "流体" },
    { kind: "quality", label: "品质" },
    { kind: "planet", label: "星球" },
  ];

  function makeAccessible(kind: CatalogKind, name: string): Accessible {
    switch (kind) {
      case "technology":
        return { Tech: name };
      case "recipe":
        return { Recipe: name };
      case "quality":
        return { Quality: name };
      case "fluid":
        return { Fluid: name };
      case "planet":
        return { Planet: name };
      case "entity":
        return { Entity: name };
      default:
        return { Item: name };
    }
  }

  /** 科技等级信息（catalogIndex）：base = 名字后缀最低等级，max = 有效上限
   *  （null = 无限）。未知科技返回 null。O(1) Map 查找，避免线性扫描 entries。 */
  const techInfoMap = $derived.by(() => {
    const map = new Map<string, { base: number; max: number | null }>();
    for (const candidate of runtime.catalogIndex?.entries ?? []) {
      if (candidate.kind === "technology" && !map.has(candidate.name)) {
        map.set(candidate.name, {
          base: candidate.technology_base_level,
          max: candidate.technology_max_level,
        });
      }
    }
    return map;
  });

  function techInfo(tech: string): { base: number; max: number | null } | null {
    return techInfoMap.get(tech) ?? null;
  }
</script>

{#if project}
  <section class="panel">
    <div class="section-head">
      <span>显示与精度</span>
      <span class="count">对所有工厂生效</span>
    </div>
    <div class="field">
      <label for="time-scale">时间刻度</label>
      <select
        id="time-scale"
        value={project.settings.time_scale}
        onchange={(event) =>
          runtime
            .setTimeScale((event.currentTarget as HTMLSelectElement).value as TimeScale)
            .catch(() => {})}
      >
        <option value="seconds">秒</option>
        <option value="minutes">分钟</option>
        <option value="hours">小时</option>
      </select>
    </div>
    <div class="field">
      <label for="major-quality">主品质（项目级）</label>
      <button
        id="major-quality"
        class="btn block"
        onclick={() =>
          onOpenSelector({
            kind: "quality",
            title: "选择主品质",
            onSelect: (name) => runtime.setProjectMajorQuality(name),
          })}
      >
        {runtime.localizedName("quality", project?.settings.major_quality || "normal")}
      </button>
      <span class="muted">所有普通工厂与宇宙工厂共享。</span>
    </div>
    <div class="field">
      <label for="lp-solver">线性规划后端</label>
      <select
        id="lp-solver"
        value={project.settings.solver}
        onchange={(event) =>
          runtime
            .setSolver((event.currentTarget as HTMLSelectElement).value as LpSolver)
            .catch(() => {})}
      >
        <option value="highs">HiGHS（推荐）</option>
        <option value="soplex">SoPlex</option>
        <option value="clarabel">Clarabel</option>
        <option value="microlp">microlp</option>
        <option value="rust-certified">Rust based</option>
      </select>
      <span class="muted">项目级设置；切换后重新求解全部工厂。</span>
    </div>
    <div class="field">
      <label for="quality-limit">品质上限（超出会自动提升）</label>
      <select
        id="quality-limit"
        value={project.settings.quality_limit ?? ""}
        onchange={(event) => {
          const value = (event.currentTarget as HTMLSelectElement).value;
          runtime.setQualityLimit(value === "" ? null : value).catch(() => {});
        }}
      >
        <option value="">{runtime.localizedName("quality", "normal")}（当前上限）</option>
        {#each runtime.catalogIndex?.qualities ?? [] as q (q)}
          <option value={q}>{runtime.localizedName("quality", q)}</option>
        {/each}
      </select>
    </div>
    <label class="check" title="忽略里程碑与科技依赖，所有原型都可用于计算">
      <input
        type="checkbox"
        checked={project.settings.all_accessible}
        onchange={(event) =>
          runtime.setAllAccessible((event.currentTarget as HTMLInputElement).checked).catch(() => {})}
      />
      无视可达性（全部可用）
    </label>
  </section>

  <section class="panel">
    <div class="section-head">
      <span>里程碑</span>
      <span class="count">{runtime.orderedMilestones?.length ?? project.settings.milestones.length}</span>
    </div>
    <div class="field">
      <span class="field-label">取消勾选 = 禁用，含依赖内容</span>
      <div class="prefs-list">
        {#each runtime.orderedMilestones ?? project.settings.milestones as milestone, i (i)}
          <div class="prefs-item">
            <HoverIcon
              type={accessibleKind(milestone.node)}
              name={accessibleName(milestone.node)}
              size={22}
              detailKind={accessibleKind(milestone.node)}
            />
            <span class="prefs-name">
              <RichText
                text={typeof milestone.node === "string"
                  ? flowName(milestone.node, (kind, name) => runtime.richName(kind, name))
                  : runtime.richName(accessibleKind(milestone.node), accessibleName(milestone.node))}
              />
            </span>
            <label class="check" title={milestone.unlocked ? "点击锁定该里程碑及其依赖" : "点击解锁该里程碑"}>
              <input
                type="checkbox"
                checked={milestone.unlocked}
                onchange={(event) =>
                  runtime
                    .setMilestoneUnlocked(
                      milestone.node,
                      (event.currentTarget as HTMLInputElement).checked,
                    )
                    .catch(() => {})}
              />
            </label>
            <button
              class="icon-btn danger"
              title="移除里程碑"
              aria-label="移除里程碑"
              onclick={() => runtime.removeMilestone(milestone.node).catch(() => {})}
            >×</button>
          </div>
        {:else}
          <span class="muted">还没有里程碑</span>
        {/each}
      </div>
    </div>
    <div class="button-row two">
      <button
        class="btn"
        title="选择一个物品、科技或配方作为里程碑"
        onclick={() =>
          onOpenSelector({
            kind: "item",
            title: "选择里程碑",
            kinds: markKinds,
            onSelect: () => {},
            onPickKind: (kind, name) => runtime.addMilestone(makeAccessible(kind, name)),
            respectAccessibility: false,
          })}
      >+ 添加里程碑</button>
      <button
        class="btn"
        title="用当前上下文中的默认里程碑替换列表"
        onclick={() => runtime.setDefaultMilestones().catch(() => {})}
      >设置默认里程碑</button>
    </div>
  </section>

  <section class="panel">
    <div class="section-head">
      <span>产能</span>
      <span class="count">配方 / 采矿 / 无限科技</span>
    </div>
    <label class="check" title="丢弃自动推算的产能加成，只保留用户手动指定的值">
      <input
        type="checkbox"
        checked={project.settings.ignore_productivity}
        onchange={(event) =>
          runtime.setIgnoreProductivity((event.currentTarget as HTMLInputElement).checked).catch(() => {})}
      />
      忽略产能加成
    </label>

    <div class="field">
      <span class="field-label">采矿产能加成</span>
      <div class="prefs-list">
        <div class="prefs-item {project.settings.mining_productivity !== 0 ? "user-specified" : "inferred"}">
          <span class="prefs-name">采矿</span>
          <input
            class="prod-input"
            type="number"
            step="0.1"
            min="0"
            title="采矿产出加成百分比；0 表示使用自动推断值"
            aria-label="采矿产能加成百分比"
            value={String(100 * (
              project.settings.mining_productivity !== 0
                ? (project.settings.mining_productivity ?? NaN)
                : (runtime.productivityInfo?.auto_mining ?? 0)
            ))}
            onchange={(event) => {
              const value = Number((event.currentTarget as HTMLInputElement).value);
              if (Number.isFinite(value)) runtime.setMiningProductivity(value / 100).catch(() => {});
            }}
          />
          <span class="muted badge">{project.settings.mining_productivity !== 0 ? "用户" : "推断"}</span>
        </div>
      </div>
    </div>

    <div class="field">
      <span class="field-label">配方产能加成（%）</span>
      <div class="prefs-list">
        {#each runtime.productivityInfo?.recipes ?? [] as entry (entry.recipe)}
          <div class="prefs-item {entry.source === "user" ? "user-specified" : "inferred"}">
            <HoverIcon type="recipe" name={entry.recipe} size={22} detailKind="recipe" />
            <span class="prefs-name"><RichText text={runtime.richName("recipe", entry.recipe)} /></span>
            <input
              class="prod-input"
              type="number"
              min="0"
              title={`配方产能加成：${runtime.localizedName("recipe", entry.recipe)}（%）`}
              aria-label={`配方产能加成：${runtime.localizedName("recipe", entry.recipe)}`}
              value={String((entry.value ?? NaN) * 100)}
              onchange={(event) => {
                const value = Number((event.currentTarget as HTMLInputElement).value);
                if (Number.isFinite(value)) {
                  runtime.setRecipeProductivity(entry.recipe, value / 100).catch(() => {});
                }
              }}
            />
            <span class="muted">%</span>
            {#if entry.source === "user"}
              <button
                class="icon-btn danger"
                title="移除该配方产能加成"
                aria-label="移除配方产能加成"
                onclick={() => runtime.removeRecipeProductivity(entry.recipe).catch(() => {})}
              >×</button>
            {:else}
              <span class="muted badge">推断</span>
            {/if}
          </div>
        {:else}
          <span class="muted">还没有配方产能加成</span>
        {/each}
      </div>
      <button
        class="btn block"
        title="选择配方并添加产能加成"
        onclick={() =>
          onOpenSelector({
            kind: "recipe",
            title: "选择配方",
            onSelect: (name) => runtime.setRecipeProductivity(name, 0.5),
          })}
      >+ 添加配方产能</button>
    </div>

    <div class="field">
      <span class="field-label">无限科技研究次数</span>
      <div class="prefs-list">
        {#each project.settings.infinite_levels as entry (entry.tech)}
          <div class="prefs-item user-specified">
            <HoverIcon type="technology" name={entry.tech} size={22} detailKind="technology" />
            <span class="prefs-name"><RichText text={runtime.richName("technology", entry.tech)} /></span>
            <input
              class="prod-input"
              type="number"
              step="1"
              min={techInfo(entry.tech)?.base ?? 0}
              title={`研究次数：${runtime.localizedName("technology", entry.tech)}`}
              aria-label={`研究次数：${runtime.localizedName("technology", entry.tech)}`}
              value={String(entry.level)}
              onchange={(event) => {
                const raw = (event.currentTarget as HTMLInputElement).value;
                let value = Number(raw);
                // 钳制到科技实际范围 [base, max]（无限无上限）：
                // 低于最低等级或无上限时按 base/自身取值。
                const info = techInfo(entry.tech);
                const base = info?.base ?? 0;
                const max = info?.max ?? null;
                if (max != null) value = Math.min(value, max);
                value = Math.max(value, base);
                if (Number.isInteger(value)) {
                  runtime.setInfiniteTechLevel(entry.tech, value).catch(() => {});
                }
              }}
            />
            <span class="muted">级</span>
            <button
              class="icon-btn danger"
              title="移除无限科技研究次数"
              aria-label="移除无限科技研究次数"
              onclick={() => runtime.removeInfiniteTechLevel(entry.tech).catch(() => {})}
            >×</button>
          </div>
        {:else}
          <span class="muted">还没有无限科技等级</span>
        {/each}
      </div>
      <button
        class="btn block"
        title="选择可多次研究的科技并设置研究次数"
        onclick={() =>
          onOpenSelector({
            kind: "technology",
            title: "选择无限科技",
            categoryFilter: [],
            respectAccessibility: true,
            multiLevelTechOnly: true,
            onSelect: (name) => {
              const info = techInfo(name);
              runtime.setInfiniteTechLevel(name, (info?.base ?? 0) + 1);
            },
          })}
      >+ 添加无限科技</button>
    </div>
  </section>

  <section class="panel">
    <div class="section-head">
      <span>规划与插件</span>
      <span class="count">自动规划</span>
    </div>
    <div class="planning-summary">
      <div class="planning-stat" title="自动规划时优先选择的机器">
        <span>机器偏好</span>
        <strong>{planning?.machine_preferences.length ?? 0}</strong>
      </div>
      <div class="planning-stat" title="自动规划参与枚举的插件">
        <span>枚举插件</span>
        <strong>{planning?.enumerate_modules.length ?? 0}</strong>
      </div>
      <div class="planning-stat" title="自动规划叠加的插件塔方案">
        <span>枚举插件塔</span>
        <strong>{planning?.enumerate_beacons.length ?? 0}</strong>
      </div>
    </div>
    <p class="planning-note">
      每配方枚举 {planning?.alternative_count ?? 1} 个备选机器；{planning?.preserve_existing_mechanics ? "保留已有机制" : "不保留已有机制"}。机器偏好、枚举插件与插件塔组合在二级弹窗中编辑。
    </p>
    <button class="btn block" title="编辑机器偏好、替代数量、枚举插件与插件塔" onclick={onOpenPrefs}>编辑规划偏好…</button>
  </section>
  {#key project.id}
    <ProjectEstimates {project} />
  {/key}
{:else}
  <div class="rail-empty">
    <strong>还没有选中的项目</strong>
    <span>在左上角「项目」菜单中新建或打开项目。</span>
  </div>
{/if}

<style>
  .panel {
    padding: 14px 16px;
    background: transparent;
    border: 0;
    border-bottom: 1px solid var(--line);
    border-radius: 0;
    box-shadow: none;
  }

  .panel > .section-head:first-child {
    margin-top: 0;
  }

  .panel > .field + .field {
    margin-top: 10px;
  }

  /* 配方/无限科技等“字段 + 整行动作按钮”的垂直间距。 */
  .field > .btn.block {
    margin-top: 7px;
  }

  /* 并排动作行：两枚按钮等宽居中；单独动作行由 .btn.block 撑满。 */
  .button-row {
    display: grid;
    gap: 6px;
    margin-top: 10px;
  }

  .button-row.two {
    grid-template-columns: 1fr 1fr;
  }

  .button-row .btn {
    width: 100%;
  }

  .planning-summary {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
    margin: 9px 0 8px;
  }

  .planning-stat {
    min-width: 0;
    display: grid;
    gap: 2px;
    padding: 7px 8px;
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .planning-stat span {
    overflow: hidden;
    color: var(--faint);
    font-size: 9px;
    letter-spacing: 0.04em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .planning-stat strong {
    color: var(--text);
    font-family: var(--mono);
    font-size: 16px;
    font-weight: 600;
    line-height: 1.2;
  }

  .planning-note {
    margin: 0 0 8px;
    color: var(--muted);
    font-size: 10px;
    line-height: 1.5;
  }

  .rail-empty {
    display: grid;
    gap: 8px;
    margin: 16px;
    padding: 18px 16px;
    color: var(--muted);
    border: 1px dashed var(--line-strong);
    border-radius: var(--radius);
    font-size: 11.5px;
    line-height: 1.55;
  }

  .rail-empty strong {
    color: var(--text);
    font-size: 12px;
  }

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin: 14px 0 6px;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.11em;
    text-transform: uppercase;
  }

  .section-head .count {
    font-family: var(--mono);
    font-weight: 500;
    letter-spacing: 0;
  }

  .prefs-list {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .prefs-item {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 4px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }

  .prefs-item:hover {
    background: var(--card-hover);
    border-color: var(--line);
  }

  .prefs-item .check {
    margin: 0;
  }

  .prefs-name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    font-size: 11.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prefs-item.inferred {
    border-style: dashed;
    border-color: var(--accent-line);
  }

  .prefs-item.user-specified {
    border-style: solid;
    border-color: var(--accent-line);
    background: color-mix(in srgb, var(--input-bg) 94%, var(--accent) 4%);
  }

  .prod-input {
    width: 58px;
    min-height: 24px;
    padding: 0 5px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10px;
    text-align: right;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }

  .prod-input:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }

  .badge {
    flex: 0 0 auto;
    max-width: 54px;
    padding: 1px 5px;
    overflow: hidden;
    color: var(--faint);
    border: 1px solid var(--line);
    border-radius: 3px;
    font-size: 9px;
    letter-spacing: 0.04em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>

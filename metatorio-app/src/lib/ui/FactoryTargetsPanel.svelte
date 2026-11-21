<script module lang="ts">
  type Token =
    | { type: "num"; value: number }
    | { type: "op"; op: "+" | "-" | "*" | "/" | "(" | ")" };

  const SUFFIX_MULTIPLIERS: Record<string, number> = {
    p: 1e-12,
    P: 1e15,
    "μ": 1e-6,
    u: 1e-6,
    k: 1e3,
    K: 1e3,
    m: 1e6,
    M: 1e6,
    g: 1e9,
    G: 1e9,
    t: 1e12,
    T: 1e12,
    E: 1e18,
    z: 1e21,
    Z: 1e21,
    y: 1e24,
    Y: 1e24,
    r: 1e27,
    R: 1e27,
    q: 1e30,
    Q: 1e30,
  };

  /** 安全四则算式解析器（禁止 eval / new Function），支持 + - * /、括号与 k/M 等单位后缀。 */
  export function parseTargetExpression(raw: string, allowWatt = false): number | null {
    const text = raw.trim();
    if (!text) return null;

    const tokens: Token[] = [];
    let pos = 0;

    while (pos < text.length) {
      const ch = text[pos];
      if (/\s/.test(ch)) {
        pos += 1;
        continue;
      }
      if (ch === "+" || ch === "-" || ch === "*" || ch === "/" || ch === "(" || ch === ")") {
        tokens.push({ type: "op", op: ch });
        pos += 1;
        continue;
      }

      const slice = text.slice(pos);
      const numMatch = /^(\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?/.exec(slice);
      if (!numMatch) return null;

      const baseValue = Number(numMatch[0]);
      if (!Number.isFinite(baseValue)) return null;
      pos += numMatch[0].length;

      while (pos < text.length && /\s/.test(text[pos])) pos += 1;

      let multiplier = 1;
      if (pos < text.length && text[pos] in SUFFIX_MULTIPLIERS) {
        multiplier = SUFFIX_MULTIPLIERS[text[pos]];
        pos += 1;
        while (pos < text.length && /\s/.test(text[pos])) pos += 1;
      }

      if (allowWatt && pos < text.length && (text[pos] === "W" || text[pos] === "w")) {
        pos += 1;
      }

      if (pos < text.length && /[a-zA-Z_]/.test(text[pos])) return null;

      const scaled = baseValue * multiplier;
      if (!Number.isFinite(scaled)) return null;
      tokens.push({ type: "num", value: scaled });
    }

    if (tokens.length === 0) return null;
    let cursor = 0;

    function parseExpr(): number | null {
      let left = parseTerm();
      if (left === null) return null;
      while (cursor < tokens.length) {
        const tok = tokens[cursor];
        if (tok.type === "op" && (tok.op === "+" || tok.op === "-")) {
          cursor += 1;
          const right = parseTerm();
          if (right === null) return null;
          left = tok.op === "+" ? left + right : left - right;
          if (!Number.isFinite(left)) return null;
        } else {
          break;
        }
      }
      return left;
    }

    function parseTerm(): number | null {
      let left = parseUnary();
      if (left === null) return null;
      while (cursor < tokens.length) {
        const tok = tokens[cursor];
        if (tok.type === "op" && (tok.op === "*" || tok.op === "/")) {
          cursor += 1;
          const right = parseUnary();
          if (right === null) return null;
          if (tok.op === "/") {
            if (right === 0) return null;
            left = left / right;
          } else {
            left = left * right;
          }
          if (!Number.isFinite(left)) return null;
        } else {
          break;
        }
      }
      return left;
    }

    function parseUnary(): number | null {
      const tok = tokens[cursor];
      if (tok?.type === "op" && (tok.op === "+" || tok.op === "-")) {
        cursor += 1;
        const inner = parseUnary();
        if (inner === null) return null;
        return tok.op === "-" ? -inner : inner;
      }
      return parsePrimary();
    }

    function parsePrimary(): number | null {
      const tok = tokens[cursor];
      if (!tok) return null;
      if (tok.type === "num") {
        cursor += 1;
        return tok.value;
      }
      if (tok.type === "op" && tok.op === "(") {
        cursor += 1;
        const inner = parseExpr();
        if (inner === null) return null;
        const close = tokens[cursor];
        if (!close || close.type !== "op" || close.op !== ")") return null;
        cursor += 1;
        return inner;
      }
      return null;
    }

    const result = parseExpr();
    if (result === null || cursor !== tokens.length || !Number.isFinite(result)) return null;
    return result;
  }
</script>

<script lang="ts">
  import { onDestroy } from 'svelte';
  import { reorderIds } from '$lib/reorder';
  import { dndzone } from 'svelte-dnd-action';
  import { runtime } from '$lib/runtime/store.svelte';
  import type { FactoryAction, UniverseAction, FlowTarget, TargetExpression, DualVar, Location } from '$lib/runtime/types';
  import type { MechanicScope } from '$lib/runtime/mechanic-commands';
  import type { MechanicFlowSelection } from '$lib/mechanics/actions';
  import { networkRate } from "$lib/mechanics/network";
  import { isEnergyFlow } from '$lib/dual-var';
  import FlowRef from './FlowRef.svelte';
  import FlowRangeEditor from './FlowRangeEditor.svelte';
  import LocationPicker from './LocationPicker.svelte';
  import { flowSelectionInitial } from '$lib/mechanics/flow-selection';
  let {scope, targets, expressions, selectFlow, onSuggest}: {scope: MechanicScope; targets: FlowTarget[]; expressions: TargetExpression[];
    selectFlow: (request: MechanicFlowSelection & {initialName?:string;initialQuality?:string}) => void;
    onSuggest?: (flow: DualVar,location?:Location) => void} = $props();
  let targetsOpen=$state(true), expressionsOpen=$state(false), rows=$state<FlowTarget[]>([]);
  $effect(() => { rows=targets; });
  let exprRows=$state<TargetExpression[]>([]);
  $effect(()=>{exprRows=expressions.map(e=>({...e,terms:[...e.terms]}));});
  const timeScale=$derived(runtime.selectedProject?.settings.time_scale ?? 'seconds');
  const timeUnit=$derived(timeScale === 'minutes' ? '/min' : timeScale === 'hours' ? '/h' : '/s');
  const factor=$derived(timeScale === 'minutes' ? 60 : timeScale === 'hours' ? 3600 : 1);

  let invalidErrors = $state<Record<number, string>>({});
  const errorTimers = new Map<number, ReturnType<typeof setTimeout>>();
  onDestroy(() => {
    for (const timer of errorTimers.values()) clearTimeout(timer);
    errorTimers.clear();
  });
  const activeInvalidMessage = $derived(Object.values(invalidErrors)[0] ?? null);

  function formatTargetValue(target: FlowTarget): string {
    if (isEnergyFlow(target.flow)) {
      const amount = target.amount ?? NaN;
      const prefix = Number.isFinite(amount) && amount < 0 ? "-" : "";
      return `${prefix}${networkRate(target.flow, amount, "seconds")}`;
    }
    const value = (target.amount ?? NaN) * factor;
    return Number.isFinite(value) ? String(Number(value.toPrecision(12))) : "";
  }

  function clearTargetError(id: number) {
    const timer = errorTimers.get(id);
    if (timer) {
      clearTimeout(timer);
      errorTimers.delete(id);
    }
    if (id in invalidErrors) {
      const next = { ...invalidErrors };
      delete next[id];
      invalidErrors = next;
    }
  }

  function commitTargetAmount(target: FlowTarget, input: HTMLInputElement) {
    const energy = isEnergyFlow(target.flow);
    const parsed = parseTargetExpression(input.value, energy);
    const amount = parsed === null ? NaN : parsed / (energy ? 1 : factor);
    if (!Number.isFinite(amount)) {
      input.value = formatTargetValue(target);
      invalidErrors = { ...invalidErrors, [target.id]: "算式或数值无效，已恢复原值" };
      const prevTimer = errorTimers.get(target.id);
      if (prevTimer) clearTimeout(prevTimer);
      errorTimers.set(
        target.id,
        setTimeout(() => clearTargetError(target.id), 2600),
      );
      return;
    }
    clearTargetError(target.id);
    input.value = energy
      ? networkRate(target.flow, amount, "seconds")
      : String(Number((amount * factor).toPrecision(12)));
    if (amount === target.amount) return;
    writer()(
      { target: { 'set-amount': { target: target.id, amount } } },
      { 'set-target-amount': { target: target.id, amount } },
    );
  }
  function reader() {
    const destination = {...scope};
    return () => runtime.mechanicFactory(destination);
  }
  function writer() {
    const destination={...scope};
    return (normal: FactoryAction, universe: UniverseAction) => (destination.kind === 'universe'
      ? runtime.dispatchUniverse(destination.project,destination.factory,universe)
      : runtime.send({scope:'factory',action:{project:destination.project,factory:destination.factory,action:normal}})).catch(() => {});
  }
  function choose(title: string, current: DualVar | undefined, apply: (flow:DualVar) => void) {
    selectFlow({title,...flowSelectionInitial(current),onSelectFlow:apply});
  }
  function setTarget(target:FlowTarget, flow:DualVar, location=target.location) {
    return writer()({target:{'set-flow':{target:target.id,flow}}},{'set-target-flow':{target:target.id,flow:{location:location ?? runtime.universeAddLocation,flow}}});
  }
  function editTarget(target:FlowTarget) {
    const send=writer(), location=target.location ?? runtime.universeAddLocation;
    choose('更改目标流',target.flow,flow => send({target:{'set-flow':{target:target.id,flow}}},{'set-target-flow':{target:target.id,flow:{location,flow}}}));
  }
  function addTarget() {
    const send=writer(), location=runtime.universeAddLocation;
    choose('添加生产目标',undefined,flow => send({flow:{'add-to-target':{flow,amount:1}}},{'add-target':{flow:{location,flow},amount:1}}));
  }
  function editTerm(expression:number, term:TargetExpression['terms'][number], next?:DualVar, location=term.location) {
    const send=writer(), loc=location ?? runtime.universeAddLocation;
    const apply=(flow:DualVar) => send({'target-expression':{'set-term-flow':{expression,term:term.id,flow}}},{'set-target-expression-term-flow':{expression,term:term.id,flow:{location:loc,flow}}});
    if (next) apply(next); else choose('更改目标表达式的流',term.flow,apply);
  }
  function addTerm(expression:number) {
    const send=writer(), location=runtime.universeAddLocation;
    choose('添加目标表达式项',undefined,flow => send({'target-expression':{'add-term':{expression,term:{id:0,flow,coefficient:1}}}},{'add-target-expression-term':{expression,flow:{location,flow}}}));
  }
  async function reorder(event:CustomEvent<{items:FlowTarget[]}>) {
    rows=event.detail.items;
    const send=writer(),read=reader(),order=event.detail.items.map(t=>t.id);
    await reorderIds(order, () => read()?.targets ?? [],
      (target,position) => send({target:{reorder:{target,position}}},{'reorder-target':{target,position}}));
  }
</script>
<section class="panel targets-panel">
  <button class="title panel-toggle" aria-expanded={targetsOpen} onclick={() => targetsOpen=!targetsOpen}>
    <span class="toggle-label">
      <span class="chev">{targetsOpen?'▾':'▸'}</span>
      目标
      <span class="unit-badge" title={`目标数量按当前项目时间刻度计算（${timeUnit}）`}>{timeUnit}</span>
      <span class="count">{targets.length}</span>
    </span>
  </button>
  {#if targetsOpen}
    <div class="rows" use:dndzone={{items:rows,type:`targets-${scope.kind ?? 'factory'}-${scope.factory}`,flipDurationMs:120}} onconsider={e=>rows=e.detail.items} onfinalize={reorder}>
      {#each rows as target (target.id)}
        <div class="row-item one-line">
          {#if target.location}<LocationPicker value={target.location} label="目标地点" onChange={location=>setTarget(target,target.flow,location)} />{/if}
          <FlowRef flow={target.flow} title="左键更改流 · 右键查看建议" grow showQualifiers={false} showBadge={!(typeof target.flow==='object' && 'Fluid' in target.flow)} size={26} onClick={()=>editTarget(target)} onContextMenu={()=>onSuggest?.(target.flow,target.location)} />
          <FlowRangeEditor flow={target.flow} onChange={flow=>setTarget(target,flow)} />
          <span class="target-value">
          <input
            class="num"
            class:invalid={!!invalidErrors[target.id]}
            type="text"
            inputmode="decimal"
            aria-label="目标数量"
            aria-invalid={!!invalidErrors[target.id] || undefined}
            title={`支持四则算式与 k/M 后缀，如 45*4、(120+60)/2、1.5k（单位：${isEnergyFlow(target.flow) ? 'W' : timeUnit}）`}
            value={formatTargetValue(target)}
            oninput={() => clearTargetError(target.id)}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                e.currentTarget.blur();
              }
            }}
            onchange={(e) => commitTargetAmount(target, e.currentTarget)}
          />
          {#if !isEnergyFlow(target.flow)}<span class="rate-unit" aria-hidden="true">{timeUnit}</span>{/if}
          <button class="icon-btn danger" title="移除目标" onclick={()=>writer()({target:{remove:{target:target.id}}},{'remove-target':{target:target.id}})}>×</button>
          </span>
        </div>
      {/each}
    </div>
    {#if activeInvalidMessage}
      <div class="target-error-banner" role="status">{activeInvalidMessage}</div>
    {/if}
    <button class="btn block" onclick={addTarget}>+ 添加目标</button>
  {/if}
</section>
<section class="panel expression-panel">
  <button class="title panel-toggle" aria-expanded={expressionsOpen} onclick={()=>expressionsOpen=!expressionsOpen}>
    <span class="toggle-label">
      <span class="chev">{expressionsOpen?'▾':'▸'}</span>
      目标表达式 <span class="count">{expressions.length}</span>
    </span>
  </button>
  {#if expressionsOpen}
    <div use:dndzone={{items:exprRows,type:`expressions-${scope.kind??'factory'}-${scope.factory}`,flipDurationMs:120}} onconsider={e=>exprRows=e.detail.items} onfinalize={async e=>{
      exprRows=e.detail.items;const send=writer(),read=reader(),order=exprRows.map(e=>e.id);
      await reorderIds(order, () => read()?.target_expressions ?? [],
        (expression,position) => send({'target-expression':{reorder:{expression,position}}},{'reorder-target-expression':{expression,position}}));
    }}>
    {#each exprRows as expression (expression.id)}
      <div class="expr-card">
        <div class="row-item expr-head"><label class="expr-const">常数 <input class="num" type="number" aria-label="目标表达式常数" value={expression.constant} onchange={e=>{const constant=Number(e.currentTarget.value);if(Number.isFinite(constant))writer()({'target-expression':{'set-constant':{expression:expression.id,constant}}},{'set-target-expression-constant':{expression:expression.id,constant}})}} /></label>
          <button class="icon-btn danger" title="移除表达式" onclick={()=>writer()({'target-expression':{remove:{expression:expression.id}}},{'remove-target-expression':{expression:expression.id}})}>×</button></div>
        <div use:dndzone={{items:expression.terms,type:`expression-terms-${expression.id}`,flipDurationMs:120}} onconsider={e=>expression.terms=e.detail.items} onfinalize={async e=>{
          expression.terms=e.detail.items; const send=writer(),read=reader(),id=expression.id,order=expression.terms.map(t=>t.id);
          await reorderIds(order, () => read()?.target_expressions.find(e=>e.id===id)?.terms ?? [],
            (term,position) => send({'target-expression':{'reorder-term':{expression:id,term,position}}},{'reorder-target-expression-term':{expression:id,term,position}}));
        }}>
        {#each expression.terms as term (term.id)}
          <div class="row-item expr-term">
            {#if term.location}<LocationPicker value={term.location} label="表达式项地点" onChange={location=>editTerm(expression.id,term,term.flow,location)} />{/if}
            <FlowRef flow={term.flow} grow size={20} onClick={()=>editTerm(expression.id,term)} />
            <FlowRangeEditor flow={term.flow} onChange={flow=>editTerm(expression.id,term,flow)} />
            <input class="num" type="number" aria-label="表达式系数" value={term.coefficient} onchange={e=>{const coefficient=Number(e.currentTarget.value);if(Number.isFinite(coefficient))writer()({'target-expression':{'set-term-coefficient':{expression:expression.id,term:term.id,coefficient}}},{'set-target-expression-term-coefficient':{expression:expression.id,term:term.id,coefficient}})}} />
            <button class="icon-btn danger" title="移除项" onclick={()=>writer()({'target-expression':{'remove-term':{expression:expression.id,term:term.id}}},{'remove-target-expression-term':{expression:expression.id,term:term.id}})}>×</button>
          </div>
        {/each}
        </div>
        <button class="btn block" onclick={()=>addTerm(expression.id)}>+ 添加项</button>
      </div>
    {/each}
    </div>
    {#if !exprRows.length}<div class="empty-hint">目标表达式 = 常数 + Σ(流 × 系数)</div>{/if}
    <button class="btn block" onclick={()=>writer()({'target-expression':{add:{expression:{id:0,constant:0,terms:[]}}}},'add-target-expression')}>+ 添加表达式</button>
  {/if}
</section>
<style>
  .panel {
    padding: 14px 16px;
    background: transparent;
    border: 0;
    border-bottom: 1px solid var(--line);
    border-radius: 0;
    box-shadow: none;
    flex: 0 0 auto;
  }
  .panel-toggle {
    width: 100%;
    display: block;
    margin: 0 0 10px;
    padding: 0;
    color: var(--text);
    background: none;
    border: 0;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .panel-toggle:hover .chev {
    color: var(--text);
  }
  .toggle-label {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
  }
  .chev {
    width: 9px;
    flex: 0 0 auto;
    color: var(--faint);
    font-size: 9px;
    transition: color var(--transition-fast);
  }
  .panel-toggle .count {
    margin-left: auto;
    color: var(--faint);
    font-family: var(--mono);
    font-size: 9px;
    font-weight: 500;
    letter-spacing: 0;
  }
  .unit-badge {
    padding: 1px 6px;
    color: var(--muted);
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: 999px;
    font-family: var(--mono);
    font-size: 9.5px;
    font-weight: 600;
    letter-spacing: 0;
  }
  .rate-unit {
    flex: 0 0 auto;
    color: var(--faint);
    font-family: var(--mono);
    font-size: 9.5px;
    user-select: none;
  }
  .target-value .icon-btn {
    opacity: 0.65;
  }
  .row-item:hover .target-value .icon-btn,
  .row-item:focus-within .target-value .icon-btn {
    opacity: 1;
  }
  .num.invalid,
  .num.invalid:focus {
    border-color: var(--danger);
    background: var(--danger-dim);
    box-shadow: 0 0 0 1px var(--danger);
  }
  .target-error-banner {
    margin: -4px 0 8px;
    padding: 4px 8px;
    color: var(--danger);
    background: var(--danger-dim);
    border: 1px solid var(--danger-line);
    border-radius: var(--radius-sm);
    font-size: 10.5px;
  }
  .rows {
    display: grid;
    gap: 1px;
    margin-bottom: 10px;
  }
  .row-item {
    min-width: 0;
    min-height: 32px;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 7px;
    padding: 3px 6px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .row-item:hover,
  .row-item:focus-within {
    background: var(--card-hover);
    border-color: var(--line);
  }
  .target-value {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
  }
  .num {
    width: 68px;
    min-width: 0;
    min-height: 26px;
    padding: 0 6px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10.5px;
    text-align: right;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .num:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }
  .expr-card {
    padding: 8px;
    margin-bottom: 8px;
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }
  .expr-head {
    justify-content: space-between;
    padding-bottom: 6px;
    margin-bottom: 4px;
    border-bottom: 1px solid var(--line);
  }
  .expr-const {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    font-size: 11px;
    font-weight: 550;
  }
  .expr-card > .btn.block,
  .panel > .btn.block {
    margin-top: 6px;
  }
  .empty-hint {
    padding: 12px 4px;
    color: var(--faint);
    font-size: 11px;
    line-height: 1.55;
  }
  @media (max-width: 1360px) {
    .row-item {
      gap: 5px;
    }
    .num {
      width: 60px;
    }
  }
</style>

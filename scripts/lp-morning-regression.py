"""Generate, execute and summarize isolated production auto-plan regression cases.

Run from the repository root. Each case has immutable input, context hash,
terminal record and a compressed archive of full LPs/exact witnesses. A harness
timeout is never an infeasibility result. The manifest can be resumed safely.
"""
import argparse
import collections
import copy
import hashlib
import json
import math
import os
import re
from pathlib import Path
import shutil
import subprocess
import time
import zipfile

ROOT = Path(__file__).resolve().parents[1]
QUALITIES = ['normal', 'uncommon', 'rare', 'epic', 'legendary']


def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def write(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + f'.{os.getpid()}.tmp')
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding='utf-8')
    temporary.replace(path)


def digest(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def make_manifest(run):
    run.mkdir(parents=True, exist_ok=False)
    sources = [ROOT / p for p in [
        'local/lengden.json', 'local/strict-mixed-pink-backup-20261021.json',
        'local/fulgora-strict-output-user-source-20261014.json',
        'crates/metatorio-shell/src/mcp/auto_plan_assets_tests/fixtures/legendary-biter-prod.json']]
    inventory = {}
    historical_sets = []
    for path in sources:
        for project in read(path)['projects']:
            plans = project.get('planning', {}).get('enumerate_beacons', [])
            historical_sets.append({'source': str(path), 'sha256': digest(path), 'project': project['name'], 'plans': plans})
            for plan in plans:
                key = json.dumps(plan, sort_keys=True)
                inventory.setdefault(key, {'plan': plan, 'sources': []})['sources'].append(str(path))
    write(run / 'beacon-history.json', {'projects': historical_sets, 'presets': list(inventory.values())})
    sparse = next(p['plan'] for p in inventory.values()
                  if p['plan']['module_config']['beacons'][0]['count'] == 4)
    shared = next(s['plans'] for s in historical_sets if len(s['plans']) == 2)
    normal_shared = copy.deepcopy(shared)
    for plan in normal_shared:
        for beacon in plan['module_config']['beacons']:
            beacon['beacon']['quality'] = 'normal'
            for module, _ in beacon['modules']:
                module['quality'] = 'normal'
    layouts = [('none', []), ('history-4-8-2', [sparse]),
               ('history-shared-legendary', shared), ('shared-normal-control', normal_shared)]
    contexts = {
        'vanilla-user-2.1.19': Path(os.environ['APPDATA']) / 'com.mirac.metatorio-app/contexts/0ac4e1f02224603e/data-raw-dump.json',
        'modded-repository': ROOT / 'assets/data-raw-dump-heavily-modded.json'}
    context_metadata = {name: {'path': str(path), 'sha256': digest(path), 'bytes': path.stat().st_size}
                        for name, path in contexts.items()}
    for name, source in list(contexts.items()):
        frozen = run / 'context-data' / name / 'data-raw-dump.json'
        frozen.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, frozen)
        assert digest(source) == digest(frozen)
        contexts[name] = frozen.resolve()
        context_metadata[name]['frozen_path'] = str(frozen.resolve())
    write(run / 'contexts.json', context_metadata)
    targets = [('nauvis', item) for item in [
        'iron-plate', 'copper-plate', 'steel-plate', 'iron-gear-wheel', 'electronic-circuit',
        'advanced-circuit', 'processing-unit', 'plastic-bar', 'low-density-structure',
        'rocket-fuel', 'biter-egg', 'productivity-module-3', 'speed-module-3', 'efficiency-module-3']]
    targets += [('vulcanus', item) for item in ['tungsten-plate', 'tungsten-carbide', 'metallurgic-science-pack', 'foundry']]
    targets += [('fulgora', item) for item in ['scrap', 'holmium-plate', 'electromagnetic-science-pack', 'quality-module-3']]
    targets += [('gleba', item) for item in ['bioflux', 'agricultural-science-pack', 'yumako-seed', 'artificial-yumako-soil']]
    targets += [('aquilo', item) for item in ['lithium-plate', 'cryogenic-science-pack']]
    aquatic = ['iron-plate', 'copper-plate', 'coal', 'stone', 'calcite', 'holmium-plate', 'processing-unit', 'low-density-structure']

    def item_source(item, price=1):
        return {'flow': {'Item': {'id': item, 'quality': 'normal'}}, 'penalty': price}

    cases = []

    def add(context, planet, item, quality, major, layout, strict, extra=None, tag='breadth'):
        layout_name, plans = layout
        inputs = []
        if context.startswith('vanilla'):
            if planet == 'nauvis' and item in ['biter-egg', 'productivity-module-3']:
                inputs.append(item_source('bioflux', 160))
            if planet == 'nauvis' and item == 'speed-module-3':
                inputs.append(item_source('tungsten-carbide'))
            if planet == 'nauvis' and item == 'efficiency-module-3':
                inputs.append(item_source('spoilage'))
            if planet == 'aquilo':
                inputs += [item_source(name) for name in aquatic]
                inputs += [{'flow': name, 'penalty': 1} for name in ['Electricity', 'Heat']]
                inputs += [{'flow': {'Fluid': {'name': name, 'temperature': [temperature, temperature]}}, 'penalty': 1}
                           for name, temperature in [('water', 15), ('crude-oil', 25)]]
        inputs += extra or []
        assert not any(x['flow'] == {'Item': {'id': item, 'quality': quality}} for x in inputs)
        factory = {'id': 2, 'name': tag, 'settings': {'planet': planet, 'major_quality': major},
                   'targets': [{'id': 3, 'flow': {'Item': {'id': item, 'quality': quality}}, 'amount': 1.0}],
                   'strict_source': True, 'strict_sink': strict,
                   'external_inputs': [dict(x, id=100 + i) for i, x in enumerate(inputs)]}
        project = {'id': 1, 'name': 'Morning regression', 'context_id': 'matrix',
                   'settings': {'quality_limit': 'legendary', 'ignore_productivity': True},
                   'planning': {'preserve_existing_mechanics': False, 'alternative_count': 3,
                                'enumerate_beacons': plans}, 'factories': [factory]}
        index = len(cases)
        case = {'id': f'{index:04d}-{context}-{planet}-{item}-{quality}-{layout_name}-{tag}',
                'context': context, 'context_dump': str(contexts[context]),
                'planet': planet, 'item': item, 'quality': quality, 'major_quality': major,
                'module_quality': major, 'layout': layout_name, 'strict_sink': strict,
                'best_modules': True, 'recheck': index % 40 == 0,
                'document': {'schema_version': 1, 'projects': [project]}}
        case['directory'] = str((run / case['id']).resolve())
        cases.append(case)

    # Breadth-first: every target and historical layout before advancing quality.
    for qi, quality in enumerate(QUALITIES):
        for li, layout in enumerate(layouts):
            for planet, item in targets:
                add('vanilla-user-2.1.19', planet, item, quality,
                    'normal' if qi % 2 == 0 else 'legendary', layout, li in [1, 2])
        for li in [1, 2]:
            for item in ['iron-plate', 'copper-plate', 'steel-plate', 'electronic-circuit',
                         'processing-unit', 'plastic-bar', 'engine-unit', 'iron-gear-wheel']:
                add('modded-repository', 'nauvis', item, quality,
                    'legendary' if qi % 2 == 0 else 'normal', layouts[li], li == 2)
        # Revisit the reported Fulgora import sensitivity at every quality and
        # both machine/module quality extremes; never import the target itself.
        for major in ['normal', 'legendary']:
            for imported in [False, True]:
                add('vanilla-user-2.1.19', 'fulgora', 'quality-module-3', quality, major, layouts[2], True,
                    [item_source('bioflux', 160)] if imported else [], 'bioflux-with' if imported else 'bioflux-without')
    for case in cases:
        write(run / 'cases' / (case['id'] + '.json'), case)
    write(run / 'manifest.json', {'generated_at': time.time(), 'layouts': layouts, 'cases': cases})
    print(json.dumps({'manifest': str(run / 'manifest.json'), 'cases': len(cases), 'historical_presets': len(inventory)}, ensure_ascii=False), flush=True)


def archive_capture(directory, run):
    capture = (directory / 'capture').resolve()
    if not capture.exists():
        return 0
    assert capture.is_relative_to(run.resolve()) and capture.name == 'capture'
    files = list(capture.rglob('*'))
    files = [p for p in files if p.is_file()]
    target = directory / 'capture.zip'
    temporary = directory / 'capture.zip.tmp'
    with zipfile.ZipFile(temporary, 'w', zipfile.ZIP_DEFLATED, compresslevel=1) as archive:
        for path in files:
            archive.write(path, path.relative_to(capture))
    with zipfile.ZipFile(temporary) as archive:
        assert len(archive.infolist()) == len(files)
    temporary.replace(target)
    # Only the verified, task-owned capture subtree is removed after archiving.
    shutil.rmtree(capture)
    return target.stat().st_size


def execute(run, executable, verifier, limit, case_timeout, worker='serial', qualities=None):
    manifest = read(run / 'manifest.json')
    done = 0
    cases = manifest['cases']
    if qualities:
        cases = [c for c in cases if c['quality'] in qualities]
        # Interleave qualities rather than postponing all legendary targets.
        cases.sort(key=lambda c: (int(c['id'].split('-')[0]) % 132, QUALITIES.index(c['quality'])))
    for case in cases:
        directory = Path(case['directory'])
        if (directory / 'outcome.json').exists():
            continue
        if done >= limit:
            break
        if worker == 'serial':
            directory.mkdir(parents=True, exist_ok=True)
            if (directory / 'configured.json').exists() or (directory / 'capture').exists():
                raise RuntimeError(f'unfinished case needs inspection, not an automatic restart: {directory}')
        else:
            # Publish the directory and a capture marker atomically. The legacy
            # serial worker also recognizes the marker and cannot duplicate it.
            staging = run / f'.claim-{worker}-{os.getpid()}-{case["id"].split("-")[0]}'
            assert staging.resolve().is_relative_to(run.resolve())
            assert directory.resolve().is_relative_to(run.resolve())
            staging.mkdir()
            (staging / 'capture').mkdir()
            claimed = False
            for attempt in range(6):
                try:
                    staging.rename(directory)
                    claimed = True
                    break
                except OSError:
                    if directory.exists():
                        break
                    if attempt == 5:
                        raise
                    # Windows scanners may briefly hold a newly created folder.
                    # Retry only the atomic claim, never the numerical test.
                    time.sleep(0.1 * (attempt + 1))
            if not claimed:
                (staging / 'capture').rmdir()
                staging.rmdir()
                continue  # Another worker owns it; never restart on a stale marker.
        env = os.environ.copy()
        for name in ['METATORIO_LP_BACKEND', 'METATORIO_LP_BALANCE_MODE', 'METATORIO_LP_RELEVANCE']:
            env.pop(name, None)
        env['LP_MATRIX_CASE'] = str(run / 'cases' / (case['id'] + '.json'))
        env['METATORIO_LP_CAPTURE_DIR'] = str(directory / 'capture')
        if worker != 'serial':
            env['RAYON_NUM_THREADS'] = '2'
        start = time.monotonic()
        started_at = time.time()
        timed_out = False
        with (directory / 'test.log').open('wb') as log:
            child = subprocess.Popen([str(executable), 'morning_matrix_case', '--ignored', '--nocapture'],
                                     cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT,
                                     creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
            active_file = run / ('active.json' if worker == 'serial' else f'worker-{worker}.active.json')
            write(active_file, {'case': case['id'], 'child_pid': child.pid, 'started_at': started_at,
                                        'runner_pid': os.getpid(), 'case_timeout': case_timeout})
            try:
                child.wait(timeout=case_timeout)
            except subprocess.TimeoutExpired:
                timed_out = True
                child.kill()
                child.wait()
        elapsed = time.monotonic() - start
        terminal = read(directory / 'result.json') if (directory / 'result.json').exists() else {}
        verified = (directory / 'verified.json').exists()
        proof = None
        status = 'timeout-unfinished' if timed_out else 'process-error'
        if not timed_out and child.returncode == 0 and terminal:
            try:
                check = subprocess.run([str(verifier), str(directory / 'capture'), '--allow-infeasible'],
                                       cwd=ROOT, capture_output=True, timeout=180,
                                       creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
                (directory / 'witness-check.log').write_bytes(check.stdout + check.stderr)
                if check.returncode == 0:
                    proof = json.loads(check.stdout)
            except subprocess.TimeoutExpired:
                pass
            if verified:
                status = 'solved-verified' if proof else 'witness-verification-failed'
            elif proof and proof.get('verified_infeasible_models', 0):
                status = 'proven-infeasible'
            else:
                status = 'not-solved-unproven'
        elif terminal and not timed_out:
            status = 'returned-but-test-failed'
        record = {'case_id': case['id'], 'status': status, 'process_seconds': elapsed,
                  'started_at': started_at, 'worker': worker,
                  'returncode': child.returncode, 'proof': proof, 'terminal': terminal,
                  'best_modules': True, 'quality': case['quality'], 'planet': case['planet'],
                  'item': case['item'], 'layout': case['layout'], 'context': case['context']}
        record['archive_bytes'] = archive_capture(directory, run)
        write(directory / 'outcome.json', record)
        with (run / 'outcomes.jsonl').open('a', encoding='utf-8') as output:
            output.write(json.dumps(record, ensure_ascii=False) + '\n')
            output.flush()
        active_file.unlink(missing_ok=True)
        done += 1
        print(json.dumps({'case': case['id'], 'status': status, 'seconds': round(elapsed, 3),
                          'completed_this_run': done}, ensure_ascii=False), flush=True)
    summarize(run)


def summarize(run):
    manifest = read(run / 'manifest.json')
    records = [read(p) for p in run.glob('*/outcome.json')]
    audits = [read(p) for p in run.glob('*/strict-output-audit.json')]
    metadata_path = run / 'run-metadata.json'
    metadata = read(metadata_path) if metadata_path.exists() else {}
    latest_verifier = metadata.get('complete_metrics_audit', {}).get('sha256')
    latest_audits = [a for a in audits if a['status'] == 'passed' and
                     a['verifier_sha256'] == latest_verifier]
    proofs = [a['proof'] for a in latest_audits]
    groups = {}
    for field in ['context', 'quality', 'layout', 'planet']:
        groups[field] = {
            value: dict(collections.Counter(r['status'] for r in records if r[field] == value))
            for value in sorted({r[field] for r in records})}
    successful_times = sorted(r['process_seconds'] for r in records
                              if r['status'] == 'solved-verified')

    def percentile(fraction):
        # Nearest rank; these describe successful cases only. Timeouts are
        # right-censored and must not be inserted as successful solve times.
        if not successful_times:
            return None
        rank = max(1, math.ceil(fraction * len(successful_times)))
        return successful_times[rank - 1]

    summary = {'planned': len(manifest['cases']), 'completed': len(records),
               'statuses': dict(collections.Counter(r['status'] for r in records)),
               'qualities': dict(collections.Counter(r['quality'] for r in records)),
               'contexts': dict(collections.Counter(r['context'] for r in records)),
               'layouts': dict(collections.Counter(r['layout'] for r in records)),
               'planets': dict(collections.Counter(r['planet'] for r in records)),
               'archive_bytes': sum(r['archive_bytes'] for r in records),
               'strict_audit_statuses': dict(collections.Counter(a['status'] for a in audits)),
               'strict_audit_verifier_counts': dict(collections.Counter(a['verifier_sha256'] for a in audits)),
               'latest_verifier_sha256': latest_verifier,
               'latest_verifier_passed': sum(a['status'] == 'passed' and
                                             a['verifier_sha256'] == latest_verifier for a in audits),
               'statuses_by': groups,
               'successful_process_seconds': {
                   'count': len(successful_times), 'median': percentile(0.5),
                   'p90': percentile(0.9), 'p95': percentile(0.95),
                   'maximum': max(successful_times, default=None),
                   'note': 'Successful cases only, under mixed concurrent load; timeouts are right-censored.'},
               'latest_audited_proofs': {
                   **{field: sum(p[field] for p in proofs) for field in
                      ['verified_models', 'verified_feasible_models', 'exact_optima',
                       'verified_relative_feasible_models']},
                   'maximum_relative_balance_error': max(
                       (p['maximum_relative_balance_error'] for p in proofs), default=None),
                   'maximum_normalized_primal_dual_cost_distance': max(
                       (p['maximum_relative_gap_upper_bound'] for p in proofs), default=None),
                   'note': 'Cost distance is a stopping comparison, not an optimum-error bound for inexact primal points.'},
               'slowest': sorted([{'case': r['case_id'], 'status': r['status'], 'seconds': r['process_seconds']}
                                  for r in records], key=lambda x: x['seconds'], reverse=True)[:30]}
    write(run / 'summary.json', summary)
    print(json.dumps(summary, ensure_ascii=False), flush=True)


def audit_inputs(run):
    """Audit the full requested matrix and the effective UseBestModules inputs."""
    manifest = read(run / 'manifest.json')
    cases = manifest['cases']
    assert len(cases) == 660
    assert len({c['id'] for c in cases}) == 660
    assert {p.stem for p in (run / 'cases').glob('*.json')} == {c['id'] for c in cases}
    contexts = read(run / 'contexts.json')
    for name, metadata in contexts.items():
        frozen = run / 'context-data' / name / 'data-raw-dump.json'
        assert digest(frozen) == metadata['sha256']
        assert frozen.stat().st_size == metadata['bytes']
    layouts = dict(manifest['layouts'])
    history = read(run / 'beacon-history.json')
    assert len(history['presets']) == 3 and layouts['none'] == []
    assert len(layouts['history-4-8-2']) == 1
    assert layouts['history-4-8-2'][0] in [p['plan'] for p in history['presets']]
    sparse_beacon = layouts['history-4-8-2'][0]['module_config']['beacons'][0]
    assert (sparse_beacon['count'], sparse_beacon['share'],
            sum(count for _, count in sparse_beacon['modules'])) == (4, 8, 2)
    assert layouts['history-shared-legendary'] in [p['plans'] for p in history['projects']]
    shared_beacons = [b for p in layouts['history-shared-legendary']
                      for b in p['module_config']['beacons']]
    assert {(b['count'], b['share'], sum(n for _, n in b['modules']))
            for b in shared_beacons} == {(1, 8, 1), (8, 8, 16)}
    normalized = copy.deepcopy(layouts['history-shared-legendary'])
    for plan in normalized:
        for beacon in plan['module_config']['beacons']:
            beacon['beacon']['quality'] = 'normal'
            for module, _ in beacon['modules']:
                module['quality'] = 'normal'
    assert layouts['shared-normal-control'] == normalized
    breadth = collections.defaultdict(set)
    controls = set()
    configured = 0
    pending = []
    module_signatures = collections.Counter()
    expected_modules = {
        'vanilla-user-2.1.19': {'efficiency-module-3', 'productivity-module-3',
                               'quality-module-3', 'speed-module-3'},
        'modded-repository': {'core-miner-module-3', 'efficiency-module-2',
                              'productivity-module-3', 'quality-module-3', 'speed-module-6'}}
    for original in cases:
        case = read(run / 'cases' / (original['id'] + '.json'))
        for key in original.keys() - {'context_dump'}:
            assert case[key] == original[key], (case['id'], key)
        assert Path(case['context_dump']).resolve() == (
            run / 'context-data' / case['context'] / 'data-raw-dump.json').resolve()
        assert Path(case['directory']).resolve() == (run / case['id']).resolve()
        project = case['document']['projects'][0]
        factory = project['factories'][0]
        target = {'id': 3, 'flow': {'Item': {'id': case['item'], 'quality': case['quality']}},
                  'amount': 1.0}
        assert factory['targets'] == [target]
        assert factory['settings']['planet'] == case['planet']
        assert factory['settings']['major_quality'] == case['major_quality']
        assert factory['strict_source'] and factory['strict_sink'] == case['strict_sink']
        assert case['best_modules'] and case['module_quality'] == case['major_quality']
        assert not any(i['flow'] == target['flow'] for i in factory['external_inputs'])
        assert project['planning']['enumerate_beacons'] == layouts[case['layout']]
        assert project['settings'] == {'quality_limit': 'legendary', 'ignore_productivity': True}
        if factory['name'] == 'breadth':
            key = (case['context'], case['planet'], case['item'])
            pair = (case['quality'], case['layout'])
            assert pair not in breadth[key]
            breadth[key].add(pair)
        else:
            assert factory['name'] in {'bioflux-with', 'bioflux-without'}
            assert (case['planet'], case['item']) == ('fulgora', 'quality-module-3')
            key = (case['quality'], case['major_quality'], factory['name'])
            assert key not in controls
            controls.add(key)
            imports = factory['external_inputs']
            if factory['name'] == 'bioflux-with':
                assert len(imports) == 1 and imports[0]['penalty'] == 160
                assert imports[0]['flow'] == {'Item': {'id': 'bioflux', 'quality': 'normal'}}
            else:
                assert imports == []
        path = run / case['id'] / 'configured.json'
        if not path.exists():
            pending.append(case['id'])
            continue
        try:
            effective = read(path)['project']
        except json.JSONDecodeError:
            # A live test may still be writing this file. Completion requires
            # another audit with zero pending cases, not accepting a partial file.
            pending.append(case['id'])
            continue
        modules = effective['planning']['enumerate_modules']
        assert {m['id'] for m in modules} == expected_modules[case['context']]
        assert all(m['quality'] == case['module_quality'] for m in modules)
        assert effective['planning']['enumerate_beacons'] == layouts[case['layout']]
        f = effective['factories'][0]
        assert f['settings'] == {'debug': False, 'surface': None, **factory['settings']}
        for key in ['targets', 'strict_source', 'strict_sink', 'external_inputs']:
            assert f[key] == factory[key], (case['id'], key)
        configured += 1
        module_signatures[f"{case['context']}|{case['module_quality']}"] += 1
    vanilla = {k: v for k, v in breadth.items() if k[0] == 'vanilla-user-2.1.19'}
    modded = {k: v for k, v in breadth.items() if k[0] == 'modded-repository'}
    assert len(vanilla) == 28 and len(modded) == 8 and len(controls) == 20
    for pairs in vanilla.values():
        assert pairs == {(q, layout) for q in QUALITIES for layout in layouts}
    for pairs in modded.values():
        assert pairs == {(q, layout) for q in QUALITIES
                         for layout in ['history-4-8-2', 'history-shared-legendary']}
    result = {'input_scope_verified': True, 'cases': len(cases), 'contexts_verified': len(contexts),
              'vanilla_targets': len(vanilla), 'modded_targets': len(modded),
              'bioflux_controls': len(controls), 'configured_audited': configured,
              'configured_pending': pending, 'module_signature_counts': dict(module_signatures),
              'audited_at': time.time()}
    write(run / 'input-audit.json', result)
    print(json.dumps({k: v for k, v in result.items() if k != 'configured_pending'}), flush=True)


def audit_outputs(run, verifier, follow):
    """Recheck older archives with the stricter pure-relative f64 verifier."""
    verifier_hash = digest(verifier)
    temporary_root = run / f'strict-audit-work-{os.getpid()}'
    temporary_root.mkdir(exist_ok=False)
    try:
        while True:
            tested = 0
            for outcome_path in sorted(run.glob('*/outcome.json')):
                directory = outcome_path.parent
                previous = directory / 'strict-output-audit.json'
                if previous.exists() and read(previous).get('verifier_sha256') == verifier_hash:
                    continue
                outcome = read(outcome_path)
                if outcome['status'] != 'solved-verified':
                    continue
                work = temporary_root / directory.name
                work.mkdir()
                with zipfile.ZipFile(directory / 'capture.zip') as source:
                    for name in source.namelist():
                        if not (('/' not in name and name.startswith('lp-') and name.endswith('.json'))
                                or name.startswith('rust-certificates/')):
                            continue
                        target = (work / name).resolve()
                        assert target.is_relative_to(work.resolve())
                        target.parent.mkdir(parents=True, exist_ok=True)
                        with source.open(name) as incoming, target.open('wb') as outgoing:
                            shutil.copyfileobj(incoming, outgoing)
                started = time.monotonic()
                record = {'case_id': outcome['case_id'], 'verifier_sha256': verifier_hash,
                          'archive_sha256': digest(directory / 'capture.zip')}
                try:
                    result = subprocess.run([str(verifier), str(work)], cwd=ROOT,
                                            capture_output=True, timeout=180,
                                            creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
                    (directory / 'strict-output-audit.log').write_bytes(result.stdout + result.stderr)
                    record['status'] = 'passed' if result.returncode == 0 else 'failed'
                    record['returncode'] = result.returncode
                    if result.returncode == 0:
                        record['proof'] = json.loads(result.stdout)
                except subprocess.TimeoutExpired:
                    record['status'] = 'audit-timeout'
                record['seconds'] = time.monotonic() - started
                write(directory / 'strict-output-audit.json', record)
                assert work.resolve().is_relative_to(temporary_root.resolve())
                shutil.rmtree(work)
                tested += 1
                print(json.dumps({'strict_audit': directory.name, 'status': record['status'],
                                  'seconds': round(record['seconds'], 2)}, ensure_ascii=False), flush=True)
            if not follow or (run / 'stop-strict-audit').exists():
                break
            if not tested:
                time.sleep(5)
    finally:
        if not any(temporary_root.iterdir()):
            temporary_root.rmdir()


def audit_completion(run):
    """Necessary completion gates; unresolved solver timeouts remain explicit."""
    audit_inputs(run)
    inputs = read(run / 'input-audit.json')
    assert inputs['configured_audited'] == 660 and not inputs['configured_pending']
    cases = read(run / 'manifest.json')['cases']
    outcomes = {p.parent.name: read(p) for p in run.glob('*/outcome.json')}
    assert set(outcomes) == {c['id'] for c in cases}, 'missing/extra terminal cases'
    metadata = read(run / 'run-metadata.json')
    verifier = metadata['complete_metrics_audit']
    assert digest(run / verifier['binary']) == verifier['sha256']
    assert digest(run / 'case-runner.exe') == metadata['case_runner_sha256']
    counts = collections.Counter()
    witnessed_models = 0
    cold_recomputes = 0
    unresolved = []
    for case in cases:
        directory = run / case['id']
        outcome = outcomes[case['id']]
        assert outcome['case_id'] == case['id']
        status = outcome['status']
        assert status in {'solved-verified', 'timeout-unfinished', 'proven-infeasible'}, (
            case['id'], 'requires diagnosis before completion', status)
        log = (directory / 'test.log').read_text(encoding='utf-8', errors='replace')
        assert '[MATRIX-START]' in log
        archive = directory / 'capture.zip'
        if outcome['archive_bytes'] == 0:
            # Serial cases may time out during enumeration/LP construction,
            # before the solver has created a capture directory at all.
            assert status == 'timeout-unfinished' and not archive.exists()
            assert '[LP-CAPTURE]' not in log
            assert not any((directory / 'capture').rglob('*.json'))
        else:
            assert archive.is_file() and archive.stat().st_size == outcome['archive_bytes']
        counts[status] += 1
        if status == 'timeout-unfinished':
            unresolved.append(case['id'])
            continue
        if status == 'proven-infeasible':
            assert outcome['proof']['verified_infeasible_models'] > 0
            continue
        verified = read(directory / 'verified.json')
        assert verified['returned_solution_checked'] and verified['writeback_checked']
        assert verified['independent_recompute'] == case['recheck']
        cold_recomputes += int(case['recheck'])
        audit = read(directory / 'strict-output-audit.json')
        assert audit['case_id'] == case['id'] and audit['status'] == 'passed'
        assert audit['verifier_sha256'] == verifier['sha256'], 'outdated audit'
        assert digest(archive) == audit['archive_sha256'], 'archive changed since verification'
        proof = audit['proof']
        assert proof['verified_models'] > 0 and proof['verified_infeasible_models'] == 0
        assert proof['all_rounded_checks_passed'] and proof['all_within_verified_budgets']
        assert 0 <= proof['maximum_relative_balance_error'] <= 1e-12
        assert 0 <= proof['maximum_relative_gap_upper_bound'] <= 1e-12
        with zipfile.ZipFile(archive) as source:
            snapshots = {n for n in source.namelist()
                         if '/' not in n and n.startswith('lp-') and n.endswith('.json')}
            assert len(snapshots) == proof['verified_models']
            assert all('rust-certificates/' + n in source.namelist() for n in snapshots)
        witnessed_models += proof['verified_models']
    result = {'matrix_attempts_complete': True, 'cases': len(cases),
              'statuses': dict(counts), 'latest_verifier_sha256': verifier['sha256'],
              'verified_feasible_models': witnessed_models, 'cold_recomputes': cold_recomputes,
              'unresolved_solver_timeouts': unresolved, 'audited_at': time.time(),
              'note': 'This proves matrix execution and acceptance gates, not that every LP was solved.'}
    write(run / 'completion-audit.json', result)
    print(json.dumps({k: v for k, v in result.items() if k != 'unresolved_solver_timeouts'}), flush=True)


def profile_phases(run):
    """Use snapshot creation timestamps to distinguish preparation and LP stages.

    Intervals include serialization/next-model construction; they are estimates,
    not isolated solver timings. No LP model or witness is recomputed here.
    """
    rows = []
    for path in sorted(run.glob('*/outcome.json')):
        outcome = read(path)
        configured = read(path.parent / 'configured.json')
        archive_path = path.parent / 'capture.zip'
        snapshots = []
        if archive_path.exists():
            with zipfile.ZipFile(archive_path) as archive:
                for name in archive.namelist():
                    match = re.fullmatch(r'lp-\d+-(\d+)-\d+\.json', name)
                    if match:
                        snapshots.append((int(match[1]) / 1e9, name))
        else:
            assert outcome['archive_bytes'] == 0 and outcome['status'] == 'timeout-unfinished'
        snapshots.sort()
        started = outcome.get('started_at')
        offsets = [t - started for t, _ in snapshots] if started is not None else []
        assert all(-1 <= t <= outcome['process_seconds'] + 1 for t in offsets)
        text = (path.parent / 'test.log').read_text(encoding='utf8', errors='replace')
        model = re.search(r'\[LP-CAPTURE\].*?: (\d+) columns, (\d+) rows', text)
        stop_stage = ('before-first-captured-lp' if not snapshots else
                      'first-model-capture-or-solve' if len(snapshots) == 1 else
                      'later-model-check-or-recompute')
        record = {'case_id': outcome['case_id'], 'status': outcome['status'],
                  'context': outcome['context'], 'process_seconds': outcome['process_seconds'],
                  'snapshot_count': len(snapshots), 'observed_stage': stop_stage,
                  'start_timestamp_recorded': started is not None,
                  'loading_seconds': configured['load_ms'] / 1000,
                  'first_model_columns': int(model[1]) if model else None,
                  'first_model_rows': int(model[2]) if model else None,
                  'planning_preparation_seconds_estimate': max(0, offsets[0] - configured['load_ms'] / 1000)
                    if offsets else None,
                  'between_snapshot_seconds': [b[0] - a[0] for a, b in zip(snapshots, snapshots[1:])],
                  'after_last_snapshot_seconds': outcome['process_seconds'] - offsets[-1]
                    if offsets else None}
        rows.append(record)
    stopped = [r for r in rows if r['status'] == 'timeout-unfinished']
    summary = {'profiled_cases': len(rows),
               'start_timestamp_missing': sum(not r['start_timestamp_recorded'] for r in rows),
               'timeout_stages': dict(collections.Counter(r['observed_stage'] for r in stopped)),
               'largest_preparation_estimates': sorted(
                   [{'case_id': r['case_id'], 'seconds': r['planning_preparation_seconds_estimate'],
                     'columns': r['first_model_columns'], 'rows': r['first_model_rows']}
                    for r in rows if r['planning_preparation_seconds_estimate'] is not None],
                   key=lambda r: r['seconds'], reverse=True)[:12],
               'note': 'Snapshot intervals include serialization and next-model setup, not pure solve time.'}
    write(run / 'phase-profile.json', {'summary': summary, 'cases': rows})
    print(json.dumps(summary), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['generate', 'run', 'summary', 'audit', 'audit-inputs', 'audit-completion', 'profile-phases'])
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--executable', type=Path)
    parser.add_argument('--verifier', type=Path)
    parser.add_argument('--limit', type=int, default=100000)
    parser.add_argument('--case-timeout', type=float, default=180)
    parser.add_argument('--worker', default='serial')
    parser.add_argument('--qualities', nargs='*', choices=QUALITIES)
    parser.add_argument('--follow', action='store_true')
    args = parser.parse_args()
    run = args.run.resolve()
    assert run.is_relative_to((ROOT / 'local').resolve())
    if args.mode == 'generate':
        make_manifest(run)
    elif args.mode == 'summary':
        summarize(run)
    elif args.mode == 'audit':
        audit_outputs(run, args.verifier.resolve(), args.follow)
    elif args.mode == 'audit-inputs':
        audit_inputs(run)
    elif args.mode == 'audit-completion':
        audit_completion(run)
    elif args.mode == 'profile-phases':
        profile_phases(run)
    else:
        execute(run, args.executable.resolve(), args.verifier.resolve(), args.limit, args.case_timeout, args.worker, args.qualities)

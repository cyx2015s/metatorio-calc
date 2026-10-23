#!/usr/bin/env python3
"""Build an unverified route-family support hint for a captured LP.

Quality erasure and signed material families are heuristic, not mathematical
model equivalence. They ignore magnitudes, energy, costs, and machine identity.
Always validate any resulting solution against the original complete model.
This script neither solves a model nor changes final acceptance rules.
"""
import argparse
import hashlib
import json
import re
import struct
from fractions import Fraction
from pathlib import Path
TARGET = re.compile('target:TargetSpec \\{ constant: [^,{}]+, coefficients: \\{(Item\\(IdWithQuality \\{ id: "[^"\\\\]+", quality: "[^"\\\\]+" \\}\\)): 1\\.0\\} \\}')

def read_source(path):
    raw = path.read_bytes()
    return (json.loads(raw.decode('utf-8-sig')), {'path': str(path.resolve()), 'sha256': hashlib.sha256(raw).hexdigest()})
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('normal_snapshot', type=Path)
parser.add_argument('normal_certificate', type=Path)
parser.add_argument('current_snapshot', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
old, normal_identity = read_source(args.normal_snapshot)
cert, certificate_identity = read_source(args.normal_certificate)
if cert.get('accepted') is not True or cert.get('model_scope') != 'full':
    raise ValueError('normal certificate must be accepted and cover the full model')
if cert.get('certificate', {}).get('primal_feasible') is not True:
    raise ValueError('normal certificate must verify exact primal feasibility')
if len(cert['values']) != len(old['columns']):
    raise ValueError('normal certificate value count differs from normal snapshot')
certificate_snapshot = cert.get('original_snapshot')
if certificate_snapshot and Path(certificate_snapshot).exists():
    if hashlib.sha256(Path(certificate_snapshot).read_bytes()).hexdigest() != normal_identity['sha256']:
        raise ValueError('certificate original_snapshot differs from supplied normal snapshot')
registry = {}

def number(bits):
    return struct.unpack('<d', struct.pack('<Q', bits))[0]

def canonical(label):
    if label.startswith('target:'):
        m = TARGET.fullmatch(label)
        if not m:
            raise ValueError('only unit single-Item targets are supported: ' + label)
        label = 'balance:' + m.group(1)
        flip = True
    else:
        flip = False
    label = re.sub('quality: "[^"]+"', 'quality: "*"', label)
    return (label, flip)

def masks(snapshot):
    result = [0] * len(snapshot['columns'])
    for row in snapshot['rows']:
        label, flip = canonical(row['label'])
        if label in ('balance:Electricity', 'balance:Heat'):
            continue
        for column, bits in row['terms']:
            value = number(bits)
            if not value:
                continue
            sign = (value > 0) ^ flip
            token = (label, sign)
            bit = registry.setdefault(token, len(registry))
            result[column] |= 1 << bit
    return result
oldm = masks(old)
support = [i for i, v in enumerate(cert['values']) if Fraction(v) != 0 and '18446744073709551615' not in old['columns'][i]['label'] and old['columns'][i]['label'].startswith('flow:')]
families = {oldm[i] for i in support}
certificate_summary = {key: cert.get(key) for key in ('accepted', 'model_scope', 'acceptance_mode', 'original_snapshot')}
certificate_summary['primal_feasible'] = cert['certificate']['primal_feasible']
del oldm, old, cert
current, current_identity = read_source(args.current_snapshot)
cm = masks(current)
physical = [i for i, mask in enumerate(cm) if mask in families and current['columns'][i]['label'].startswith('flow:') and ('18446744073709551615' not in current['columns'][i]['label'])]
aux = [i for i, col in enumerate(current['columns']) if '18446744073709551615' in col['label'] or not col['label'].startswith('flow:')]
selected = set(physical + aux)
target = next((r for r in current['rows'] if r['label'].startswith('target:')))
producers = {i: number(v) for i, v in target['terms'] if number(v) > 0 and i in selected}
non_normal_inputs = set()
for row in current['rows']:
    if row['label'].startswith('target:'):
        continue
    if 'quality:' in row['label'] and 'quality: "normal"' not in row['label']:
        for i, v in row['terms']:
            if i in producers and number(v) > 0:
                non_normal_inputs.add(i)
ordinary = [i for i in producers if i not in non_normal_inputs]
metadata = {'purpose': 'unverified search support hint only; original full model certification required', 'mapping': 'signed non-energy material family; quality erased; target coefficients sign reversed into balance orientation', 'normal_physical_support': len(support), 'unique_normal_families': len(families), 'physical_quality_module_variants': len(physical), 'auxiliary_columns': len(aux), 'selected_columns': len(selected), 'current_total_columns': len(cm), 'selected_positive_target_producers': len(producers), 'normal_quality_input_legendary_output_columns': len(ordinary), 'ordinary_input_examples': ordinary[:20]}
metadata['sources'] = {'normal_snapshot': normal_identity, 'normal_certificate': certificate_identity, 'current_snapshot': current_identity}
metadata['normal_certificate'] = certificate_summary
metadata['heuristic_limits'] = 'signed family and quality erasure are not equivalence; validate against original full model'
metadata['selected_columns_sha256'] = hashlib.sha256(json.dumps(sorted(selected), separators=(',', ':')).encode()).hexdigest()
materials = set()
physical_materials = set()
physical_set = set(physical)
for row in current['rows']:
    if row['label'].startswith('target:'):
        continue
    if any((i in selected and number(v) != 0 for i, v in row['terms'])):
        materials.add(canonical(row['label'])[0])
    if any((i in physical_set and number(v) != 0 for i, v in row['terms'])):
        physical_materials.add(canonical(row['label'])[0])
metadata['selected_material_families'] = sorted(materials)
metadata['physical_material_families'] = sorted(physical_materials)
metadata['unmatched_normal_families'] = len(families - {cm[i] for i in physical})
output = {'selected_columns': sorted(selected), 'metadata': metadata}
with args.output.open('x', encoding='utf-8') as stream:
    json.dump(output, stream, indent=2)
print(json.dumps({k: v for k, v in metadata.items() if k not in ('selected_material_families', 'physical_material_families')}, indent=2))

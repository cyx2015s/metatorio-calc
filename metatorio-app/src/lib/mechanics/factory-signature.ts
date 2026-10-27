import type { AppDocument } from '../runtime/types';

const ordered = <T extends { id: number }>(items: T[]) => [...items].sort((a, b) => a.id - b.id);

/** Only numerical inputs invalidate a cached solve; names/order/display units do not. */
export function factorySolveSignatures(document: AppDocument | null): Map<string, string> {
  const signatures = new Map<string, string>();
  for (const project of document?.projects ?? []) {
    const { time_scale, ...settings } = project.settings;
    for (const factory of project.factories) {
      const { name, id, mechanics, targets, external_inputs, target_expressions, ...environment } = factory;
      signatures.set(`${project.id}:${id}`, JSON.stringify([project.context_id, settings, environment, {
        mechanics: ordered(mechanics), targets: ordered(targets), external_inputs: ordered(external_inputs),
        expressions: ordered(target_expressions).map(expression => ({ ...expression, terms: ordered(expression.terms) })),
      }]));
    }
  }
  return signatures;
}

export function factoryImplicitSignature(document: AppDocument | null, projectId: number | null, factoryId: number | null): string {
  const project = document?.projects.find(entry => entry.id === projectId);
  const factory = project?.factories.find(entry => entry.id === factoryId);
  const { time_scale, ...settings } = project?.settings ?? {};
  return JSON.stringify([projectId, factoryId, project?.context_id, settings, factory?.settings,
    factory?.external_inputs.map(input => JSON.stringify(input.flow)).sort()]);
}

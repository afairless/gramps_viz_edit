// Tests for the selection and export module.

import { describe, it, expect } from 'vitest';
import { SelectionManager, buildSelectedPeople, buildSelectionExport } from '../src/selection';
import type { GraphData, PersonNode } from '../src/types';

function makeNode(handle: string, overrides: Partial<PersonNode> = {}): PersonNode {
  return {
    handle,
    name: `Person ${handle}`,
    birth_date: null,
    death_date: null,
    birth_year: null,
    is_imputed: false,
    gender: 'unknown',
    family_group: 0,
    generation: 0,
    ...overrides,
  };
}

function makeGraph(nodes: PersonNode[]): GraphData {
  return { nodes, links: [], family_groups: [] };
}

describe('SelectionManager', () => {
  it('starts empty', () => {
    const sm = new SelectionManager();
    expect(sm.size).toBe(0);
    expect(sm.handles).toEqual([]);
  });

  it('toggle selects and deselects', () => {
    const sm = new SelectionManager();
    sm.toggle('p1');
    expect(sm.has('p1')).toBe(true);
    expect(sm.size).toBe(1);
    sm.toggle('p1');
    expect(sm.has('p1')).toBe(false);
    expect(sm.size).toBe(0);
  });

  it('click toggles without shift', () => {
    const sm = new SelectionManager();
    sm.click('p1', false);
    expect(sm.has('p1')).toBe(true);
    sm.click('p1', false);
    expect(sm.has('p1')).toBe(false);
  });

  it('shift-click adds to selection without toggling others', () => {
    const sm = new SelectionManager();
    sm.toggle('p1');
    sm.click('p2', true);
    expect(sm.has('p1')).toBe(true);
    expect(sm.has('p2')).toBe(true);
    expect(sm.size).toBe(2);
  });

  it('add and remove work independently', () => {
    const sm = new SelectionManager();
    sm.add('p1');
    sm.add('p2');
    expect(sm.size).toBe(2);
    sm.remove('p1');
    expect(sm.has('p1')).toBe(false);
    expect(sm.size).toBe(1);
  });

  it('clear removes all selections', () => {
    const sm = new SelectionManager();
    sm.add('p1');
    sm.add('p2');
    sm.add('p3');
    sm.clear();
    expect(sm.size).toBe(0);
    expect(sm.handles).toEqual([]);
  });

  it('handles returns sorted order of insertion', () => {
    const sm = new SelectionManager();
    sm.add('p3');
    sm.add('p1');
    sm.add('p2');
    expect(sm.handles).toEqual(['p3', 'p1', 'p2']);
  });

  it('empty selection has no handles', () => {
    const sm = new SelectionManager();
    expect(sm.handles).toEqual([]);
  });
});

describe('buildSelectedPeople', () => {
  it('returns selected people from graph data', () => {
    const data = makeGraph([makeNode('p1', { name: 'Alice' }), makeNode('p2', { name: 'Bob' })]);
    const result = buildSelectedPeople(data, ['p1']);
    expect(result).toHaveLength(1);
    expect(result[0].handle).toBe('p1');
    expect(result[0].name).toBe('Alice');
  });

  it('skips handles not found in graph data', () => {
    const data = makeGraph([makeNode('p1')]);
    const result = buildSelectedPeople(data, ['p1', 'ghost']);
    expect(result).toHaveLength(1);
    expect(result[0].handle).toBe('p1');
  });

  it('returns empty array for empty handles', () => {
    const data = makeGraph([makeNode('p1')]);
    expect(buildSelectedPeople(data, [])).toEqual([]);
  });

  it('includes all fields in SelectedPerson', () => {
    const data = makeGraph([
      makeNode('p1', {
        name: 'Alice',
        birth_date: '1850-03-15',
        death_date: '1920-07-01',
        gender: 'female',
        family_group: 1,
      }),
    ]);
    const result = buildSelectedPeople(data, ['p1']);
    expect(result).toHaveLength(1);
    expect(result[0]).toEqual({
      handle: 'p1',
      name: 'Alice',
      birth_date: '1850-03-15',
      death_date: '1920-07-01',
      gender: 'female',
      family_group: 1,
    });
  });
});

describe('buildSelectionExport', () => {
  it('builds a valid export payload', () => {
    const exportData = buildSelectionExport(
      'out.json',
      [{ handle: 'p1', name: 'Alice', birth_date: null, death_date: null, gender: 'female', family_group: 0 }],
      new Date('2025-01-01T00:00:00Z'),
    );
    expect(exportData.exported_at).toBe('2025-01-01T00:00:00.000Z');
    expect(exportData.file).toBe('out.json');
    expect(exportData.selections).toHaveLength(1);
    expect(exportData.selections[0].handle).toBe('p1');
  });

  it('uses current date when exportedAt is not provided', () => {
    const before = new Date();
    const exportData = buildSelectionExport('out.json', []);
    const after = new Date();
    const exportedAt = new Date(exportData.exported_at);
    expect(exportedAt.getTime()).toBeGreaterThanOrEqual(before.getTime());
    expect(exportedAt.getTime()).toBeLessThanOrEqual(after.getTime());
  });

  it('handles empty selections', () => {
    const exportData = buildSelectionExport('out.json', []);
    expect(exportData.selections).toEqual([]);
  });
});
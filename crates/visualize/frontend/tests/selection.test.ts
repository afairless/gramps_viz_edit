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

  it('clickWithIndirect with empty indirects behaves like click (toggle)', () => {
    const sm = new SelectionManager();
    sm.clickWithIndirect('p1', new Set());
    expect(sm.has('p1')).toBe(true);
    expect(sm.size).toBe(1);
    sm.clickWithIndirect('p1', new Set());
    expect(sm.has('p1')).toBe(false);
    expect(sm.size).toBe(0);
  });

  it('clickWithIndirect adds node + indirects when node unselected', () => {
    const sm = new SelectionManager();
    sm.clickWithIndirect('p1', new Set(['p2', 'p3']));
    expect(sm.has('p1')).toBe(true);
    expect(sm.has('p2')).toBe(true);
    expect(sm.has('p3')).toBe(true);
    expect(sm.size).toBe(3);
  });

  it('clickWithIndirect removes node + indirects when node selected, even if some indirects were selected via other means', () => {
    const sm = new SelectionManager();
    // p1 selected via direct click; p2 selected via a different action
    sm.add('p1');
    sm.add('p2');
    // Click p1 again with p2, p3 as indirects — removes p1, p2 (unconditional), and p3 is no-op
    sm.clickWithIndirect('p1', new Set(['p2', 'p3']));
    expect(sm.has('p1')).toBe(false);
    expect(sm.has('p2')).toBe(false); // removed even though selected via other means
    expect(sm.has('p3')).toBe(false);
    expect(sm.size).toBe(0);
  });

  it('clickWithIndirect does not remove indirects when adding (additive behavior)', () => {
    const sm = new SelectionManager();
    sm.add('p4'); // pre-existing selection
    sm.clickWithIndirect('p1', new Set(['p2']));
    expect(sm.has('p1')).toBe(true);
    expect(sm.has('p2')).toBe(true);
    expect(sm.has('p4')).toBe(true); // preserved
    expect(sm.size).toBe(3);
  });

  it('addAll adds multiple handles', () => {
    const sm = new SelectionManager();
    sm.addAll(['p1', 'p2', 'p3']);
    expect(sm.has('p1')).toBe(true);
    expect(sm.has('p2')).toBe(true);
    expect(sm.has('p3')).toBe(true);
    expect(sm.size).toBe(3);
  });

  it('removeAll removes multiple handles', () => {
    const sm = new SelectionManager();
    sm.addAll(['p1', 'p2', 'p3']);
    sm.removeAll(['p1', 'p2']);
    expect(sm.has('p1')).toBe(false);
    expect(sm.has('p2')).toBe(false);
    expect(sm.has('p3')).toBe(true);
    expect(sm.size).toBe(1);
  });

  it('addAll with already-selected handles is idempotent (no double-counting)', () => {
    const sm = new SelectionManager();
    sm.addAll(['p1', 'p2']);
    sm.addAll(['p2', 'p3']);
    expect(sm.size).toBe(3);
    expect(sm.has('p1')).toBe(true);
    expect(sm.has('p2')).toBe(true);
    expect(sm.has('p3')).toBe(true);
  });

  it('removeAll with non-selected handles is idempotent', () => {
    const sm = new SelectionManager();
    sm.add('p1');
    sm.removeAll(['p2', 'p3']); // not selected
    expect(sm.size).toBe(1);
    expect(sm.has('p1')).toBe(true);
  });

  it('addAll([]) is a no-op (empty iterable)', () => {
    const sm = new SelectionManager();
    sm.add('p1');
    sm.addAll([]);
    expect(sm.size).toBe(1);
    expect(sm.has('p1')).toBe(true);
  });

  it('removeAll([]) is a no-op (empty iterable)', () => {
    const sm = new SelectionManager();
    sm.addAll(['p1', 'p2']);
    sm.removeAll([]);
    expect(sm.size).toBe(2);
  });

  describe('invertSelection', () => {
    it('selects all when none selected', () => {
      const sm = new SelectionManager();
      sm.invertSelection(['p1', 'p2']);
      expect(sm.has('p1')).toBe(true);
      expect(sm.has('p2')).toBe(true);
      expect(sm.size).toBe(2);
    });

    it('deselects all when all selected', () => {
      const sm = new SelectionManager();
      sm.addAll(['p1', 'p2']);
      sm.invertSelection(['p1', 'p2']);
      expect(sm.has('p1')).toBe(false);
      expect(sm.has('p2')).toBe(false);
      expect(sm.size).toBe(0);
    });

    it('flips mixed state correctly', () => {
      const sm = new SelectionManager();
      sm.add('p1');
      sm.invertSelection(['p1', 'p2', 'p3']);
      expect(sm.has('p1')).toBe(false); // was selected, now deselected
      expect(sm.has('p2')).toBe(true);  // was unselected, now selected
      expect(sm.has('p3')).toBe(true);  // was unselected, now selected
      expect(sm.size).toBe(2);
    });

    it('is a no-op with empty iterable', () => {
      const sm = new SelectionManager();
      sm.add('p1');
      sm.invertSelection([]);
      expect(sm.size).toBe(1);
      expect(sm.has('p1')).toBe(true);
    });

    it('leaves handles outside iterable untouched', () => {
      const sm = new SelectionManager();
      sm.add('p1');
      sm.add('p2');
      sm.invertSelection(['p1']);
      expect(sm.has('p1')).toBe(false); // toggled
      expect(sm.has('p2')).toBe(true);  // untouched
      expect(sm.size).toBe(1);
    });

    it('is idempotent on double-invert (back to original state)', () => {
      const sm = new SelectionManager();
      sm.add('p1');
      sm.invertSelection(['p1', 'p2']);
      sm.invertSelection(['p1', 'p2']);
      expect(sm.has('p1')).toBe(true);  // back to original
      expect(sm.has('p2')).toBe(false); // back to original
      expect(sm.size).toBe(1);
    });
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
// Tests for the graph topology query module.

import { describe, it, expect } from 'vitest';
import {
  buildAdjacency,
  getAncestors,
  getDescendants,
  getFirstDegree,
  getSecondDegree,
  getIndirectSet,
} from '../src/graph-query';
import type { GraphData, PersonNode, FamilyLink } from '../src/types';

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

function makeGraph(
  nodes: PersonNode[],
  links: FamilyLink[],
): GraphData {
  return { nodes, links, family_groups: [] };
}

// ---------------------------------------------------------------------------
// buildAdjacency
// ---------------------------------------------------------------------------

describe('buildAdjacency', () => {
  it('creates an entry for every node, including isolated nodes', () => {
    const data = makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [],
    );
    const adj = buildAdjacency(data);
    expect(adj.has('p1')).toBe(true);
    expect(adj.has('p2')).toBe(true);
    expect(adj.size).toBe(2);
  });

  it('populates parent-child links', () => {
    const data = makeGraph(
      [makeNode('parent'), makeNode('child')],
      [{ source: 'parent', target: 'child', link_type: 'ParentChild' }],
    );
    const adj = buildAdjacency(data);
    expect(adj.get('parent')!.children).toEqual(new Set(['child']));
    expect(adj.get('child')!.parents).toEqual(new Set(['parent']));
  });

  it('populates spouse links', () => {
    const data = makeGraph(
      [makeNode('a'), makeNode('b')],
      [{ source: 'a', target: 'b', link_type: 'Spouse' }],
    );
    const adj = buildAdjacency(data);
    expect(adj.get('a')!.spouses).toEqual(new Set(['b']));
    expect(adj.get('b')!.spouses).toEqual(new Set(['a']));
  });

  it('derives siblings from shared parents', () => {
    const data = makeGraph(
      [makeNode('parent'), makeNode('child1'), makeNode('child2')],
      [
        { source: 'parent', target: 'child1', link_type: 'ParentChild' },
        { source: 'parent', target: 'child2', link_type: 'ParentChild' },
      ],
    );
    const adj = buildAdjacency(data);
    expect(adj.get('child1')!.siblings).toEqual(new Set(['child2']));
    expect(adj.get('child2')!.siblings).toEqual(new Set(['child1']));
  });

  it('computes allNeighbors as union of parents, children, spouses, siblings', () => {
    const data = makeGraph(
      [makeNode('parent'), makeNode('child1'), makeNode('child2')],
      [
        { source: 'parent', target: 'child1', link_type: 'ParentChild' },
        { source: 'parent', target: 'child2', link_type: 'ParentChild' },
      ],
    );
    const adj = buildAdjacency(data);
    // child1's allNeighbors = parent + child2 (sibling)
    expect(adj.get('child1')!.allNeighbors).toEqual(new Set(['parent', 'child2']));
  });

  it('handles empty graph', () => {
    const data = makeGraph([], []);
    const adj = buildAdjacency(data);
    expect(adj.size).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// getAncestors
// ---------------------------------------------------------------------------

describe('getAncestors', () => {
  it('returns empty set for node not in graph', () => {
    const adj = new Map();
    expect(getAncestors(adj, 'ghost')).toEqual(new Set());
  });

  it('returns empty set for isolated node', () => {
    const data = makeGraph([makeNode('p1')], []);
    const adj = buildAdjacency(data);
    expect(getAncestors(adj, 'p1')).toEqual(new Set());
  });

  it('finds parent of a child', () => {
    const data = makeGraph(
      [makeNode('parent'), makeNode('child')],
      [{ source: 'parent', target: 'child', link_type: 'ParentChild' }],
    );
    const adj = buildAdjacency(data);
    expect(getAncestors(adj, 'child')).toEqual(new Set(['parent']));
  });

  it('finds grandparent via three-generation chain', () => {
    const data = makeGraph(
      [makeNode('gp'), makeNode('p'), makeNode('c')],
      [
        { source: 'gp', target: 'p', link_type: 'ParentChild' },
        { source: 'p', target: 'c', link_type: 'ParentChild' },
      ],
    );
    const adj = buildAdjacency(data);
    expect(getAncestors(adj, 'c')).toEqual(new Set(['p', 'gp']));
  });

  it('does not include the starting node', () => {
    const data = makeGraph(
      [makeNode('p'), makeNode('c')],
      [{ source: 'p', target: 'c', link_type: 'ParentChild' }],
    );
    const adj = buildAdjacency(data);
    const ancestors = getAncestors(adj, 'c');
    expect(ancestors.has('c')).toBe(false);
  });

  it('handles cycles without infinite loop', () => {
    const data = makeGraph(
      [makeNode('a'), makeNode('b'), makeNode('c')],
      [
        { source: 'a', target: 'b', link_type: 'ParentChild' },
        { source: 'b', target: 'c', link_type: 'ParentChild' },
        { source: 'c', target: 'a', link_type: 'ParentChild' },
      ],
    );
    const adj = buildAdjacency(data);
    const ancestors = getAncestors(adj, 'a');
    // In a 3-node cycle, all 3 nodes are reachable via parent links
    expect(ancestors.size).toBe(3);
    expect(ancestors.has('b')).toBe(true);
    expect(ancestors.has('c')).toBe(true);
    // Cycle closes back on the start node — no infinite loop
    expect(ancestors.has('a')).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// getDescendants
// ---------------------------------------------------------------------------

describe('getDescendants', () => {
  it('returns empty set for node not in graph', () => {
    const adj = new Map();
    expect(getDescendants(adj, 'ghost')).toEqual(new Set());
  });

  it('returns empty set for isolated node', () => {
    const data = makeGraph([makeNode('p1')], []);
    const adj = buildAdjacency(data);
    expect(getDescendants(adj, 'p1')).toEqual(new Set());
  });

  it('finds child of a parent', () => {
    const data = makeGraph(
      [makeNode('parent'), makeNode('child')],
      [{ source: 'parent', target: 'child', link_type: 'ParentChild' }],
    );
    const adj = buildAdjacency(data);
    expect(getDescendants(adj, 'parent')).toEqual(new Set(['child']));
  });

  it('finds grandchild via three-generation chain', () => {
    const data = makeGraph(
      [makeNode('gp'), makeNode('p'), makeNode('c')],
      [
        { source: 'gp', target: 'p', link_type: 'ParentChild' },
        { source: 'p', target: 'c', link_type: 'ParentChild' },
      ],
    );
    const adj = buildAdjacency(data);
    expect(getDescendants(adj, 'gp')).toEqual(new Set(['p', 'c']));
  });

  it('does not include the starting node', () => {
    const data = makeGraph(
      [makeNode('p'), makeNode('c')],
      [{ source: 'p', target: 'c', link_type: 'ParentChild' }],
    );
    const adj = buildAdjacency(data);
    const descendants = getDescendants(adj, 'p');
    expect(descendants.has('p')).toBe(false);
  });

  it('handles cycles without infinite loop', () => {
    const data = makeGraph(
      [makeNode('a'), makeNode('b'), makeNode('c')],
      [
        { source: 'a', target: 'b', link_type: 'ParentChild' },
        { source: 'b', target: 'c', link_type: 'ParentChild' },
        { source: 'c', target: 'a', link_type: 'ParentChild' },
      ],
    );
    const adj = buildAdjacency(data);
    const descendants = getDescendants(adj, 'a');
    // In a 3-node cycle, all 3 nodes are reachable via child links
    expect(descendants.size).toBe(3);
    expect(descendants.has('b')).toBe(true);
    expect(descendants.has('c')).toBe(true);
    // Cycle closes back on the start node — no infinite loop
    expect(descendants.has('a')).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// getFirstDegree
// ---------------------------------------------------------------------------

describe('getFirstDegree', () => {
  it('returns empty set for node not in graph', () => {
    const adj = new Map();
    expect(getFirstDegree(adj, 'ghost')).toEqual(new Set());
  });

  it('returns empty set for isolated node', () => {
    const data = makeGraph([makeNode('p1')], []);
    const adj = buildAdjacency(data);
    expect(getFirstDegree(adj, 'p1')).toEqual(new Set());
  });

  it('includes spouse', () => {
    const data = makeGraph(
      [makeNode('a'), makeNode('b')],
      [{ source: 'a', target: 'b', link_type: 'Spouse' }],
    );
    const adj = buildAdjacency(data);
    expect(getFirstDegree(adj, 'a')).toEqual(new Set(['b']));
    expect(getFirstDegree(adj, 'b')).toEqual(new Set(['a']));
  });

  it('includes parent and child', () => {
    const data = makeGraph(
      [makeNode('parent'), makeNode('child')],
      [{ source: 'parent', target: 'child', link_type: 'ParentChild' }],
    );
    const adj = buildAdjacency(data);
    expect(getFirstDegree(adj, 'parent')).toEqual(new Set(['child']));
    expect(getFirstDegree(adj, 'child')).toEqual(new Set(['parent']));
  });

  it('includes spouse, parent, and child for a married parent', () => {
    const data = makeGraph(
      [makeNode('spouse'), makeNode('parent'), makeNode('child')],
      [
        { source: 'parent', target: 'spouse', link_type: 'Spouse' },
        { source: 'parent', target: 'child', link_type: 'ParentChild' },
      ],
    );
    const adj = buildAdjacency(data);
    expect(getFirstDegree(adj, 'parent')).toEqual(new Set(['spouse', 'child']));
  });
});

// ---------------------------------------------------------------------------
// getSecondDegree
// ---------------------------------------------------------------------------

describe('getSecondDegree', () => {
  it('returns empty set for node not in graph', () => {
    const adj = new Map();
    expect(getSecondDegree(adj, 'ghost')).toEqual(new Set());
  });

  it('returns empty set for isolated node', () => {
    const data = makeGraph([makeNode('p1')], []);
    const adj = buildAdjacency(data);
    expect(getSecondDegree(adj, 'p1')).toEqual(new Set());
  });

  it('includes 1st-degree connections (spouse)', () => {
    const data = makeGraph(
      [makeNode('a'), makeNode('b')],
      [{ source: 'a', target: 'b', link_type: 'Spouse' }],
    );
    const adj = buildAdjacency(data);
    expect(getSecondDegree(adj, 'a')).toEqual(new Set(['b']));
  });

  it('includes siblings (via shared parents)', () => {
    const data = makeGraph(
      [makeNode('parent'), makeNode('child1'), makeNode('child2')],
      [
        { source: 'parent', target: 'child1', link_type: 'ParentChild' },
        { source: 'parent', target: 'child2', link_type: 'ParentChild' },
      ],
    );
    const adj = buildAdjacency(data);
    // child1's 2nd-degree = parent (1st) + child2 (sibling, 2nd)
    // But also: parent's 1st-degree includes child1, so child1's 2nd-degree = parent + child2
    const result = getSecondDegree(adj, 'child1');
    expect(result.has('parent')).toBe(true);
    expect(result.has('child2')).toBe(true);
  });

  it('three-generation chain: grandparent sees parent and child', () => {
    const data = makeGraph(
      [makeNode('gp'), makeNode('p'), makeNode('c')],
      [
        { source: 'gp', target: 'p', link_type: 'ParentChild' },
        { source: 'p', target: 'c', link_type: 'ParentChild' },
      ],
    );
    const adj = buildAdjacency(data);
    // gp's 2nd-degree = p (1st) + c (2nd via p)
    const result = getSecondDegree(adj, 'gp');
    expect(result).toEqual(new Set(['p', 'c']));
  });

  it('three-generation chain: child sees parent and grandparent', () => {
    const data = makeGraph(
      [makeNode('gp'), makeNode('p'), makeNode('c')],
      [
        { source: 'gp', target: 'p', link_type: 'ParentChild' },
        { source: 'p', target: 'c', link_type: 'ParentChild' },
      ],
    );
    const adj = buildAdjacency(data);
    // c's 2nd-degree = p (1st) + gp (2nd via p)
    const result = getSecondDegree(adj, 'c');
    expect(result).toEqual(new Set(['p', 'gp']));
  });
});

// ---------------------------------------------------------------------------
// getIndirectSet
// ---------------------------------------------------------------------------

describe('getIndirectSet', () => {
  it('returns empty set for single mode', () => {
    const data = makeGraph([makeNode('p1')], []);
    const adj = buildAdjacency(data);
    expect(getIndirectSet(adj, 'p1', 'single')).toEqual(new Set());
  });

  it('dispatches to getAncestors for ancestors mode', () => {
    const data = makeGraph(
      [makeNode('p'), makeNode('c')],
      [{ source: 'p', target: 'c', link_type: 'ParentChild' }],
    );
    const adj = buildAdjacency(data);
    expect(getIndirectSet(adj, 'c', 'ancestors')).toEqual(new Set(['p']));
  });

  it('dispatches to getDescendants for descendants mode', () => {
    const data = makeGraph(
      [makeNode('p'), makeNode('c')],
      [{ source: 'p', target: 'c', link_type: 'ParentChild' }],
    );
    const adj = buildAdjacency(data);
    expect(getIndirectSet(adj, 'p', 'descendants')).toEqual(new Set(['c']));
  });

  it('dispatches to getFirstDegree for first-degree mode', () => {
    const data = makeGraph(
      [makeNode('a'), makeNode('b')],
      [{ source: 'a', target: 'b', link_type: 'Spouse' }],
    );
    const adj = buildAdjacency(data);
    expect(getIndirectSet(adj, 'a', 'first-degree')).toEqual(new Set(['b']));
  });

  it('dispatches to getSecondDegree for second-degree mode', () => {
    const data = makeGraph(
      [makeNode('gp'), makeNode('p'), makeNode('c')],
      [
        { source: 'gp', target: 'p', link_type: 'ParentChild' },
        { source: 'p', target: 'c', link_type: 'ParentChild' },
      ],
    );
    const adj = buildAdjacency(data);
    const result = getIndirectSet(adj, 'c', 'second-degree');
    expect(result.has('p')).toBe(true);
    expect(result.has('gp')).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Property-based invariants
// ---------------------------------------------------------------------------

describe('property-based invariants', () => {
  it('getAncestors never returns the starting node', () => {
    const data = makeGraph(
      [makeNode('gp'), makeNode('p'), makeNode('c')],
      [
        { source: 'gp', target: 'p', link_type: 'ParentChild' },
        { source: 'p', target: 'c', link_type: 'ParentChild' },
      ],
    );
    const adj = buildAdjacency(data);
    for (const handle of ['gp', 'p', 'c']) {
      expect(getAncestors(adj, handle).has(handle)).toBe(false);
    }
  });

  it('getDescendants never returns the starting node', () => {
    const data = makeGraph(
      [makeNode('gp'), makeNode('p'), makeNode('c')],
      [
        { source: 'gp', target: 'p', link_type: 'ParentChild' },
        { source: 'p', target: 'c', link_type: 'ParentChild' },
      ],
    );
    const adj = buildAdjacency(data);
    for (const handle of ['gp', 'p', 'c']) {
      expect(getDescendants(adj, handle).has(handle)).toBe(false);
    }
  });

  it('getFirstDegree never returns the starting node', () => {
    const data = makeGraph(
      [makeNode('a'), makeNode('b')],
      [{ source: 'a', target: 'b', link_type: 'Spouse' }],
    );
    const adj = buildAdjacency(data);
    expect(getFirstDegree(adj, 'a').has('a')).toBe(false);
    expect(getFirstDegree(adj, 'b').has('b')).toBe(false);
  });

  it('getSecondDegree never returns the starting node', () => {
    const data = makeGraph(
      [makeNode('gp'), makeNode('p'), makeNode('c')],
      [
        { source: 'gp', target: 'p', link_type: 'ParentChild' },
        { source: 'p', target: 'c', link_type: 'ParentChild' },
      ],
    );
    const adj = buildAdjacency(data);
    for (const handle of ['gp', 'p', 'c']) {
      expect(getSecondDegree(adj, handle).has(handle)).toBe(false);
    }
  });
});
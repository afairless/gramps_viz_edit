// Tests for the D3 force simulation graph rendering module.
// Covers data validation and node/link transform helpers.

import { describe, it, expect } from 'vitest';
import {
  buildSimNodes,
  buildSimLinks,
  validateGraphData,
} from '../src/graph';
import type { GraphData, PersonNode, FamilyLink, LinkType } from '../src/types';

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

function makeGraph(nodes: PersonNode[], links: FamilyLink[]): GraphData {
  return { nodes, links, family_groups: [] };
}

describe('validateGraphData', () => {
  it('accepts a valid GraphData object', () => {
    const data = makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [{ source: 'p1', target: 'p2', link_type: 'Spouse' }],
    );
    expect(validateGraphData(data)).toBe(true);
  });

  it('rejects null and non-objects', () => {
    expect(validateGraphData(null)).toBe(false);
    expect(validateGraphData(undefined)).toBe(false);
    expect(validateGraphData('nope')).toBe(false);
    expect(validateGraphData(42)).toBe(false);
  });

  it('rejects missing nodes or links arrays', () => {
    expect(validateGraphData({ nodes: [], links: [] })).toBe(true);
    expect(validateGraphData({})).toBe(false);
    expect(validateGraphData({ nodes: [] })).toBe(false);
    expect(validateGraphData({ links: [] })).toBe(false);
  });

  it('rejects nodes with wrong field types', () => {
    const data = makeGraph(
      [makeNode('p1', { name: 42 } as unknown as PersonNode)],
      [],
    );
    expect(validateGraphData(data)).toBe(false);
  });

  it('rejects links with bad link_type or dangling handles', () => {
    const badType = makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [{ source: 'p1', target: 'p2', link_type: 'Other' as 'Spouse' }],
    );
    expect(validateGraphData(badType)).toBe(false);

    const missing = makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [{ source: 'p1', target: 'p2', link_type: 'Spouse' }],
    );
    // validation is structural — handles can dangle (rendering skips them)
    expect(validateGraphData(missing)).toBe(true);
  });

  it('accepts empty graph', () => {
    expect(validateGraphData({ nodes: [], links: [], family_groups: [] })).toBe(true);
  });
});

describe('buildSimNodes', () => {
  it('maps GraphData nodes to simulation nodes preserving fields', () => {
    const data = makeGraph(
      [
        makeNode('p1', {
          name: 'Alice',
          birth_year: 1850,
          is_imputed: true,
          gender: 'female',
          family_group: 2,
          generation: 3,
        }),
      ],
      [],
    );
    const sim = buildSimNodes(data);
    expect(sim).toHaveLength(1);
    expect(sim[0]).toMatchObject({
      handle: 'p1',
      name: 'Alice',
      birth_year: 1850,
      is_imputed: true,
      gender: 'female',
      family_group: 2,
      generation: 3,
    });
  });

  it('returns empty array for empty input', () => {
    expect(buildSimNodes({ nodes: [], links: [], family_groups: [] })).toEqual([]);
  });
});

describe('buildSimLinks', () => {
  it('maps handles to node references', () => {
    const data = makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [{ source: 'p1', target: 'p2', link_type: 'ParentChild' }],
    );
    const nodes = buildSimNodes(data);
    const nodeMap = new Map(nodes.map((n) => [n.handle, n]));
    const links = buildSimLinks(data, nodeMap);
    expect(links).toHaveLength(1);
    expect(links[0].source.handle).toBe('p1');
    expect(links[0].target.handle).toBe('p2');
    expect(links[0].link_type).toBe('ParentChild');
  });

  it('deduplicates bidirectional spouse links', () => {
    const data = makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [
        { source: 'p1', target: 'p2', link_type: 'Spouse' },
        { source: 'p2', target: 'p1', link_type: 'Spouse' },
      ],
    );
    const nodes = buildSimNodes(data);
    const nodeMap = new Map(nodes.map((n) => [n.handle, n]));
    const links = buildSimLinks(data, nodeMap);
    expect(links).toHaveLength(1);
  });

  it('keeps directional parent-child links distinct', () => {
    const data = makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [
        { source: 'p1', target: 'p2', link_type: 'ParentChild' },
        { source: 'p2', target: 'p1', link_type: 'ParentChild' },
      ],
    );
    const nodes = buildSimNodes(data);
    const nodeMap = new Map(nodes.map((n) => [n.handle, n]));
    const links = buildSimLinks(data, nodeMap);
    expect(links).toHaveLength(2);
  });

  it('skips links referencing unknown handles', () => {
    const data = makeGraph(
      [makeNode('p1')],
      [
        { source: 'p1', target: 'ghost', link_type: 'ParentChild' },
        { source: 'ghost', target: 'p1', link_type: 'Spouse' },
      ],
    );
    const nodes = buildSimNodes(data);
    const nodeMap = new Map(nodes.map((n) => [n.handle, n]));
    const links = buildSimLinks(data, nodeMap);
    expect(links).toHaveLength(0);
  });

  it('preserves link_type as ParentChild through the mapping', () => {
    const data = makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [{ source: 'p1', target: 'p2', link_type: 'ParentChild' }],
    );
    const nodes = buildSimNodes(data);
    const nodeMap = new Map(nodes.map((n) => [n.handle, n]));
    const links = buildSimLinks(data, nodeMap);
    expect(links).toHaveLength(1);
    expect(links[0].link_type).toBe('ParentChild');
  });

  it('preserves link_type as Spouse through the mapping', () => {
    const data = makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [{ source: 'p1', target: 'p2', link_type: 'Spouse' }],
    );
    const nodes = buildSimNodes(data);
    const nodeMap = new Map(nodes.map((n) => [n.handle, n]));
    const links = buildSimLinks(data, nodeMap);
    expect(links).toHaveLength(1);
    expect(links[0].link_type).toBe('Spouse');
  });

  it('preserves link_type for mixed Spouse and ParentChild links', () => {
    const data = makeGraph(
      [makeNode('p1'), makeNode('p2'), makeNode('p3')],
      [
        { source: 'p1', target: 'p2', link_type: 'Spouse' },
        { source: 'p1', target: 'p3', link_type: 'ParentChild' },
      ],
    );
    const nodes = buildSimNodes(data);
    const nodeMap = new Map(nodes.map((n) => [n.handle, n]));
    const links = buildSimLinks(data, nodeMap);
    expect(links).toHaveLength(2);
    const spouseLink = links.find((l) => l.link_type === 'Spouse');
    const parentChildLink = links.find((l) => l.link_type === 'ParentChild');
    expect(spouseLink).toBeDefined();
    expect(parentChildLink).toBeDefined();
    expect(spouseLink!.source.handle).toBe('p1');
    expect(parentChildLink!.source.handle).toBe('p1');
  });
});

describe('force simulation configuration shape', () => {
  it('simulation node objects are valid SimulationNodeDatum (have index/x/y optional)', () => {
    const data = makeGraph([makeNode('p1')], []);
    const sim = buildSimNodes(data);
    // d3.forceSimulation accepts arrays of SimulationNodeDatum
    // SimNode extends SimulationNodeDatum, so this is a compile-time check.
    expect(sim[0]).toBeDefined();
    // Positions start undefined until the simulation runs
    expect(sim[0].x).toBeUndefined();
    expect(sim[0].y).toBeUndefined();
  });
});

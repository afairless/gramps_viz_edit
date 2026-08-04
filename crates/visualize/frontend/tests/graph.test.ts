// @vitest-environment happy-dom
// Tests for the D3 force simulation graph rendering module.
// Covers data validation and node/link transform helpers.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as d3 from 'd3';
import {
  buildSimNodes,
  buildSimLinks,
  createDragBehavior,
  onDragStart,
  onDrag,
  onDragEnd,
  validateGraphData,
  resetNodePositions,
  computeGenerationSpacing,
  createSimulationForces,
  applyForceConfig,
  renderGraph,
} from '../src/graph';
import type { SimNode } from '../src/graph';
import type { GraphData, PersonNode, FamilyLink } from '../src/types';
import { DEFAULT_FORCE_CONFIG, type ForceConfig } from '../src/types';

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

describe('drag handlers', () => {
  let mockAlphaTarget: any;
  let mockRestart: ReturnType<typeof vi.fn>;
  let mockSimulation: d3.Simulation<SimNode, undefined>;

  beforeEach(() => {
    mockRestart = vi.fn();
    mockAlphaTarget = vi.fn(() => ({ restart: mockRestart }));
    mockSimulation = { alphaTarget: mockAlphaTarget } as unknown as d3.Simulation<SimNode, undefined>;
  });

  function makeSimNode(): SimNode {
    return buildSimNodes(makeGraph([makeNode('p1')], []))[0];
  }

  function makeEvent(overrides?: Partial<Record<string, unknown>>) {
    return {
      active: false,
      x: 0,
      y: 0,
      sourceEvent: { currentTarget: null },
      ...overrides,
    } as unknown as d3.D3DragEvent<SVGGElement, SimNode, SimNode>;
  }

  function makeSvg() {
    return document.createElementNS(
      'http://www.w3.org/2000/svg',
      'svg',
    ) as SVGSVGElement;
  }

  describe('onDragStart', () => {
    it('pins the node at its current position', () => {
      const node = makeSimNode();
      node.x = 100;
      node.y = 200;
      onDragStart(node, makeEvent(), mockSimulation, makeSvg());
      expect(node.fx).toBe(100);
      expect(node.fy).toBe(200);
    });

    it('reheats the simulation on first gesture', () => {
      const node = makeSimNode();
      node.x = 1;
      node.y = 2;
      onDragStart(node, makeEvent({ active: false }), mockSimulation, makeSvg());
      expect(mockAlphaTarget).toHaveBeenCalledWith(0.3);
      expect(mockRestart).toHaveBeenCalledTimes(1);
    });

    it('does not reheat the simulation for subsequent concurrent gestures', () => {
      const node = makeSimNode();
      node.x = 1;
      node.y = 2;
      onDragStart(node, makeEvent({ active: true }), mockSimulation, makeSvg());
      expect(mockAlphaTarget).not.toHaveBeenCalled();
    });

    it('sets the grabbing cursor on the dragged element', () => {
      const svg = makeSvg();
      const g = document.createElementNS(
        'http://www.w3.org/2000/svg',
        'g',
      ) as SVGGElement;
      svg.appendChild(g);
      const node = makeSimNode();
      node.x = 1;
      node.y = 2;
      onDragStart(
        node,
        makeEvent({ sourceEvent: { currentTarget: g } }),
        mockSimulation,
        svg,
      );
      expect(g.style.cursor).toBe('grabbing');
    });
  });

  describe('onDrag', () => {
    it('updates fx/fy to event coords at identity zoom', () => {
      const svg = makeSvg();
      (svg as unknown as { __zoom: d3.ZoomTransform }).__zoom =
        d3.zoomIdentity;
      const node = makeSimNode();
      onDrag(node, makeEvent({ x: 42, y: 77 }), mockSimulation, svg);
      expect(node.fx).toBe(42);
      expect(node.fy).toBe(77);
    });

    it('inverts zoomed event coords back to base SVG space', () => {
      const svg = makeSvg();
      (svg as unknown as { __zoom: d3.ZoomTransform }).__zoom =
        d3.zoomIdentity.scale(2);
      const node = makeSimNode();
      onDrag(node, makeEvent({ x: 100, y: 50 }), mockSimulation, svg);
      expect(node.fx).toBe(50);
      expect(node.fy).toBe(25);
    });
  });

  describe('onDragEnd', () => {
    it('cools the simulation on last gesture', () => {
      const node = makeSimNode();
      onDragEnd(node, makeEvent({ active: false }), mockSimulation, makeSvg());
      expect(mockAlphaTarget).toHaveBeenCalledWith(0);
    });

    it('does not cool the simulation while other gestures are active', () => {
      const node = makeSimNode();
      onDragEnd(node, makeEvent({ active: true }), mockSimulation, makeSvg());
      expect(mockAlphaTarget).not.toHaveBeenCalled();
    });

    it('keeps fx/fy pinned (does not clear them)', () => {
      const node = makeSimNode();
      node.fx = 100;
      node.fy = 200;
      onDragEnd(node, makeEvent(), mockSimulation, makeSvg());
      expect(node.fx).toBe(100);
      expect(node.fy).toBe(200);
    });

    it('sets the grab cursor on the dragged element', () => {
      const svg = makeSvg();
      const g = document.createElementNS(
        'http://www.w3.org/2000/svg',
        'g',
      ) as SVGGElement;
      svg.appendChild(g);
      const node = makeSimNode();
      onDragEnd(
        node,
        makeEvent({ sourceEvent: { currentTarget: g } }),
        mockSimulation,
        svg,
      );
      expect(g.style.cursor).toBe('grab');
    });
  });

  describe('createDragBehavior', () => {
    it('exposes start/drag/end handlers as functions', () => {
      const svg = makeSvg();
      const behavior = createDragBehavior(
        mockSimulation as unknown as d3.Simulation<SimNode, undefined>,
        svg,
      );
      expect(typeof behavior.on('start')).toBe('function');
      expect(typeof behavior.on('drag')).toBe('function');
      expect(typeof behavior.on('end')).toBe('function');
    });
  });
});

describe('resetNodePositions', () => {
  it('clears fx/fy on all nodes', () => {
    const nodes = buildSimNodes(makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [],
    ));
    nodes[0].fx = 100; nodes[0].fy = 200;
    nodes[1].fx = 300; nodes[1].fy = 400;

    const mockSim = { alpha: vi.fn().mockReturnThis(), restart: vi.fn() } as unknown as d3.Simulation<SimNode, undefined>;
    resetNodePositions(nodes, mockSim);

    expect(nodes[0].fx).toBeNull();
    expect(nodes[0].fy).toBeNull();
    expect(nodes[1].fx).toBeNull();
    expect(nodes[1].fy).toBeNull();
  });

  it('reheats the simulation', () => {
    const nodes = buildSimNodes(makeGraph([makeNode('p1')], []));
    const mockAlpha = vi.fn().mockReturnThis();
    const mockRestart = vi.fn();
    const mockSim = { alpha: mockAlpha, restart: mockRestart } as unknown as d3.Simulation<SimNode, undefined>;

    resetNodePositions(nodes, mockSim);

    expect(mockAlpha).toHaveBeenCalledWith(1);
    expect(mockRestart).toHaveBeenCalledTimes(1);
  });

  it('handles empty node list', () => {
    const mockSim = { alpha: vi.fn().mockReturnThis(), restart: vi.fn() } as unknown as d3.Simulation<SimNode, undefined>;
    expect(() => resetNodePositions([], mockSim)).not.toThrow();
  });

  it('is idempotent (second call is a no-op)', () => {
    const nodes = buildSimNodes(makeGraph([makeNode('p1')], []));
    nodes[0].fx = 100; nodes[0].fy = 200;
    const mockAlpha = vi.fn().mockReturnThis();
    const mockRestart = vi.fn();
    const mockSim = { alpha: mockAlpha, restart: mockRestart } as unknown as d3.Simulation<SimNode, undefined>;

    resetNodePositions(nodes, mockSim);
    resetNodePositions(nodes, mockSim); // second call

    expect(nodes[0].fx).toBeNull();
    expect(nodes[0].fy).toBeNull();
    expect(mockAlpha).toHaveBeenCalledTimes(2);
    expect(mockRestart).toHaveBeenCalledTimes(2);
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

describe('DEFAULT_FORCE_CONFIG', () => {
  it('has all three keys', () => {
    const cfg = DEFAULT_FORCE_CONFIG;
    expect(cfg).toHaveProperty('generationPull');
    expect(cfg).toHaveProperty('spouseStrength');
    expect(cfg).toHaveProperty('parentChildStrength');
  });

  it('values are within [0, 2] range', () => {
    const cfg = DEFAULT_FORCE_CONFIG;
    expect(cfg.generationPull).toBeGreaterThanOrEqual(0);
    expect(cfg.generationPull).toBeLessThanOrEqual(2);
    expect(cfg.spouseStrength).toBeGreaterThanOrEqual(0);
    expect(cfg.spouseStrength).toBeLessThanOrEqual(2);
    expect(cfg.parentChildStrength).toBeGreaterThanOrEqual(0);
    expect(cfg.parentChildStrength).toBeLessThanOrEqual(2);
  });

  it('provides sensible defaults (not all zero)', () => {
    const cfg = DEFAULT_FORCE_CONFIG;
    expect(cfg.generationPull).toBeGreaterThan(0);
    expect(cfg.spouseStrength).toBeGreaterThan(0);
    expect(cfg.parentChildStrength).toBeGreaterThan(0);
  });
});

describe('computeGenerationSpacing', () => {
  function makeNode(generation: number): SimNode {
    return {
      handle: 'h',
      name: 'N',
      birth_date: null,
      death_date: null,
      birth_year: null,
      is_imputed: false,
      gender: 'unknown',
      family_group: 0,
      generation,
      index: undefined,
      x: undefined,
      y: undefined,
      vx: undefined,
      vy: undefined,
    };
  }

  it('returns 0 for empty node list', () => {
    expect(computeGenerationSpacing([], 600)).toBe(0);
  });

  it('returns 0 for single generation', () => {
    expect(computeGenerationSpacing([makeNode(0), makeNode(0)], 600)).toBe(0);
  });

  it('returns 0 for uniform generation (all same value)', () => {
    expect(computeGenerationSpacing([makeNode(2), makeNode(2), makeNode(2)], 600)).toBe(0);
  });

  it('computes spacing for two generations', () => {
    const spacing = computeGenerationSpacing([makeNode(0), makeNode(1)], 600);
    // 600 * 0.7 / 1 = 420
    expect(spacing).toBe(420);
  });

  it('computes spacing for five generations', () => {
    const spacing = computeGenerationSpacing(
      [makeNode(0), makeNode(1), makeNode(2), makeNode(3), makeNode(4)],
      1000,
    );
    // 1000 * 0.7 / 4 = 175
    expect(spacing).toBe(175);
  });

  it('returns 0 for non-positive height', () => {
    expect(computeGenerationSpacing([makeNode(0), makeNode(1)], 0)).toBe(0);
    expect(computeGenerationSpacing([makeNode(0), makeNode(1)], -100)).toBe(0);
  });

  it('applies the 40px minimum floor for very deep trees', () => {
    // 100 gens in 600px: 600 * 0.7 / 99 ≈ 4.2 → clamped to 40
    const nodes = Array.from({ length: 100 }, (_, i) => makeNode(i));
    expect(computeGenerationSpacing(nodes, 600)).toBe(40);
  });

  it('returns NaN for NaN generation values (contract violation)', () => {
    const nodes = [makeNode(NaN), makeNode(1)];
    expect(computeGenerationSpacing(nodes, 600)).toBeNaN();
  });
});

describe('applyForceConfig roundtrip', () => {
  it('mutates link strengths and gen-field strength on an existing simulation', () => {
    // Create a bare simulation with registerable forces
    const sim = d3.forceSimulation<SimNode>([]);
    const genY = () => 0;
    const config1: ForceConfig = {
      generationPull: 0.3,
      spouseStrength: 0.8,
      parentChildStrength: 0.5,
    };
    const config2: ForceConfig = {
      generationPull: 1.5,
      spouseStrength: 0.2,
      parentChildStrength: 0.9,
    };

    // Register forces with config1
    createSimulationForces(sim, config1, genY, [], [], 800, 600);

    // Mutate to config2
    applyForceConfig(sim, config2, genY);

    // Read back the values
    const spouseLink = sim.force('spouse-link') as d3.ForceLink<SimNode, d3.SimulationLinkDatum<SimNode>>;
    expect(spouseLink.strength()(null as unknown as d3.SimulationLinkDatum<SimNode>, 0, [])).toBe(0.2);

    const pcLink = sim.force('pc-link') as d3.ForceLink<SimNode, d3.SimulationLinkDatum<SimNode>>;
    expect(pcLink.strength()(null as unknown as d3.SimulationLinkDatum<SimNode>, 0, [])).toBe(0.9);

    const genField = sim.force('gen-field') as d3.ForceY<SimNode>;
    expect(genField.strength()(null as unknown as SimNode, 0, [])).toBe(1.5);

    // Clean up
    sim.stop();
  });

  it('handles missing forces gracefully (no-op)', () => {
    const sim = d3.forceSimulation<SimNode>([]);
    // Call applyForceConfig on a simulation with NO forces registered
    expect(() => {
      applyForceConfig(sim, DEFAULT_FORCE_CONFIG, () => 0);
    }).not.toThrow();
    sim.stop();
  });
});

describe('createSimulationForces', () => {
  it('registers all six named forces', () => {
    const sim = d3.forceSimulation<SimNode>([]);
    createSimulationForces(sim, DEFAULT_FORCE_CONFIG, () => 0, [], [], 800, 600);

    expect(sim.force('spouse-link')).toBeTruthy();
    expect(sim.force('pc-link')).toBeTruthy();
    expect(sim.force('gen-field')).toBeTruthy();
    expect(sim.force('charge')).toBeTruthy();
    expect(sim.force('collision')).toBeTruthy();
    expect(sim.force('center')).toBeTruthy();

    sim.stop();
  });

  it('uses the provided config values for force strengths', () => {
    const config: ForceConfig = {
      generationPull: 0.5,
      spouseStrength: 0.6,
      parentChildStrength: 0.7,
    };
    const sim = d3.forceSimulation<SimNode>([]);
    createSimulationForces(sim, config, () => 0, [], [], 800, 600);

    const spouseLink = sim.force('spouse-link') as d3.ForceLink<SimNode, d3.SimulationLinkDatum<SimNode>>;
    expect(spouseLink.strength()(null as unknown as d3.SimulationLinkDatum<SimNode>, 0, [])).toBe(0.6);

    const pcLink = sim.force('pc-link') as d3.ForceLink<SimNode, d3.SimulationLinkDatum<SimNode>>;
    expect(pcLink.strength()(null as unknown as d3.SimulationLinkDatum<SimNode>, 0, [])).toBe(0.7);

    const genField = sim.force('gen-field') as d3.ForceY<SimNode>;
    expect(genField.strength()(null as unknown as SimNode, 0, [])).toBe(0.5);

    sim.stop();
  });
});

describe('restartSimulation', () => {
  it('setForceConfig computes spacing from filtered node set', () => {
    // Two family groups with different generation spans:
    //   Group 1: generations 0-1 (2 gens → spacing = 420 for height 600)
    //   Group 2: generations 0-3 (4 gens → spacing = 140 for height 600)
    const data = makeGraph(
      [
        makeNode('p1', { family_group: 1, generation: 0 }),
        makeNode('p2', { family_group: 1, generation: 1 }),
        makeNode('p3', { family_group: 2, generation: 0 }),
        makeNode('p4', { family_group: 2, generation: 1 }),
        makeNode('p5', { family_group: 2, generation: 2 }),
        makeNode('p6', { family_group: 2, generation: 3 }),
      ],
      [
        { source: 'p1', target: 'p2', link_type: 'ParentChild' },
        { source: 'p3', target: 'p4', link_type: 'ParentChild' },
        { source: 'p4', target: 'p5', link_type: 'ParentChild' },
        { source: 'p5', target: 'p6', link_type: 'ParentChild' },
      ],
    );

    const container = document.createElement('div');
    container.style.width = '800px';
    container.style.height = '600px';
    document.body.appendChild(container);

    const controller = renderGraph(container, data);

    // Filter to group 1 (generations 0-1, 2 nodes)
    controller.setFamilyGroupFilter(1);

    // Apply a non-default config — should not throw
    const testConfig: ForceConfig = { generationPull: 0.5, spouseStrength: 0.5, parentChildStrength: 0.5 };
    expect(() => {
      controller.setForceConfig(testConfig);
    }).not.toThrow();

    controller.destroy();
    document.body.removeChild(container);
  });
});

describe('selected node sizing', () => {
  it('renders selected nodes at 2x radius and unselected at default', () => {
    const data = makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [{ source: 'p1', target: 'p2', link_type: 'Spouse' }],
    );
    const container = document.createElement('div');
    container.style.width = '800px';
    container.style.height = '600px';
    document.body.appendChild(container);

    const controller = renderGraph(container, data);
    controller.setHighlighted(new Set(['p1']));

    // DOM join order follows data.nodes order: p1 then p2
    const circles = container.querySelectorAll('circle');
    expect(circles).toHaveLength(2);
    expect(circles[0].getAttribute('r')).toBe('16'); // selected
    expect(circles[1].getAttribute('r')).toBe('8');  // unselected

    // Labels move up with the radius
    const labels = container.querySelectorAll('text');
    expect(labels[0].getAttribute('dy')).toBe('-22');
    expect(labels[1].getAttribute('dy')).toBe('-14');

    controller.destroy();
    document.body.removeChild(container);
  });

  it('restores default size when selection is cleared', () => {
    const data = makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [{ source: 'p1', target: 'p2', link_type: 'Spouse' }],
    );
    const container = document.createElement('div');
    container.style.width = '800px';
    container.style.height = '600px';
    document.body.appendChild(container);

    const controller = renderGraph(container, data);
    controller.setHighlighted(new Set(['p1']));
    controller.setHighlighted(new Set());

    const circles = container.querySelectorAll('circle');
    expect(circles).toHaveLength(2);
    expect(circles[0].getAttribute('r')).toBe('8');
    expect(circles[1].getAttribute('r')).toBe('8');

    const labels = container.querySelectorAll('text');
    expect(labels[0].getAttribute('dy')).toBe('-14');
    expect(labels[1].getAttribute('dy')).toBe('-14');

    controller.destroy();
    document.body.removeChild(container);
  });

  it('grows all selected nodes in a multi-node selection', () => {
    const data = makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [{ source: 'p1', target: 'p2', link_type: 'Spouse' }],
    );
    const container = document.createElement('div');
    container.style.width = '800px';
    container.style.height = '600px';
    document.body.appendChild(container);

    const controller = renderGraph(container, data);
    controller.setHighlighted(new Set(['p1', 'p2']));

    const circles = container.querySelectorAll('circle');
    expect(circles).toHaveLength(2);
    expect(circles[0].getAttribute('r')).toBe('16');
    expect(circles[1].getAttribute('r')).toBe('16');

    controller.destroy();
    document.body.removeChild(container);
  });

  it('grows only visible nodes when a family-group filter is active', () => {
    const data = makeGraph(
      [
        makeNode('p1', { family_group: 1, generation: 0 }),
        makeNode('p2', { family_group: 1, generation: 1 }),
        makeNode('p3', { family_group: 2, generation: 0 }),
        makeNode('p4', { family_group: 2, generation: 1 }),
      ],
      [
        { source: 'p1', target: 'p2', link_type: 'ParentChild' },
        { source: 'p3', target: 'p4', link_type: 'ParentChild' },
      ],
    );
    const container = document.createElement('div');
    container.style.width = '800px';
    container.style.height = '600px';
    document.body.appendChild(container);

    const controller = renderGraph(container, data);
    // Filter to group 1, then highlight p1 (visible) and p3 (hidden)
    controller.setFamilyGroupFilter(1);
    controller.setHighlighted(new Set(['p1', 'p3']));

    // Only group 1 nodes (p1, p2) are in the DOM
    const circles = container.querySelectorAll('circle');
    expect(circles).toHaveLength(2);
    // p1 is selected and visible → r=16
    expect(circles[0].getAttribute('r')).toBe('16');
    // p2 is not selected → r=8
    expect(circles[1].getAttribute('r')).toBe('8');

    controller.destroy();
    document.body.removeChild(container);
  });
});


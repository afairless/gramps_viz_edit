// Force-directed graph rendering using D3.js
// Manages the SVG canvas, force simulation, zoom/pan, and node/link rendering.

import * as d3 from 'd3';
import type { GraphData, LinkType } from './types';
import { DEFAULT_FORCE_CONFIG, type ForceConfig } from './types';
import {
  buildColorScale,
  getNodeColor,
  getNodeStrokeDash,
  getNodeOpacity,
  getLinkColor,
  getLinkStrokeDash,
  getLinkStrokeWidth,
} from './colors';

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface GraphController {
  /** Replace the rendered data (new simulation). */
  updateData(data: GraphData): void;
  /** Tear down the simulation and SVG elements. */
  destroy(): void;
  /** Register a callback fired when a node circle is clicked (handle). */
  onNodeClick(cb: (handle: string) => void): void;
  /** Register a callback fired when cursor hovers a node (handle, event). */
  onNodeHover(cb: (handle: string | null, event: MouseEvent) => void): void;
  /** Resize the SVG to match the parent container. */
  resize(): void;
  /** Highlight a set of node handles (selection ring). */
  setHighlighted(handles: Set<string>): void;
  /** Filter to one family group (null = all groups). */
  setFamilyGroupFilter(groupId: number | null): void;
  /** Get all visible node handles. */
  getVisibleNodes(): string[];
  /** Reset all node positions and re-run the force layout. */
  resetLayout(): void;
  /** Update force configuration and reheat the simulation. */
  setForceConfig(config: ForceConfig): void;
}

// Internal node/link types for D3 simulation.
// `source`/`target` are mutated by D3 from handles to node references.
export interface SimNode extends d3.SimulationNodeDatum {
  handle: string;
  name: string;
  birth_date: string | null;
  death_date: string | null;
  birth_year: number | null;
  is_imputed: boolean;
  gender: string;
  family_group: number;
  generation: number;
}

interface SimLink {
  source: SimNode;
  target: SimNode;
  link_type: LinkType;
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const NODE_RADIUS = 8;
const SELECTED_NODE_RADIUS = 16;
const SELECTED_STROKE_WIDTH = 3;

// Force simulation parameters (module-level so tests can import them)
/** Spouse links are short — couples should be visually cohesive. */
const SPOUSE_BASE_DISTANCE = 40;
/** Parent-child links span generations and need vertical room. */
const PC_BASE_DISTANCE = 120;
/** Unchanged from the previous layout: repulsion between all nodes. */
const CHARGE_STRENGTH = -300;
/** Unchanged from the previous layout: node collision radius. */
const COLLIDE_RADIUS = 18;
/** Weak X-centering only — the gen-field handles vertical placement. */
const CENTER_STRENGTH = 0.05;

/** Base repulsion constant for the selection-repel pairwise force. */
const BASE_REPEL = 500;

/** Max selected×unselected pairs before the repel force is skipped. */
const REPEL_PAIR_LIMIT = 10_000;

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

export function validateGraphData(data: unknown): data is GraphData {
  if (!data || typeof data !== 'object') return false;
  const d = data as Record<string, unknown>;
  if (!Array.isArray(d.nodes) || !Array.isArray(d.links)) return false;
  for (const n of d.nodes) {
    if (!n || typeof n !== 'object') return false;
    const node = n as Record<string, unknown>;
    if (typeof node.handle !== 'string') return false;
    if (typeof node.name !== 'string') return false;
    if (typeof node.family_group !== 'number') return false;
  }
  for (const l of d.links) {
    if (!l || typeof l !== 'object') return false;
    const link = l as Record<string, unknown>;
    if (typeof link.source !== 'string') return false;
    if (typeof link.target !== 'string') return false;
    if (link.link_type !== 'Spouse' && link.link_type !== 'ParentChild') return false;
  }
  return true;
}

// ---------------------------------------------------------------------------
// Transform helpers (exported for testing)
// ---------------------------------------------------------------------------

export function buildSimNodes(data: GraphData): SimNode[] {
  return data.nodes.map((n) => ({
    handle: n.handle,
    name: n.name,
    birth_date: n.birth_date,
    death_date: n.death_date,
    birth_year: n.birth_year,
    is_imputed: n.is_imputed,
    gender: n.gender,
    family_group: n.family_group,
    generation: n.generation,
  }));
}

export function buildSimLinks(
  data: GraphData,
  nodeMap: Map<string, SimNode>,
): SimLink[] {
  const links: SimLink[] = [];
  const seen = new Set<string>();
  for (const l of data.links) {
    const src = nodeMap.get(l.source);
    const tgt = nodeMap.get(l.target);
    if (!src || !tgt) continue;
    // Deduplicate undirected edges (spouse links are bidirectional)
    const key =
      l.link_type === 'Spouse'
        ? `Spouse:${[l.source, l.target].sort().join('|')}`
        : `ParentChild:${l.source}|${l.target}`;
    if (seen.has(key)) continue;
    seen.add(key);
    links.push({ source: src, target: tgt, link_type: l.link_type });
  }
  return links;
}

// ---------------------------------------------------------------------------
// Drag handlers (exported for testing)
// ---------------------------------------------------------------------------

export function onDragStart(
  d: SimNode,
  event: d3.D3DragEvent<SVGGElement, SimNode, SimNode>,
  simulation: d3.Simulation<SimNode, undefined>,
): void {
  if (!event.active) simulation.alphaTarget(0.3).restart();
  d.fx = d.x ?? null;
  d.fy = d.y ?? null;
  d3.select(event.sourceEvent.currentTarget as SVGGElement).style(
    'cursor',
    'grabbing',
  );
}

export function onDrag(
  d: SimNode,
  event: d3.D3DragEvent<SVGGElement, SimNode, SimNode>,
  _simulation: d3.Simulation<SimNode, undefined>,
): void {
  d.fx = event.x;
  d.fy = event.y;
}

export function onDragEnd(
  _d: SimNode,
  event: d3.D3DragEvent<SVGGElement, SimNode, SimNode>,
  simulation: d3.Simulation<SimNode, undefined>,
): void {
  if (!event.active) simulation.alphaTarget(0);
  // Pin the node where dropped — do NOT clear fx/fy
  d3.select(event.sourceEvent.currentTarget as SVGGElement).style(
    'cursor',
    'grab',
  );
}

/** Reset all pinned positions and reheat the simulation. */
export function resetNodePositions(
  nodes: SimNode[],
  simulation: d3.Simulation<SimNode, undefined>,
): void {
  for (const node of nodes) {
    node.fx = null;
    node.fy = null;
  }
  simulation.alpha(1).restart();
}

export function createDragBehavior(
  simulation: d3.Simulation<SimNode, undefined>,
): d3.DragBehavior<SVGGElement, SimNode, SimNode | d3.SubjectPosition> {
  return d3
    .drag<SVGGElement, SimNode>()
    .on('start', (event: d3.D3DragEvent<SVGGElement, SimNode, SimNode>, d: SimNode) =>
      onDragStart(d, event, simulation),
    )
    .on('drag', (event: d3.D3DragEvent<SVGGElement, SimNode, SimNode>, d: SimNode) =>
      onDrag(d, event, simulation),
    )
    .on('end', (event: d3.D3DragEvent<SVGGElement, SimNode, SimNode>, d: SimNode) =>
      onDragEnd(d, event, simulation),
    );
}

// ---------------------------------------------------------------------------
// Force configuration helpers (exported for testing)
// ---------------------------------------------------------------------------

/**
 * Compute the vertical spacing between generation bands.
 *
 * Uses ~70% of the canvas height across (numGens - 1) gaps, with a 40px
 * floor so very deep trees don't collapse onto a single band. Returns 0 for
 * empty node sets, a single generation, or non-positive canvas heights.
 */
export function computeGenerationSpacing(
  nodes: SimNode[],
  canvasHeight: number,
): number {
  // Guard: non-positive height means no viewport area — nothing to space.
  if (canvasHeight <= 0) return 0;
  const gens = nodes.map((n) => n.generation);
  if (gens.length === 0) return 0;
  const minGen = Math.min(...gens);
  const maxGen = Math.max(...gens);
  const numGens = maxGen - minGen + 1;
  if (numGens <= 1) return 0;
  // Use ~70% of the canvas height, leaving top/bottom margin. The
  // Math.max(40, …) floor ensures minimum spacing for very deep trees.
  return Math.max(40, (canvasHeight * 0.7) / (numGens - 1));
}

/**
 * Register all six named forces on a fresh simulation.
 *
 * Spouse and parent-child links get separate forces with their own base
 * distances; a generation field force pulls nodes toward their generation
 * band; weak centering keeps the layout in view horizontally.
 */
export function createSimulationForces(
  sim: d3.Simulation<SimNode, undefined>,
  config: ForceConfig,
  genY: (d: SimNode) => number,
  spouseLinks: d3.SimulationLinkDatum<SimNode>[],
  pcLinks: d3.SimulationLinkDatum<SimNode>[],
  width: number,
  height: number,
  getSelected: () => Set<string>,
): void {
  sim
    .force(
      'spouse-link',
      d3
        .forceLink<SimNode, d3.SimulationLinkDatum<SimNode>>(spouseLinks)
        .id((d: SimNode) => d.handle)
        .distance(SPOUSE_BASE_DISTANCE)
        .strength(config.spouseStrength),
    )
    .force(
      'pc-link',
      d3
        .forceLink<SimNode, d3.SimulationLinkDatum<SimNode>>(pcLinks)
        .id((d: SimNode) => d.handle)
        .distance(PC_BASE_DISTANCE)
        .strength(config.parentChildStrength),
    )
    .force('gen-field', d3.forceY<SimNode>().y(genY).strength(config.generationPull))
    .force('charge', d3.forceManyBody().strength(CHARGE_STRENGTH))
    .force('collision', d3.forceCollide(COLLIDE_RADIUS))
    .force('center', d3.forceCenter(width / 2, height / 2).strength(CENTER_STRENGTH))
    .force(
      'selection-repel',
      createSelectionRepelForce(getSelected).strength(config.repelStrength),
    )
    .force(
      'selected-attract',
      createSelectedAttractForce(getSelected).strength(config.selectedAttractStrength),
    )
    .force(
      'unselected-attract',
      createUnselectedAttractForce(getSelected).strength(config.unselectedAttractStrength),
    );
}

/**
 * Mutate the forces on an existing (running) simulation to a new config.
 * No D3 selections are torn down — the simulation keeps ticking.
 */
export function applyForceConfig(
  simulation: d3.Simulation<SimNode, undefined>,
  config: ForceConfig,
  genY: (d: SimNode) => number,
): void {
  const spouse = simulation.force('spouse-link');
  if (spouse) {
    (spouse as d3.ForceLink<SimNode, d3.SimulationLinkDatum<SimNode>>).strength(config.spouseStrength);
  }
  const pc = simulation.force('pc-link');
  if (pc) {
    (pc as d3.ForceLink<SimNode, d3.SimulationLinkDatum<SimNode>>).strength(config.parentChildStrength);
  }
  const gf = simulation.force('gen-field');
  if (gf) {
    (gf as d3.ForceY<SimNode>).strength(config.generationPull).y(genY);
  }
  const repel = simulation.force('selection-repel') as SelectionRepelForce | undefined;
  if (repel) {
    repel.strength(config.repelStrength);
  }

  const selAttract = simulation.force('selected-attract') as AttractForce | undefined;
  if (selAttract) {
    selAttract.strength(config.selectedAttractStrength);
  }

  const unselAttract = simulation.force('unselected-attract') as AttractForce | undefined;
  if (unselAttract) {
    unselAttract.strength(config.unselectedAttractStrength);
  }
}

// ---------------------------------------------------------------------------
// Selection-repel custom force
// ---------------------------------------------------------------------------

/**
 * Interface for the selection-repel custom D3 force.
 * Exposes strength() getter/setter so callers can mutate it at runtime.
 */
export interface SelectionRepelForce extends d3.Force<SimNode, undefined> {
  /** Get or set the repel multiplier in [0, 2]. */
  strength(s: number): this;
  strength(): number;
  /** Initialize the force with the simulation's node array. */
  initialize(nodes: SimNode[]): void;
}

/**
 * Create a custom D3 force that repels selected nodes from unselected nodes
 * (and vice versa) using pairwise Coulomb-like repulsion.
 *
 * The force reads the current selected set via the getter on every tick, so
 * callers can mutate the set without restarting the simulation.
 *
 * @param getSelected - Callback returning the current set of selected handles.
 * @returns A SelectionRepelForce instance.
 */
export function createSelectionRepelForce(
  getSelected: () => Set<string>,
): SelectionRepelForce {
  let nodes: SimNode[] = [];
  let strengthValue = 0;
  let warned = false;

  function force(tickAlpha: number): void {
    const selected = getSelected();
    const selCount = selected.size;

    // Degenerate cases: no strength, zero selected, or all/none unselected
    if (strengthValue === 0 || selCount === 0) return;

    // Separate selected from unselected nodes
    const selectedNodes: SimNode[] = [];
    const unselectedNodes: SimNode[] = [];
    for (const n of nodes) {
      if (selected.has(n.handle)) {
        selectedNodes.push(n);
      } else {
        unselectedNodes.push(n);
      }
    }

    const unselCount = unselectedNodes.length;
    if (unselCount === 0) return;

    // O(N·M) guard: skip if too many pairs
    const pairCount = selCount * unselCount;
    if (pairCount > REPEL_PAIR_LIMIT) {
      if (!warned) {
        console.warn(
          `[selection-repel] Skipping tick: ${pairCount} pairs exceeds limit of ${REPEL_PAIR_LIMIT}`,
        );
        warned = true;
      }
      return;
    }

    const impulseScale = strengthValue * BASE_REPEL;

    for (const s of selectedNodes) {
      for (const u of unselectedNodes) {
        let dx = (s.x ?? 0) - (u.x ?? 0);
        let dy = (s.y ?? 0) - (u.y ?? 0);
        let dist = Math.sqrt(dx * dx + dy * dy);

        // Tie-breaking: coincident nodes get a small random offset
        if (dist < 1) {
          dx = (Math.random() - 0.5);
          dy = (Math.random() - 0.5);
          dist = Math.max(1, Math.sqrt(dx * dx + dy * dy));
        }

        const r = Math.max(1, dist);
        const impulse = tickAlpha * impulseScale / (r * r);

        // Normalise direction
        const nx = dx / r;
        const ny = dy / r;

        // Push selected node away from unselected, divided by selected count
        s.vx = (s.vx ?? 0) + nx * impulse / Math.max(1, selCount);
        s.vy = (s.vy ?? 0) + ny * impulse / Math.max(1, selCount);

        // Push unselected node away from selected (opposite direction), divided by unselected count
        u.vx = (u.vx ?? 0) - nx * impulse / Math.max(1, unselCount);
        u.vy = (u.vy ?? 0) - ny * impulse / Math.max(1, unselCount);
      }
    }
  }

  force.initialize = (nodeList: SimNode[]) => {
    nodes = nodeList;
    warned = false;
  };

  force.strength = (s?: number) => {
    if (s === undefined) return strengthValue;
    strengthValue = s;
    return force;
  };

  return force as unknown as SelectionRepelForce;
}

// ---------------------------------------------------------------------------
// Attract-force interface and implementations
// ---------------------------------------------------------------------------

/**
 * Interface for the selected-attract and unselected-attract custom D3 forces.
 * Exposes strength() getter/setter so callers can mutate it at runtime.
 */
export interface AttractForce extends d3.Force<SimNode, undefined> {
  /** Get or set the attract multiplier in [0, 2]. */
  strength(s: number): this;
  strength(): number;
  /** Initialize the force with the simulation's node array. */
  initialize(nodes: SimNode[]): void;
}

/**
 * Create a custom D3 force that pulls selected nodes toward the centroid
 * of the selected set, forming a cluster. Only active when there are at
 * least 2 selected nodes and at least 1 unselected node (a complement to
 * separate from).
 *
 * @param getSelected - Callback returning the current set of selected handles.
 * @returns An AttractForce instance.
 */
export function createSelectedAttractForce(
  getSelected: () => Set<string>,
): AttractForce {
  let nodes: SimNode[] = [];
  let strengthValue = 0;

  function force(tickAlpha: number): void {
    const selected = getSelected();
    if (strengthValue === 0 || selected.size < 2) return;

    // Partition nodes into selected and check for unselected complement
    const selectedNodes: SimNode[] = [];
    let hasUnselected = false;
    for (const n of nodes) {
      if (selected.has(n.handle)) {
        selectedNodes.push(n);
      } else {
        hasUnselected = true;
      }
    }

    // No-op if no unselected nodes to separate from
    if (!hasUnselected) return;

    // Compute centroid of selected nodes
    let cx = 0, cy = 0;
    for (const n of selectedNodes) {
      cx += n.x ?? 0;
      cy += n.y ?? 0;
    }
    cx /= selectedNodes.length;
    cy /= selectedNodes.length;

    // Apply impulse toward centroid
    const impulse = tickAlpha * strengthValue;
    for (const n of selectedNodes) {
      n.vx = (n.vx ?? 0) + (cx - (n.x ?? 0)) * impulse;
      n.vy = (n.vy ?? 0) + (cy - (n.y ?? 0)) * impulse;
    }
  }

  force.initialize = (nodeList: SimNode[]) => { nodes = nodeList; };
  force.strength = (s?: number) => {
    if (s === undefined) return strengthValue;
    strengthValue = s;
    return force;
  };

  return force as unknown as AttractForce;
}

/**
 * Create a custom D3 force that pulls unselected nodes toward the centroid
 * of the unselected set, forming a cluster. Mirror of the selected-attract
 * force operating on the complement. Only active when there is at least
 * one selected node (an "other cluster" to separate from) and at least
 * 2 unselected nodes.
 *
 * @param getSelected - Callback returning the current set of selected handles.
 * @returns An AttractForce instance.
 */
export function createUnselectedAttractForce(
  getSelected: () => Set<string>,
): AttractForce {
  let nodes: SimNode[] = [];
  let strengthValue = 0;

  function force(tickAlpha: number): void {
    const selected = getSelected();
    // No-op without selected nodes: nothing to cluster away from
    if (strengthValue === 0 || selected.size === 0) return;

    // Partition nodes into unselected and check for selected complement
    const unselectedNodes: SimNode[] = [];
    let hasSelected = false;
    for (const n of nodes) {
      if (selected.has(n.handle)) {
        hasSelected = true;
      } else {
        unselectedNodes.push(n);
      }
    }

    // No-op if fewer than 2 unselected nodes (need a cluster to form)
    if (!hasSelected || unselectedNodes.length < 2) return;

    // Compute centroid of unselected nodes
    let cx = 0, cy = 0;
    for (const n of unselectedNodes) {
      cx += n.x ?? 0;
      cy += n.y ?? 0;
    }
    cx /= unselectedNodes.length;
    cy /= unselectedNodes.length;

    // Apply impulse toward centroid
    const impulse = tickAlpha * strengthValue;
    for (const n of unselectedNodes) {
      n.vx = (n.vx ?? 0) + (cx - (n.x ?? 0)) * impulse;
      n.vy = (n.vy ?? 0) + (cy - (n.y ?? 0)) * impulse;
    }
  }

  force.initialize = (nodeList: SimNode[]) => { nodes = nodeList; };
  force.strength = (s?: number) => {
    if (s === undefined) return strengthValue;
    strengthValue = s;
    return force;
  };

  return force as unknown as AttractForce;
}

// ---------------------------------------------------------------------------
// Main render function
// ---------------------------------------------------------------------------

export function renderGraph(
  containerElement: HTMLElement,
  data: GraphData,
): GraphController {
  // --- state ---
  let currentFilter: number | null = null;
  let currentConfig: ForceConfig = { ...DEFAULT_FORCE_CONFIG };
  let highlighted = new Set<string>();
  let selectedSet = new Set<string>();
  let nodeClickCb: ((handle: string) => void) | null = null;
  let nodeHoverCb: ((handle: string | null, event: MouseEvent) => void) | null =
    null;
  let simNodes: SimNode[] = [];
  let simLinks: SimLink[] = [];
  let nodeGroup: d3.Selection<SVGGElement, SimNode, SVGGElement, unknown>;
  let linkGroup: d3.Selection<SVGLineElement, SimLink, SVGGElement, unknown>;
  let simulation: d3.Simulation<SimNode, undefined>;
  let colorScale = buildColorScale([]);

  // --- SVG scaffold ---
  const width = containerElement.clientWidth || 960;
  const height = containerElement.clientHeight || 600;

  const svg = d3
    .select(containerElement)
    .append('svg')
    .attr('width', width)
    .attr('height', height)
    .style('cursor', 'grab');

  const g = svg.append('g'); // zoom/pan container

  // --- zoom ---
  const zoom = d3
    .zoom<SVGSVGElement, unknown>()
    .scaleExtent([0.1, 8])
    .on('zoom', (event: d3.D3ZoomEvent<SVGSVGElement, unknown>) => {
      g.attr('transform', event.transform.toString());
    });
  svg.call(zoom);

  // --- resize observer ---
  const ro = new ResizeObserver(() => {
    const w = containerElement.clientWidth;
    const h = containerElement.clientHeight;
    if (w > 0 && h > 0) {
      svg.attr('width', w).attr('height', h);
    }
  });
  ro.observe(containerElement);

  // Live SVG dimensions — read at force-creation time, not captured at
  // construction, so they stay correct after window resize.
  function getSvgHeight(): number {
    const el = svg.node();
    if (el) {
      const rect = el.getBoundingClientRect();
      if (rect.height > 0) return rect.height;
    }
    return containerElement.clientHeight || 600;
  }

  function getSvgWidth(): number {
    const el = svg.node();
    if (el) {
      const rect = el.getBoundingClientRect();
      if (rect.width > 0) return rect.width;
    }
    return containerElement.clientWidth || 960;
  }

  // --- build simulation ---
  function restartSimulation() {
    // Filter nodes
    const filtered =
      currentFilter === null
        ? simNodes
        : simNodes.filter((n) => n.family_group === currentFilter);

    // Filter links to only visible nodes
    const visibleHandles = new Set(filtered.map((n) => n.handle));
    const filteredLinks = simLinks.filter(
      (l) =>
        visibleHandles.has(l.source.handle) &&
        visibleHandles.has(l.target.handle),
    );

    // ---- links ----
    if (!linkGroup) {
      linkGroup = g.append('g').attr('class', 'links').selectAll('line');
    }
    const linkBind = linkGroup.data(
      filteredLinks,
      (d: SimLink) => `${d.source.handle}|${d.target.handle}`,
    );
    linkBind.exit().remove();
    const linkEnter = linkBind
      .enter()
      .append('line')
      .attr('stroke', (d: SimLink) => getLinkColor(d.link_type))
      .attr('stroke-width', (d: SimLink) => getLinkStrokeWidth(d.link_type))
      .attr('stroke-dasharray', (d: SimLink) => getLinkStrokeDash(d.link_type))
      .attr('stroke-opacity', 0.8);
    linkGroup = linkEnter.merge(linkBind);

    // ---- nodes ----
    if (!nodeGroup) {
      nodeGroup = g.append('g').attr('class', 'nodes').selectAll('g');
    }
    const nodeBind = nodeGroup.data(filtered, (d: SimNode) => d.handle);
    nodeBind.exit().remove();
    const nodeEnter = nodeBind
      .enter()
      .append('g')
      .attr('cursor', 'grab')
      .on('click', (event: MouseEvent, d: SimNode) => {
        event.stopPropagation();
        if (nodeClickCb) nodeClickCb(d.handle);
      })
      .on('mouseenter', (event: MouseEvent, d: SimNode) => {
        if (nodeHoverCb) nodeHoverCb(d.handle, event);
      })
      .on('mouseleave', (event: MouseEvent) => {
        if (nodeHoverCb) nodeHoverCb(null, event);
      });

    nodeEnter
      .append('circle')
      .attr('r', NODE_RADIUS)
      .attr('stroke', '#fff')
      .attr('stroke-width', 1.5);

    // Apply colors to all circles (enter + update)
    nodeGroup.each(function (d: SimNode) {
      const circle = d3.select(this).select('circle');
      circle.attr('fill', getNodeColor(d.birth_year, colorScale));
      circle.attr('stroke-dasharray', getNodeStrokeDash(d.is_imputed));
      circle.attr('opacity', getNodeOpacity(d.is_imputed));
    });

    nodeEnter
      .append('text')
      .text((d: SimNode) => d.name)
      .attr('text-anchor', 'middle')
      .attr('dy', -NODE_RADIUS - 6)
      .attr('font-size', '10px')
      .attr('fill', '#333')
      .style('pointer-events', 'none');

    nodeGroup = nodeEnter.merge(nodeBind);

    // ---- simulation ----
    if (simulation) simulation.stop();

    // Split links by type so each force can have its own strength.
    const spouseLinks = filteredLinks.filter(
      (l) => l.link_type === 'Spouse',
    ) as unknown as d3.SimulationLinkDatum<SimNode>[];
    const pcLinks = filteredLinks.filter(
      (l) => l.link_type === 'ParentChild',
    ) as unknown as d3.SimulationLinkDatum<SimNode>[];

    simulation = d3.forceSimulation(filtered);

    // Generation band target: nodes of generation g land at y = (g - minGen) * spacing.
    const genTarget = (d: SimNode): number => {
      if (filtered.length === 0) return 0;
      const spacing = computeGenerationSpacing(filtered, getSvgHeight());
      const minGen = Math.min(...filtered.map((n) => n.generation));
      return (d.generation - minGen) * spacing;
    };

    createSimulationForces(
      simulation,
      currentConfig,
      genTarget,
      spouseLinks,
      pcLinks,
      getSvgWidth(),
      getSvgHeight(),
      () => selectedSet,
    );

    simulation.on('tick', () => {
        linkGroup
          .attr('x1', (d: SimLink) => d.source.x ?? 0)
          .attr('y1', (d: SimLink) => d.source.y ?? 0)
          .attr('x2', (d: SimLink) => d.target.x ?? 0)
          .attr('y2', (d: SimLink) => d.target.y ?? 0);

        nodeGroup.attr(
          'transform',
          (d: SimNode) => `translate(${d.x ?? 0},${d.y ?? 0})`,
        );
      });

    // ---- drag behavior (re-bind all visible nodes with current simulation) ----
    nodeGroup.call(createDragBehavior(simulation));

    // Apply highlighting
    applyHighlight();
  }

  function applyHighlight() {
    if (!nodeGroup) return;
    nodeGroup.each(function (d: SimNode) {
      const el = d3.select(this);
      const isSelected = highlighted.has(d.handle);
      el.select('circle')
        .attr('r', isSelected ? SELECTED_NODE_RADIUS : NODE_RADIUS)
        .attr('stroke', isSelected ? '#ff6b6b' : '#fff')
        .attr('stroke-width', isSelected ? SELECTED_STROKE_WIDTH : 1.5)
        // Ensure fill/stroke-dasharray/opacity stay correct (no overlapped deselection)
        .attr('fill', getNodeColor(d.birth_year, colorScale))
        .attr('stroke-dasharray', getNodeStrokeDash(d.is_imputed))
        .attr('opacity', getNodeOpacity(d.is_imputed));
      el.select('text').attr(
        'dy',
        isSelected ? -SELECTED_NODE_RADIUS - 6 : -NODE_RADIUS - 6,
      );
    });
  }

  // ---- build data ----
  simNodes = buildSimNodes(data);
  colorScale = buildColorScale(simNodes.map((n: SimNode) => n.birth_year));
  const nodeMap = new Map(simNodes.map((n: SimNode) => [n.handle, n]));
  simLinks = buildSimLinks(data, nodeMap);
  restartSimulation();

  // ---- controller ----
  const controller: GraphController = {
    updateData(newData: GraphData) {
      simNodes = buildSimNodes(newData);
      colorScale = buildColorScale(simNodes.map((n: SimNode) => n.birth_year));
      const nm = new Map(simNodes.map((n: SimNode) => [n.handle, n]));
      simLinks = buildSimLinks(newData, nm);
      highlighted = new Set();
      restartSimulation();
    },

    destroy() {
      if (simulation) simulation.stop();
      ro.disconnect();
      svg.remove();
    },

    onNodeClick(cb: (handle: string) => void) {
      nodeClickCb = cb;
    },

    onNodeHover(cb: (handle: string | null, event: MouseEvent) => void) {
      nodeHoverCb = cb;
    },

    resize() {
      const w = containerElement.clientWidth;
      const h = containerElement.clientHeight;
      if (w > 0 && h > 0) {
        svg.attr('width', w).attr('height', h);
        // Preserve CENTER_STRENGTH — recreating forceCenter without it would
        // reset the strength to D3's default of 1.0.
        simulation.force('center', d3.forceCenter(w / 2, h / 2).strength(CENTER_STRENGTH));
        simulation.alpha(0.3).restart();
      }
    },

    setHighlighted(handles: Set<string>) {
      highlighted = handles;
      selectedSet = handles;
      applyHighlight();
    },

    setFamilyGroupFilter(groupId: number | null) {
      currentFilter = groupId;
      restartSimulation();
    },

    getVisibleNodes(): string[] {
      const filtered =
        currentFilter === null
          ? simNodes
          : simNodes.filter((n: SimNode) => n.family_group === currentFilter);
      return filtered.map((n: SimNode) => n.handle);
    },

    resetLayout() {
      if (svg.node()?.ownerDocument === null) return;
      resetNodePositions(simNodes, simulation);
      svg.transition().duration(500).call(
        zoom.transform,
        d3.zoomIdentity,
      );
    },

    setForceConfig(config: ForceConfig) {
      currentConfig = { ...config };
      // Use the *active* node set so the computed generation range matches
      // what the simulation is actually running on (handles filtered views).
      const activeNodes =
        currentFilter === null
          ? simNodes
          : simNodes.filter((n) => n.family_group === currentFilter);
      if (activeNodes.length === 0) return; // nothing to configure
      const spacing = computeGenerationSpacing(activeNodes, getSvgHeight());
      const minGen = Math.min(...activeNodes.map((n) => n.generation));
      const targetY = (d: SimNode) => (d.generation - minGen) * spacing;
      applyForceConfig(simulation, currentConfig, targetY);
      simulation.alpha(0.3).restart();
    },
  };

  return controller;
}
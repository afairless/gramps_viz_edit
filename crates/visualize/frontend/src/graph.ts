// Force-directed graph rendering using D3.js
// Manages the SVG canvas, force simulation, zoom/pan, and node/link rendering.

import * as d3 from 'd3';
import type { GraphData } from './types';
import {
  buildColorScale,
  getNodeColor,
  getNodeStrokeDash,
  getNodeOpacity,
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
}

// Internal node/link types for D3 simulation.
// `source`/`target` are mutated by D3 from handles to node references.
interface SimNode extends d3.SimulationNodeDatum {
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
  link_type: 'Spouse' | 'ParentChild';
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const NODE_RADIUS = 8;
const LINK_STROKE_WIDTH = 1.5;
const SELECTED_STROKE_WIDTH = 3;

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
// Main render function
// ---------------------------------------------------------------------------

export function renderGraph(
  containerElement: HTMLElement,
  data: GraphData,
): GraphController {
  // --- state ---
  let currentFilter: number | null = null;
  let highlighted = new Set<string>();
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
      .attr('stroke', '#999')
      .attr('stroke-width', LINK_STROKE_WIDTH)
      .attr('stroke-opacity', 0.6);
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
      .attr('cursor', 'pointer')
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

    // Cast links to the D3 link datum type
    const d3Links = filteredLinks as unknown as d3.SimulationLinkDatum<SimNode>[];

    simulation = d3
      .forceSimulation(filtered)
      .force(
        'link',
        d3
          .forceLink<SimNode, d3.SimulationLinkDatum<SimNode>>(d3Links)
          .id((d: SimNode) => d.handle)
          .distance(80),
      )
      .force('charge', d3.forceManyBody().strength(-300))
      .force('collision', d3.forceCollide(18))
      .force(
        'center',
        d3.forceCenter(width / 2, height / 2).strength(0.3),
      )
      .on('tick', () => {
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

    // Apply highlighting
    applyHighlight();
  }

  function applyHighlight() {
    if (!nodeGroup) return;
    nodeGroup.each(function (d: SimNode) {
      const el = d3.select(this);
      const isSelected = highlighted.has(d.handle);
      el.select('circle').attr(
        'stroke',
        isSelected ? '#ff6b6b' : '#fff',
      );
      el.select('circle').attr(
        'stroke-width',
        isSelected ? SELECTED_STROKE_WIDTH : 1.5,
      );
      // Ensure fill/stroke-dasharray/opacity stay correct (no overlapped deselection)
      el.select('circle').attr('fill', getNodeColor(d.birth_year, colorScale));
      el.select('circle').attr('stroke-dasharray', getNodeStrokeDash(d.is_imputed));
      el.select('circle').attr('opacity', getNodeOpacity(d.is_imputed));
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
        simulation.force('center', d3.forceCenter(w / 2, h / 2));
        simulation.alpha(0.3).restart();
      }
    },

    setHighlighted(handles: Set<string>) {
      highlighted = handles;
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
  };

  return controller;
}
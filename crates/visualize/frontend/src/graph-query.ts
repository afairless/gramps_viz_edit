// Graph topology query functions for multi-node selection.
// Builds an adjacency map from GraphData and provides traversal functions
// to compute indirect node sets (ancestors, descendants, etc.).

import type { GraphData, SelectionMode } from './types';

// ---------------------------------------------------------------------------
// Adjacency data structure
// ---------------------------------------------------------------------------

export interface Adjacency {
  parents: Set<string>;
  children: Set<string>;
  spouses: Set<string>;
  siblings: Set<string>;
  allNeighbors: Set<string>;
}

/**
 * Build adjacency indices from GraphData.
 *
 * 1. Creates an entry for every node (even isolated nodes).
 * 2. Populates parents, children, spouses from links.
 * 3. Derives siblings from shared parents.
 * 4. Computes allNeighbors as the union of all other sets.
 */
export function buildAdjacency(data: GraphData): Map<string, Adjacency> {
  const adj = new Map<string, Adjacency>();

  // Step 1: entry for every node
  for (const node of data.nodes) {
    adj.set(node.handle, {
      parents: new Set(),
      children: new Set(),
      spouses: new Set(),
      siblings: new Set(),
      allNeighbors: new Set(),
    });
  }

  // Step 2: populate parents, children, spouses from links
  for (const link of data.links) {
    const src = adj.get(link.source);
    const tgt = adj.get(link.target);
    if (!src || !tgt) continue;

    if (link.link_type === 'ParentChild') {
      // source is the parent, target is the child
      src.children.add(link.target);
      tgt.parents.add(link.source);
    } else if (link.link_type === 'Spouse') {
      src.spouses.add(link.target);
      tgt.spouses.add(link.source);
    }
  }

  // Step 3: derive siblings from shared parents
  for (const [, entry] of adj) {
    const childList = [...entry.children];
    for (let i = 0; i < childList.length; i++) {
      for (let j = i + 1; j < childList.length; j++) {
        const a = adj.get(childList[i]);
        const b = adj.get(childList[j]);
        if (a && b) {
          a.siblings.add(childList[j]);
          b.siblings.add(childList[i]);
        }
      }
    }
  }

  // Step 4: compute allNeighbors as union
  for (const [, entry] of adj) {
    entry.allNeighbors = new Set([
      ...entry.parents,
      ...entry.children,
      ...entry.spouses,
      ...entry.siblings,
    ]);
  }

  return adj;
}

// ---------------------------------------------------------------------------
// Traversal functions
// ---------------------------------------------------------------------------

/**
 * Get all ancestors of a node (parents, grandparents, etc.).
 * Excludes the starting node. Uses a visited set for cycle safety.
 */
export function getAncestors(
  adj: Map<string, Adjacency>,
  handle: string,
): Set<string> {
  const entry = adj.get(handle);
  if (!entry) return new Set();

  const result = new Set<string>();
  const visited = new Set<string>();
  const stack = [...entry.parents];

  while (stack.length > 0) {
    const current = stack.pop()!;
    if (visited.has(current)) continue;
    visited.add(current);
    result.add(current);

    const currentEntry = adj.get(current);
    if (currentEntry) {
      for (const parent of currentEntry.parents) {
        if (!visited.has(parent)) {
          stack.push(parent);
        }
      }
    }
  }

  return result;
}

/**
 * Get all descendants of a node (children, grandchildren, etc.).
 * Excludes the starting node. Uses a visited set for cycle safety.
 */
export function getDescendants(
  adj: Map<string, Adjacency>,
  handle: string,
): Set<string> {
  const entry = adj.get(handle);
  if (!entry) return new Set();

  const result = new Set<string>();
  const visited = new Set<string>();
  const stack = [...entry.children];

  while (stack.length > 0) {
    const current = stack.pop()!;
    if (visited.has(current)) continue;
    visited.add(current);
    result.add(current);

    const currentEntry = adj.get(current);
    if (currentEntry) {
      for (const child of currentEntry.children) {
        if (!visited.has(child)) {
          stack.push(child);
        }
      }
    }
  }

  return result;
}

/**
 * Get all 1st-degree connections: parents, children, spouses.
 */
export function getFirstDegree(
  adj: Map<string, Adjacency>,
  handle: string,
): Set<string> {
  const entry = adj.get(handle);
  if (!entry) return new Set();

  return new Set([
    ...entry.parents,
    ...entry.children,
    ...entry.spouses,
  ]);
}

/**
 * Get all 2nd-degree connections: nodes reachable in 1 or 2 hops
 * through allNeighbors. Includes 1st-degree connections, siblings,
 * grandparents, grandchildren, aunts/uncles, nieces/nephews.
 */
export function getSecondDegree(
  adj: Map<string, Adjacency>,
  handle: string,
): Set<string> {
  const entry = adj.get(handle);
  if (!entry) return new Set();

  const result = new Set<string>();
  const visited = new Set<string>([handle]);
  // BFS limited to 2 hops
  const queue: Array<{ node: string; depth: number }> = [{ node: handle, depth: 0 }];

  while (queue.length > 0) {
    const { node, depth } = queue.shift()!;
    if (depth > 0) {
      result.add(node);
    }
    if (depth >= 2) continue;

    const currentEntry = adj.get(node);
    if (currentEntry) {
      for (const neighbor of currentEntry.allNeighbors) {
        if (!visited.has(neighbor)) {
          visited.add(neighbor);
          queue.push({ node: neighbor, depth: depth + 1 });
        }
      }
    }
  }

  return result;
}

/**
 * Dispatch to the correct query function based on selection mode.
 * Returns an empty set for 'single' mode.
 */
export function getIndirectSet(
  adj: Map<string, Adjacency>,
  handle: string,
  mode: SelectionMode,
): Set<string> {
  switch (mode) {
    case 'single':
      return new Set();
    case 'ancestors':
      return getAncestors(adj, handle);
    case 'descendants':
      return getDescendants(adj, handle);
    case 'first-degree':
      return getFirstDegree(adj, handle);
    case 'second-degree':
      return getSecondDegree(adj, handle);
  }
}
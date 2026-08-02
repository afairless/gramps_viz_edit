// Hover tooltip for the family graph visualization.
// Shows on 200ms delay with name, birth date, and death date.
// Follows cursor position; hides on mouse-out.

import type { GraphData } from './types';

const TOOLTIP_DELAY_MS = 200;

let tooltipEl: HTMLElement | null = null;
let hoverTimer: ReturnType<typeof setTimeout> | null = null;
let currentHandle: string | null = null;

function getTooltip(): HTMLElement {
  if (!tooltipEl) {
    tooltipEl = document.getElementById('tooltip');
  }
  if (!tooltipEl) {
    throw new Error('#tooltip element not found in the DOM');
  }
  return tooltipEl;
}

// Build a lookup map from the graph data for quick access.
function buildLookup(data: GraphData): Map<string, { name: string; birth_date: string | null; death_date: string | null }> {
  const map = new Map<string, { name: string; birth_date: string | null; death_date: string | null }>();
  for (const node of data.nodes) {
    map.set(node.handle, {
      name: node.name,
      birth_date: node.birth_date,
      death_date: node.death_date,
    });
  }
  return map;
}

/**
 * Create a hover handler that can be passed to `graphController.onNodeHover()`.
 * Returns a callback `(handle: string | null, event: MouseEvent) => void`.
 */
export function createHoverHandler(data: GraphData): (handle: string | null, event: MouseEvent) => void {
  const lookup = buildLookup(data);

  return (handle: string | null, event: MouseEvent) => {
    if (handle === null) {
      // Mouse left the node
      if (hoverTimer) {
        clearTimeout(hoverTimer);
        hoverTimer = null;
      }
      hideTooltip();
      currentHandle = null;
      return;
    }

    // Mouse entered a node
    currentHandle = handle;

    if (hoverTimer) {
      clearTimeout(hoverTimer);
    }

    hoverTimer = setTimeout(() => {
      if (currentHandle !== handle) return; // Stale timer

      const info = lookup.get(handle);
      if (!info) return;

      showTooltip(info.name, info.birth_date, info.death_date, event);
    }, TOOLTIP_DELAY_MS);
  };
}

function showTooltip(name: string, birthDate: string | null, deathDate: string | null, event: MouseEvent): void {
  const el = getTooltip();
  el.textContent = '';

  const title = document.createElement('strong');
  title.textContent = name;
  el.appendChild(title);

  if (birthDate) {
    el.appendChild(document.createElement('br'));
    const span = document.createElement('span');
    span.textContent = `Born: ${birthDate}`;
    el.appendChild(span);
  }

  if (deathDate) {
    el.appendChild(document.createElement('br'));
    const span = document.createElement('span');
    span.textContent = `Died: ${deathDate}`;
    el.appendChild(span);
  }

  // Position near cursor with offset
  const offsetX = 12;
  const offsetY = 12;
  el.style.left = `${event.clientX + offsetX}px`;
  el.style.top = `${event.clientY + offsetY}px`;
  el.classList.remove('hidden');
}

function hideTooltip(): void {
  const el = getTooltip();
  el.classList.add('hidden');
}

/**
 * Cleanup the tooltip state (e.g., when graph data is replaced).
 */
export function resetTooltip(): void {
  if (hoverTimer) {
    clearTimeout(hoverTimer);
    hoverTimer = null;
  }
  currentHandle = null;
  hideTooltip();
}
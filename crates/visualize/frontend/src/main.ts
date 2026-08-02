// Entry point for the family-group visualization frontend.
// Mounts the D3 force-directed graph and wires up Tauri IPC when available.

import { renderGraph, validateGraphData } from './graph';
import type { GraphController } from './graph';
import type { GraphData } from './types';
import { createHoverHandler } from './tooltip';

function showError(container: HTMLElement, message: string): void {
  container.textContent = '';
  const div = document.createElement('div');
  div.style.display = 'flex';
  div.style.alignItems = 'center';
  div.style.justifyContent = 'center';
  div.style.height = '100%';
  div.style.color = '#666';
  div.style.fontSize = '16px';
  const p = document.createElement('p');
  p.textContent = message;
  div.appendChild(p);
  container.appendChild(div);
}

function showEmpty(container: HTMLElement): void {
  container.textContent = '';
  const div = document.createElement('div');
  div.style.display = 'flex';
  div.style.alignItems = 'center';
  div.style.justifyContent = 'center';
  div.style.height = '100%';
  div.style.color = '#999';
  div.style.fontSize = '16px';
  const p = document.createElement('p');
  p.textContent = 'No people found in the Gramps file.';
  div.appendChild(p);
  container.appendChild(div);
}

async function main(): Promise<void> {
  const container = document.getElementById('graph-container');
  if (!container) {
    console.error('graph-container element not found');
    return;
  }

  // Try loading data — in dev mode we use a script-injected fixture,
  // in Tauri mode we invoke the backend.
  let graphData: GraphData | null = null;

  // Check if we're running inside Tauri
  const isTauri =
    typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  if (isTauri) {
    try {
      // Dynamic import to avoid bundling Tauri API in dev builds
      const tauri = await import('@tauri-apps/api/core');
      // The path is passed as a CLI arg to the Tauri binary and can be
      // accessed via window.__GRAMPS_FILE__
      const filePath: string =
        (window as unknown as Record<string, string>).__GRAMPS_FILE__ || '';
      graphData = await tauri.invoke('load_graph', { path: filePath });
    } catch (err) {
      console.error('Failed to load graph data via Tauri IPC:', err);
      showError(container, 'Failed to load Gramps data file.');
      return;
    }
  } else {
    // Dev mode: try to load from window.__GRAPH_DATA__ (injected by test harness)
    const devData = (window as unknown as Record<string, unknown>).__GRAPH_DATA__;
    if (devData && validateGraphData(devData)) {
      graphData = devData as GraphData;
    } else {
      console.warn(
        'No graph data available. Set window.__GRAPH_DATA__ for dev mode.',
      );
      showEmpty(container);
      return;
    }
  }

  // Validate and render
  if (!graphData) {
    showEmpty(container);
    return;
  }

  if (graphData.nodes.length === 0) {
    showEmpty(container);
    return;
  }

  const controller = renderGraph(container, graphData);

  // Wire up hover tooltip
  const hoverHandler = createHoverHandler(graphData);
  controller.onNodeHover(hoverHandler);

  // Store controller for dev console access
  (window as unknown as Record<string, GraphController>).__GRAPH_CONTROLLER__ =
    controller;
}

// Boot
document.addEventListener('DOMContentLoaded', main);
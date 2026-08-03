// Entry point for the family-group visualization frontend.
// Mounts the D3 force-directed graph and wires up Tauri IPC when available.

import { renderGraph, validateGraphData } from './graph';
import type { GraphController } from './graph';
import type { GraphData } from './types';
import { createHoverHandler } from './tooltip';
import { createSelectionPanel, exportToFile } from './selection';
import { renderLegend, buildColorScale } from './colors';

/** Default generation gap in years when not specified via CLI. */
const DEFAULT_GENERATION_GAP = 25;

/** Default for --no-impute when not specified via CLI. */
const DEFAULT_NO_IMPUTE = false;

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

function renderFilterDropdown(
  data: GraphData,
  controller: GraphController,
): HTMLElement | null {
  const container = document.createElement('div');
  container.id = 'filter-container';
  container.style.position = 'absolute';
  container.style.top = '20px';
  container.style.left = '20px';
  container.style.zIndex = '500';

  const label = document.createElement('label');
  label.textContent = 'Family Group: ';
  label.style.fontSize = '12px';
  label.style.color = '#666';
  label.style.marginRight = '4px';
  container.appendChild(label);

  const select = document.createElement('select');
  select.style.padding = '4px 8px';
  select.style.fontSize = '12px';
  select.style.borderRadius = '4px';
  select.style.border = '1px solid #ccc';

  const allOption = document.createElement('option');
  allOption.value = '';
  allOption.textContent = 'All groups';
  select.appendChild(allOption);

  // Sort family groups by id for consistent ordering
  const sorted = [...data.family_groups].sort((a, b) => a.id - b.id);
  for (const fg of sorted) {
    const option = document.createElement('option');
    option.value = String(fg.id);
    option.textContent = `Group ${fg.id} (${fg.size} people, ${fg.span} gen.)`;
    select.appendChild(option);
  }

  select.addEventListener('change', () => {
    const val = select.value;
    controller.setFamilyGroupFilter(val === '' ? null : Number(val));
  });

  container.appendChild(select);
  return container;
}

/**
 * Show a file-open dialog, load the selected .gramps file via Tauri IPC,
 * and render the graph. Returns `true` if a file was loaded, `false` if cancelled.
 */
async function openAndRenderFile(
  container: HTMLElement,
  appEl: HTMLElement,
): Promise<boolean> {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const selected = await open({
    multiple: false,
    filters: [{ name: 'Gramps XML', extensions: ['gramps'] }],
  });
  if (!selected) return false; // user cancelled

  const tauri = await import('@tauri-apps/api/core');
  let graphData: GraphData;
  try {
    const gap: number =
      (window as unknown as Record<string, number>).__GENERATION_GAP__ ??
      DEFAULT_GENERATION_GAP;
    const noImpute: boolean =
      (window as unknown as Record<string, boolean>).__NO_IMPUTE__ ??
      DEFAULT_NO_IMPUTE;
    graphData = await tauri.invoke('load_graph', {
      path: selected,
      noImpute: noImpute,
      generationGap: gap,
    });
  } catch (err) {
    console.error('Failed to load graph data via Tauri IPC:', err);
    // Differentiate between IPC permission errors and data errors
    const message =
      err instanceof Error && err.message?.includes('not allowed')
        ? 'Permission denied: could not open the file dialog. Try reinstalling the application.'
        : 'Failed to load Gramps data file. The file may be corrupted or not a valid Gramps XML file.';
    showError(container, message);
    return false;
  }

  renderGraphFromData(container, appEl, graphData);
  return true;
}

/**
 * Load a .gramps file from an explicit path (e.g. from CLI --path arg)
 * and render the graph.
 */
async function openAndRenderFileFromPath(
  container: HTMLElement,
  appEl: HTMLElement,
  filePath: string,
): Promise<void> {
  const tauri = await import('@tauri-apps/api/core');

  const gap: number =
    (window as unknown as Record<string, number>).__GENERATION_GAP__ ??
    DEFAULT_GENERATION_GAP;
  const noImpute: boolean =
    (window as unknown as Record<string, boolean>).__NO_IMPUTE__ ??
    DEFAULT_NO_IMPUTE;

  let graphData: GraphData;
  try {
    graphData = await tauri.invoke('load_graph', {
      path: filePath,
      noImpute: noImpute,
      generationGap: gap,
    });
  } catch (err) {
    console.error('Failed to load graph data via Tauri IPC:', err);
    showError(container, 'Failed to load Gramps data file.');
    return;
  }

  renderGraphFromData(container, appEl, graphData);
}

/**
 * Render a toolbar containing the family group filter dropdown and a reset layout button.
 */
export function renderToolbar(
  graphData: GraphData,
  controller: GraphController,
): HTMLElement {
  const toolbar = document.createElement('div');
  toolbar.id = 'toolbar';
  toolbar.style.position = 'absolute';
  toolbar.style.top = '20px';
  toolbar.style.left = '20px';
  toolbar.style.zIndex = '500';
  toolbar.style.display = 'flex';
  toolbar.style.alignItems = 'center';
  toolbar.style.gap = '8px';

  // Family group filter dropdown
  const filterDropdown = renderFilterDropdown(graphData, controller);
  if (filterDropdown) {
    filterDropdown.style.position = 'relative';
    filterDropdown.style.top = 'auto';
    filterDropdown.style.left = 'auto';
    filterDropdown.style.zIndex = 'auto';
    toolbar.appendChild(filterDropdown);
  }

  // Reset layout button
  const resetBtn = document.createElement('button');
  resetBtn.textContent = '↺ Reset';
  resetBtn.title = 'Reset node positions to force-directed layout';
  resetBtn.style.padding = '4px 10px';
  resetBtn.style.fontSize = '12px';
  resetBtn.style.borderRadius = '4px';
  resetBtn.style.border = '1px solid #ccc';
  resetBtn.style.background = '#fff';
  resetBtn.style.cursor = 'pointer';
  resetBtn.style.color = '#333';
  resetBtn.addEventListener('mouseenter', () => {
    resetBtn.style.background = '#eee';
  });
  resetBtn.addEventListener('mouseleave', () => {
    resetBtn.style.background = '#fff';
  });
  resetBtn.addEventListener('click', () => {
    controller.resetLayout();
  });
  toolbar.appendChild(resetBtn);

  return toolbar;
}

/** Render the graph UI from already-loaded GraphData. */
function renderGraphFromData(
  container: HTMLElement,
  appEl: HTMLElement,
  graphData: GraphData,
): void {
  // Clear any previous content (e.g. the open-file prompt)
  container.textContent = '';

  if (graphData.nodes.length === 0) {
    showEmpty(container);
    return;
  }

  const controller = renderGraph(container, graphData);

  // Wire up hover tooltip
  const hoverHandler = createHoverHandler(graphData);
  controller.onNodeHover(hoverHandler);

  // Wire up toolbar (filter dropdown + reset button)
  const toolbar = renderToolbar(graphData, controller);
  if (appEl) {
    appEl.insertBefore(toolbar, document.getElementById('legend'));
  }

  // Wire up color legend
  const legendEl = document.getElementById('legend');
  if (legendEl) {
    const scale = buildColorScale(graphData.nodes.map((n) => n.birth_year));
    const knownYears = graphData.nodes
      .map((n) => n.birth_year)
      .filter((y): y is number => y !== null);
    renderLegend(legendEl, {
      colorScale: scale,
      minYear: knownYears.length > 0 ? Math.min(...knownYears) : null,
      maxYear: knownYears.length > 0 ? Math.max(...knownYears) : null,
      hasImputed: graphData.nodes.some((n) => n.is_imputed),
      hasUndated: graphData.nodes.some((n) => n.birth_year === null),
      hasSpouseLinks: graphData.links.some((l) => l.link_type === 'Spouse'),
      hasParentChildLinks: graphData.links.some((l) => l.link_type === 'ParentChild'),
    });
  }

  // Wire up selection panel
  const panelEl = document.getElementById('selection-panel');
  if (panelEl) {
    const selectionManager = createSelectionPanel(graphData, {
      panelEl,
      onExport: async (exportData) => {
        await exportToFile(exportData);
      },
    });

    // Route graph node clicks to selection manager
    controller.onNodeClick((handle: string) => {
      selectionManager.click(handle, false);
      controller.setHighlighted(new Set(selectionManager.handles));
    });
  }

  // Store controller for dev console access
  (window as unknown as Record<string, GraphController>).__GRAPH_CONTROLLER__ =
    controller;
}

/**
 * Show a welcome screen with a large "Open Gramps File" button.
 */
function showWelcomeScreen(container: HTMLElement): void {
  container.textContent = '';

  const wrapper = document.createElement('div');
  wrapper.style.display = 'flex';
  wrapper.style.flexDirection = 'column';
  wrapper.style.alignItems = 'center';
  wrapper.style.justifyContent = 'center';
  wrapper.style.height = '100%';
  wrapper.style.gap = '16px';

  const title = document.createElement('h2');
  title.textContent = 'Gramps Family Group Visualization';
  title.style.color = '#333';
  title.style.fontSize = '20px';
  title.style.fontWeight = '600';
  title.style.margin = '0';
  wrapper.appendChild(title);

  const subtitle = document.createElement('p');
  subtitle.textContent = 'Open a Gramps XML (.gramps) file to get started.';
  subtitle.style.color = '#888';
  subtitle.style.fontSize = '14px';
  subtitle.style.margin = '0';
  wrapper.appendChild(subtitle);

  const btn = document.createElement('button');
  btn.textContent = 'Open Gramps File';
  btn.style.padding = '12px 32px';
  btn.style.fontSize = '16px';
  btn.style.borderRadius = '6px';
  btn.style.border = 'none';
  btn.style.backgroundColor = '#2266aa';
  btn.style.color = '#fff';
  btn.style.cursor = 'pointer';
  btn.style.marginTop = '8px';
  btn.addEventListener('mouseenter', () => {
    btn.style.backgroundColor = '#1a4f88';
  });
  btn.addEventListener('mouseleave', () => {
    btn.style.backgroundColor = '#2266aa';
  });
  btn.addEventListener('click', async () => {
    const appEl = document.getElementById('app');
    if (appEl) {
      await openAndRenderFile(container, appEl);
    }
  });
  wrapper.appendChild(btn);

  container.appendChild(wrapper);
}

async function main(): Promise<void> {
  const container = document.getElementById('graph-container');
  if (!container) {
    console.error('graph-container element not found');
    return;
  }

  // Check if we're running inside Tauri
  const isTauri =
    typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  if (isTauri) {
    // Check if a path was passed via CLI (window.__GRAMPS_FILE__)
    const cliPath: string | undefined =
      (window as unknown as Record<string, string | undefined>).__GRAMPS_FILE__;
    if (cliPath) {
      // Auto-load the file from the CLI arg
      const appEl = document.getElementById('app');
      if (appEl) {
        await openAndRenderFileFromPath(container, appEl, cliPath);
      }
    } else {
      // Show the welcome screen with an open-file button
      showWelcomeScreen(container);
    }
  } else {
    // Dev mode: try to load from window.__GRAPH_DATA__ (injected by test harness)
    const devData = (window as unknown as Record<string, unknown>).__GRAPH_DATA__;
    if (devData && validateGraphData(devData)) {
      const appEl = document.getElementById('app');
      if (appEl) {
        renderGraphFromData(container, appEl, devData as GraphData);
      }
    } else {
      console.warn(
        'No graph data available. Set window.__GRAPH_DATA__ for dev mode.',
      );
      showEmpty(container);
    }
  }
}

// Boot
document.addEventListener('DOMContentLoaded', main);
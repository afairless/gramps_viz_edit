// Entry point for the family-group visualization frontend.
// Mounts the D3 force-directed graph and wires up Tauri IPC when available.

import { renderGraph, validateGraphData } from './graph';
import type { GraphController } from './graph';
import type { GraphData, LoadedGraph, SelectionMode } from './types';
import { DEFAULT_FORCE_CONFIG, SELECTION_MODES, type ForceConfig } from './types';
import { buildAdjacency, getIndirectSet } from './graph-query';
import { createHoverHandler } from './tooltip';
import { createSelectionPanel, exportToFile } from './selection';
import { renderLegend, buildColorScale } from './colors';
import { StatsPanel } from './stats-panel';
import type { StatsReport } from './types';

/** Default generation gap in years when not specified via CLI. */
const DEFAULT_GENERATION_GAP = 25;

/** Default for --no-impute when not specified via CLI. */
const DEFAULT_NO_IMPUTE = false;

/** Global stats panel instance, created in main(). */
let statsPanel!: StatsPanel;

/**
 * Fetch file statistics via Tauri IPC and render them in the stats panel.
 * Handles errors gracefully, showing a non-intrusive error message.
 */
async function fetchAndRenderStats(filePath: string): Promise<void> {
  const tauri = await import('@tauri-apps/api/core');
  try {
    const report: StatsReport = await tauri.invoke('get_stats', { path: filePath });
    statsPanel.render(report);
  } catch (err) {
    console.warn('Failed to load stats:', err);
    statsPanel.renderError('Failed to load statistics. The file may have been moved or deleted.');
  }
}

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
): { container: HTMLElement; select: HTMLSelectElement } | null {
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
  return { container, select };
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
  let loadedGraph: LoadedGraph;
  try {
    const gap: number =
      (window as unknown as Record<string, number>).__GENERATION_GAP__ ??
      DEFAULT_GENERATION_GAP;
    const noImpute: boolean =
      (window as unknown as Record<string, boolean>).__NO_IMPUTE__ ??
      DEFAULT_NO_IMPUTE;
    loadedGraph = await tauri.invoke('load_graph', {
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

  renderGraphFromData(container, appEl, loadedGraph.graph_data, selected, loadedGraph.stats);
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

  let loadedGraph: LoadedGraph;
  try {
    loadedGraph = await tauri.invoke('load_graph', {
      path: filePath,
      noImpute: noImpute,
      generationGap: gap,
    });
  } catch (err) {
    console.error('Failed to load graph data via Tauri IPC:', err);
    showError(container, 'Failed to load Gramps data file.');
    return;
  }

  renderGraphFromData(container, appEl, loadedGraph.graph_data, filePath, loadedGraph.stats);
}

export function renderModeSelector(onChange: (mode: SelectionMode) => void): HTMLElement {
  const container = document.createElement('div');
  container.id = 'mode-selector-container';
  container.style.display = 'inline-flex';
  container.style.alignItems = 'center';
  container.style.gap = '4px';

  const label = document.createElement('label');
  label.textContent = 'Mode: ';
  label.style.fontSize = '12px';
  label.style.color = '#666';
  label.style.marginRight = '2px';
  container.appendChild(label);

  const select = document.createElement('select');
  select.style.padding = '4px 8px';
  select.style.fontSize = '12px';
  select.style.borderRadius = '4px';
  select.style.border = '1px solid #ccc';

  for (const option of SELECTION_MODES) {
    const optEl = document.createElement('option');
    optEl.value = option.value;
    optEl.textContent = option.label;
    select.appendChild(optEl);
  }

  select.addEventListener('change', () => {
    onChange(select.value as SelectionMode);
  });

  container.appendChild(select);
  return container;
}

export function renderSelectAllButtons(
  onSelectAll: () => void,
  onDeselectAll: () => void,
): HTMLElement {
  const container = document.createElement('div');
  container.id = 'select-all-container';
  container.style.display = 'inline-flex';
  container.style.alignItems = 'center';
  container.style.gap = '4px';

  const selectAllBtn = document.createElement('button');
  selectAllBtn.textContent = 'Select All';
  selectAllBtn.title = 'Select all visible nodes';
  selectAllBtn.style.padding = '4px 10px';
  selectAllBtn.style.fontSize = '12px';
  selectAllBtn.style.borderRadius = '4px';
  selectAllBtn.style.border = '1px solid #ccc';
  selectAllBtn.style.background = '#fff';
  selectAllBtn.style.cursor = 'pointer';
  selectAllBtn.style.color = '#333';
  selectAllBtn.addEventListener('mouseenter', () => {
    selectAllBtn.style.background = '#eee';
  });
  selectAllBtn.addEventListener('mouseleave', () => {
    selectAllBtn.style.background = '#fff';
  });
  selectAllBtn.addEventListener('click', () => onSelectAll());
  container.appendChild(selectAllBtn);

  const deselectAllBtn = document.createElement('button');
  deselectAllBtn.textContent = 'Deselect All';
  deselectAllBtn.title = 'Clear all selections';
  deselectAllBtn.style.padding = '4px 10px';
  deselectAllBtn.style.fontSize = '12px';
  deselectAllBtn.style.borderRadius = '4px';
  deselectAllBtn.style.border = '1px solid #ccc';
  deselectAllBtn.style.background = '#fff';
  deselectAllBtn.style.cursor = 'pointer';
  deselectAllBtn.style.color = '#333';
  deselectAllBtn.addEventListener('mouseenter', () => {
    deselectAllBtn.style.background = '#eee';
  });
  deselectAllBtn.addEventListener('mouseleave', () => {
    deselectAllBtn.style.background = '#fff';
  });
  deselectAllBtn.addEventListener('click', () => onDeselectAll());
  container.appendChild(deselectAllBtn);

  return container;
}

/**
 * Render a collapsible force-control panel with three sliders and a restore-defaults button.
 * Collapsed by default. Each slider maps to a ForceConfig key with range [0, 2].
 */
export function renderForcePanel(
  config: ForceConfig,
  onChange: (c: ForceConfig) => void,
): HTMLElement {
  const panel = document.createElement('div');
  panel.id = 'force-panel';

  // ---- header (always visible, click toggles body) ----
  const header = document.createElement('div');
  header.className = 'force-header';
  const title = document.createElement('span');
  title.textContent = 'Force Controls';
  const toggle = document.createElement('span');
  toggle.textContent = '\u25B2'; // ▲ (up = expanded)
  toggle.style.fontSize = '10px';
  header.appendChild(title);
  header.appendChild(toggle);

  // ---- body (collapsed by default) ----
  const body = document.createElement('div');
  body.className = 'force-body';
  body.style.display = 'none';

  interface SliderDef {
    key: keyof ForceConfig;
    label: string;
  }
  const sliders: SliderDef[] = [
    { key: 'generationPull', label: 'Generation pull' },
    { key: 'spouseStrength', label: 'Spouse bond' },
    { key: 'parentChildStrength', label: 'Parent-child bond' },
  ];

  const valueSpans: Record<string, HTMLSpanElement> = {};

  for (const s of sliders) {
    const row = document.createElement('div');
    row.className = 'force-slider';

    const lbl = document.createElement('label');
    lbl.textContent = s.label;
    row.appendChild(lbl);

    const input = document.createElement('input');
    input.type = 'range';
    input.min = '0';
    input.max = '200';
    input.step = '1';
    input.value = String(Math.round(config[s.key] * 100));
    row.appendChild(input);

    const valSpan = document.createElement('span');
    valSpan.className = 'value';
    valSpan.textContent = config[s.key].toFixed(2);
    row.appendChild(valSpan);
    valueSpans[s.key] = valSpan;

    input.addEventListener('input', () => {
      const val = Number(input.value) / 100;
      valSpan.textContent = val.toFixed(2);
      onChange({ ...config, [s.key]: val });
    });

    body.appendChild(row);
  }

  // Restore defaults button
  const restoreBtn = document.createElement('button');
  restoreBtn.textContent = 'Restore defaults';
  restoreBtn.style.padding = '4px 10px';
  restoreBtn.style.fontSize = '11px';
  restoreBtn.style.borderRadius = '4px';
  restoreBtn.style.border = '1px solid #ccc';
  restoreBtn.style.background = '#fff';
  restoreBtn.style.cursor = 'pointer';
  restoreBtn.style.marginTop = '6px';
  restoreBtn.addEventListener('mouseenter', () => {
    restoreBtn.style.background = '#eee';
  });
  restoreBtn.addEventListener('mouseleave', () => {
    restoreBtn.style.background = '#fff';
  });
  restoreBtn.addEventListener('click', () => {
    // Reset sliders to defaults using nth-child selectors
    const rowEls = body.querySelectorAll('.force-slider');
    for (let i = 0; i < sliders.length && i < rowEls.length; i++) {
      const inputEl = rowEls[i].querySelector<HTMLInputElement>('input[type="range"]');
      const valSpan = rowEls[i].querySelector<HTMLSpanElement>('.value');
      if (inputEl) {
        const defaultVal = Math.round(DEFAULT_FORCE_CONFIG[sliders[i].key] * 100);
        inputEl.value = String(defaultVal);
      }
      if (valSpan) {
        valSpan.textContent = DEFAULT_FORCE_CONFIG[sliders[i].key].toFixed(2);
      }
    }
    onChange({ ...DEFAULT_FORCE_CONFIG });
  });
  body.appendChild(restoreBtn);

  // Toggle expand/collapse
  let expanded = false;
  header.addEventListener('click', () => {
    expanded = !expanded;
    body.style.display = expanded ? 'flex' : 'none';
    toggle.textContent = expanded ? '\u25B2' : '\u25BC'; // ▲ or ▼
  });

  panel.appendChild(header);
  panel.appendChild(body);
  return panel;
}

/**
 * Render a toolbar containing the family group filter dropdown, selection controls,
 * reset button, and force control panel.
 */
export function renderToolbar(
  graphData: GraphData,
  controller: GraphController,
  forceConfig?: ForceConfig,
  onForceConfigChange?: (c: ForceConfig) => void,
  selectionManager?: {
    addAll: (handles: Iterable<string>) => void;
    removeAll: (handles: Iterable<string>) => void;
    clear: () => void;
    handles: string[];
  },
  onModeChange?: (mode: SelectionMode) => void,
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
  toolbar.style.flexWrap = 'wrap';

  // Selection mode selector
  if (onModeChange) {
    const modeSelector = renderModeSelector(onModeChange);
    toolbar.appendChild(modeSelector);
  }

  // Select All / Deselect All buttons
  if (selectionManager) {
    const selectAllEl = renderSelectAllButtons(
      () => {
        selectionManager!.addAll(controller.getVisibleNodes());
        controller.setHighlighted(new Set(selectionManager!.handles));
      },
      () => {
        selectionManager!.clear();
        controller.setHighlighted(new Set());
      },
    );
    toolbar.appendChild(selectAllEl);
  }

  // Visual separator
  const sep = document.createElement('span');
  sep.textContent = '|';
  sep.style.color = '#ccc';
  sep.style.fontSize = '14px';
  sep.style.margin = '0 2px';
  toolbar.appendChild(sep);

  // Family group filter dropdown + group select/deselect buttons
  const filterResult = renderFilterDropdown(graphData, controller);
  if (filterResult) {
    filterResult.container.style.position = 'relative';
    filterResult.container.style.top = 'auto';
    filterResult.container.style.left = 'auto';
    filterResult.container.style.zIndex = 'auto';
    toolbar.appendChild(filterResult.container);

    // Group select/deselect buttons (disabled when "All groups" selected)
    if (selectionManager) {
      const groupSelectBtn = document.createElement('button');
      groupSelectBtn.textContent = 'Select Group';
      groupSelectBtn.title = 'Select all nodes in this family group';
      groupSelectBtn.style.padding = '4px 10px';
      groupSelectBtn.style.fontSize = '12px';
      groupSelectBtn.style.borderRadius = '4px';
      groupSelectBtn.style.border = '1px solid #ccc';
      groupSelectBtn.style.background = '#fff';
      groupSelectBtn.style.cursor = 'pointer';
      groupSelectBtn.style.color = '#333';
      groupSelectBtn.disabled = filterResult.select.value === '';

      const groupDeselectBtn = document.createElement('button');
      groupDeselectBtn.textContent = 'Deselect Group';
      groupDeselectBtn.title = 'Deselect all nodes in this family group';
      groupDeselectBtn.style.padding = '4px 10px';
      groupDeselectBtn.style.fontSize = '12px';
      groupDeselectBtn.style.borderRadius = '4px';
      groupDeselectBtn.style.border = '1px solid #ccc';
      groupDeselectBtn.style.background = '#fff';
      groupDeselectBtn.style.cursor = 'pointer';
      groupDeselectBtn.style.color = '#333';
      groupDeselectBtn.disabled = filterResult.select.value === '';

      // Update button enabled state when filter changes
      filterResult.select.addEventListener('change', () => {
        const isAll = filterResult.select.value === '';
        groupSelectBtn.disabled = isAll;
        groupDeselectBtn.disabled = isAll;
      });

      // Group select: select all nodes in the chosen family group
      groupSelectBtn.addEventListener('click', () => {
        const groupId = filterResult.select.value;
        if (groupId === '') return;
        const groupHandles = graphData.nodes
          .filter((n) => n.family_group === Number(groupId))
          .map((n) => n.handle);
        selectionManager!.addAll(groupHandles);
        controller.setHighlighted(new Set(selectionManager!.handles));
      });

      // Group deselect: deselect all nodes in the chosen family group
      groupDeselectBtn.addEventListener('click', () => {
        const groupId = filterResult.select.value;
        if (groupId === '') return;
        const groupHandles = graphData.nodes
          .filter((n) => n.family_group === Number(groupId))
          .map((n) => n.handle);
        selectionManager!.removeAll(groupHandles);
        controller.setHighlighted(new Set(selectionManager!.handles));
      });

      toolbar.appendChild(groupSelectBtn);
      toolbar.appendChild(groupDeselectBtn);
    }
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
    if (forceConfig) {
      controller.setForceConfig(forceConfig);
    }
    controller.resetLayout();
  });
  toolbar.appendChild(resetBtn);

  // Force control panel (appended after reset button when config is provided)
  if (forceConfig && onForceConfigChange) {
    const forcePanel = renderForcePanel(forceConfig, onForceConfigChange);
    toolbar.appendChild(forcePanel);
  }

  return toolbar;
}

/** Render the graph UI from already-loaded GraphData. */
function renderGraphFromData(
  container: HTMLElement,
  appEl: HTMLElement,
  graphData: GraphData,
  filePath?: string,
  statsReport?: StatsReport,
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

  // Force config state (pending — updated by sliders, applied on reset)
  const forceConfig: ForceConfig = { ...DEFAULT_FORCE_CONFIG };

  // Build adjacency once from the full graph topology (not rebuilt on filter changes)
  const adjacency = buildAdjacency(graphData);
  let currentMode: SelectionMode = 'single';

  // Wire up selection panel first (toolbar buttons call into the manager)
  const panelEl = document.getElementById('selection-panel');
  let selectionManager: ReturnType<typeof createSelectionPanel> | null = null;
  if (panelEl) {
    selectionManager = createSelectionPanel(graphData, {
      panelEl,
      onExport: async (exportData) => {
        await exportToFile(exportData);
      },
    });
  }

  // Wire up toolbar (filter dropdown + reset button + force panel + selection controls)
  const toolbar = renderToolbar(
    graphData,
    controller,
    forceConfig,
    (c) => {
      Object.assign(forceConfig, c);
    },
    selectionManager ?? undefined,
    (mode: SelectionMode) => {
      currentMode = mode;
    },
  );
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

  // Route graph node clicks to selection manager (with indirect-set support)
  if (selectionManager) {
    controller.onNodeClick((handle: string) => {
      const indirect = getIndirectSet(adjacency, handle, currentMode);
      selectionManager!.clickWithIndirect(handle, indirect);
      controller.setHighlighted(new Set(selectionManager!.handles));
    });
  }

  // Store controller for dev console access
  (window as unknown as Record<string, GraphController>).__GRAPH_CONTROLLER__ =
    controller;

  // Render stats from the preloaded report when available, otherwise fetch
  if (statsReport) {
    statsPanel.render(statsReport);
  } else if (filePath) {
    fetchAndRenderStats(filePath);
  }
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

  // Create the stats panel and append to #app
  const appEl = document.getElementById('app');
  statsPanel = new StatsPanel();
  const statsPanelEl = statsPanel.create();
  if (appEl) {
    appEl.appendChild(statsPanelEl);
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
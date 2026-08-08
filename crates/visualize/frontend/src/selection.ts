// Click-to-select and export for the family graph visualization.
// Handles toggle selection, Shift multi-select, the selection panel,
// and JSON export.

import type { GraphData, PersonNode, SelectedPerson, SelectionExport } from './types';

// ---------------------------------------------------------------------------
// Pure selection logic (unit-testable)
// ---------------------------------------------------------------------------

export class SelectionManager {
  private selected = new Set<string>();

  /** Click behavior: toggle selection when no modifier held. */
  click(handle: string, shiftKey: boolean): void {
    if (shiftKey) {
      // Shift-click: add to selection without toggling others
      this.selected.add(handle);
    } else {
      this.toggle(handle);
    }
  }

  toggle(handle: string): void {
    if (this.selected.has(handle)) {
      this.selected.delete(handle);
    } else {
      this.selected.add(handle);
    }
  }

  add(handle: string): void {
    this.selected.add(handle);
  }

  remove(handle: string): void {
    this.selected.delete(handle);
  }

  /**
   * Click with indirect selection support.
   * If handle is NOT selected: add handle + all indirectHandles.
   * If handle IS selected: remove handle + all indirectHandles (unconditional —
   * indirects selected via other actions are also removed).
   */
  clickWithIndirect(handle: string, indirectHandles: Set<string>): void {
    if (this.selected.has(handle)) {
      // DESELECT: remove handle + all indirects
      this.selected.delete(handle);
      for (const h of indirectHandles) {
        this.selected.delete(h);
      }
    } else {
      // SELECT: add handle + all indirects
      this.selected.add(handle);
      for (const h of indirectHandles) {
        this.selected.add(h);
      }
    }
  }

  /** Add multiple handles at once (no toggle — pure add). */
  addAll(handles: Iterable<string>): void {
    for (const h of handles) {
      this.selected.add(h);
    }
  }

  /** Remove multiple handles at once (no toggle — pure remove). */
  removeAll(handles: Iterable<string>): void {
    for (const h of handles) {
      this.selected.delete(h);
    }
  }

  /**
   * Invert selection for the given set of handles.
   * Selected handles become deselected; unselected handles become selected.
   * Handles not in the provided iterable are left unchanged.
   */
  invertSelection(handles: Iterable<string>): void {
    for (const h of handles) {
      if (this.selected.has(h)) {
        this.selected.delete(h);
      } else {
        this.selected.add(h);
      }
    }
  }

  has(handle: string): boolean {
    return this.selected.has(handle);
  }

  get size(): number {
    return this.selected.size;
  }

  get handles(): string[] {
    return [...this.selected];
  }

  clear(): void {
    this.selected.clear();
  }
}

// ---------------------------------------------------------------------------
// Export data building (pure, unit-testable)
// ---------------------------------------------------------------------------

export function buildSelectedPeople(
  data: GraphData,
  handles: string[],
): SelectedPerson[] {
  const byHandle = new Map<string, PersonNode>();
  for (const node of data.nodes) {
    byHandle.set(node.handle, node);
  }
  const result: SelectedPerson[] = [];
  for (const handle of handles) {
    const node = byHandle.get(handle);
    if (!node) continue;
    result.push({
      handle: node.handle,
      name: node.name,
      birth_date: node.birth_date,
      death_date: node.death_date,
      gender: node.gender,
      family_group: node.family_group,
    });
  }
  return result;
}

export function buildSelectionExport(
  file: string,
  selections: SelectedPerson[],
  exportedAt?: Date,
): SelectionExport {
  return {
    exported_at: (exportedAt ?? new Date()).toISOString(),
    file,
    selections,
  };
}

// ---------------------------------------------------------------------------
// DOM wiring
// ---------------------------------------------------------------------------

export interface SelectionPanelOptions {
  /** Callback invoked with the export payload when the user clicks Export. */
  onExport?: (exportData: SelectionExport) => Promise<void> | void;
  /** Root element where the panel lives (#selection-panel). */
  panelEl: HTMLElement;
}

/**
 * Create the selection panel controller bound to #selection-panel.
 * The graph's onNodeClick callback should route clicks into `onNodeClick`.
 */
export function createSelectionPanel(
  data: GraphData,
  options: SelectionPanelOptions,
): SelectionManager {
  const manager = new SelectionManager();
  const { panelEl, onExport } = options;

  const countEl = document.createElement('span');
  countEl.id = 'selection-count';
  countEl.textContent = '0 selected';

  const exportBtn = document.createElement('button');
  exportBtn.id = 'export-btn';
  exportBtn.textContent = 'Export Selected';
  exportBtn.disabled = true;
  exportBtn.addEventListener('click', async () => {
    const exportData = buildSelectionExport(
      'selections.json',
      buildSelectedPeople(data, manager.handles),
    );
    if (onExport) {
      await onExport(exportData);
    }
  });

  panelEl.textContent = '';
  panelEl.appendChild(countEl);
  panelEl.appendChild(exportBtn);

  // Re-render panel on selection changes
  function render(): void {
    countEl.textContent = `${manager.size} selected`;
    exportBtn.disabled = manager.size === 0;
  }
  render();

  // Wrap manager methods to trigger render after mutation
  const origClick = manager.click.bind(manager);
  manager.click = (handle: string, shiftKey: boolean) => {
    origClick(handle, shiftKey);
    render();
  };
  const origToggle = manager.toggle.bind(manager);
  manager.toggle = (handle: string) => {
    origToggle(handle);
    render();
  };
  const origAdd = manager.add.bind(manager);
  manager.add = (handle: string) => {
    origAdd(handle);
    render();
  };
  const origRemove = manager.remove.bind(manager);
  manager.remove = (handle: string) => {
    origRemove(handle);
    render();
  };
  const origClear = manager.clear.bind(manager);
  manager.clear = () => {
    origClear();
    render();
  };

  const origClickWithIndirect = manager.clickWithIndirect.bind(manager);
  manager.clickWithIndirect = (handle: string, indirectHandles: Set<string>) => {
    origClickWithIndirect(handle, indirectHandles);
    render();
  };

  const origAddAll = manager.addAll.bind(manager);
  manager.addAll = (handles: Iterable<string>) => {
    origAddAll(handles);
    render();
  };

  const origRemoveAll = manager.removeAll.bind(manager);
  manager.removeAll = (handles: Iterable<string>) => {
    origRemoveAll(handles);
    render();
  };

  const origInvert = manager.invertSelection.bind(manager);
  manager.invertSelection = (handles: Iterable<string>) => {
    origInvert(handles);
    render();
  };

  return manager;
}

// ---------------------------------------------------------------------------
// Tauri export helper
// ---------------------------------------------------------------------------

/**
 * Export selections to a file via Tauri IPC.
 * Returns the path written on success, or null if cancelled.
 */
export async function exportToFile(
  exportData: SelectionExport,
): Promise<string | null> {
  try {
    // Try Tauri API — save dialog then write file
    const tauriDialog = await import('@tauri-apps/plugin-dialog');
    const path = await tauriDialog.save({
      defaultPath: 'selections.json',
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (!path) return null; // user cancelled

    const tauri = await import('@tauri-apps/api/core');
    await tauri.invoke('export_selections', {
      path,
      exportedAt: exportData.exported_at,
      file: exportData.file,
      selections: exportData.selections,
    });
    return path as string;
  } catch {
    // Fallback: download as blob in browser
    const blob = new Blob([JSON.stringify(exportData, null, 2)], {
      type: 'application/json',
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = exportData.file;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 100);
    return exportData.file;
  }
}
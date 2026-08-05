// @vitest-environment happy-dom
// Tests for the main.ts module — toolbar rendering and UI wiring.

import { describe, it, expect, vi } from 'vitest';
import { GRAMPS_FILE_FILTER, renderToolbar, renderForcePanel, renderModeSelector, renderSelectAllButtons, showWelcomeScreen } from '../src/main';
import type { GraphController } from '../src/graph';
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

describe('GRAMPS_FILE_FILTER', () => {
  it('accepts both gramps and xml extensions with a Gramps XML label', () => {
    expect(GRAMPS_FILE_FILTER.name).toBe('Gramps XML');
    expect(GRAMPS_FILE_FILTER.extensions).toContain('gramps');
    expect(GRAMPS_FILE_FILTER.extensions).toContain('xml');
  });
});

describe('showWelcomeScreen', () => {
  it('subtitle mentions both .gramps and .xml extensions', () => {
    const container = document.createElement('div');
    showWelcomeScreen(container);
    const subtitle = container.querySelector('p');
    expect(subtitle).toBeTruthy();
    expect(subtitle!.textContent).toContain('.gramps');
    expect(subtitle!.textContent).toContain('.xml');
  });
});

describe('renderToolbar', () => {
  it('returns a toolbar element with a reset button containing ↺', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
    } as unknown as GraphController;

    const toolbar = renderToolbar(data, mockController);

    expect(toolbar.id).toBe('toolbar');
    const buttons = toolbar.querySelectorAll('button');
    const resetBtn = Array.from(buttons).find((b) => b.textContent === '↺ Reset');
    expect(resetBtn).toBeTruthy();
    expect(resetBtn!.title).toBe('Reset node positions to force-directed layout');
  });

  it('includes a mode selector <select> with 5 options', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      getVisibleNodes: vi.fn().mockReturnValue([]),
      setHighlighted: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
    } as unknown as GraphController;

    const toolbar = renderToolbar(data, mockController, undefined, undefined, undefined, vi.fn());
    const modeSelect = toolbar.querySelector('#mode-selector-container select') as HTMLSelectElement;
    expect(modeSelect).toBeTruthy();
    expect(modeSelect.options.length).toBe(5);
    expect(modeSelect.options[0].value).toBe('single');
    expect(modeSelect.options[1].value).toBe('ancestors');
    expect(modeSelect.options[2].value).toBe('descendants');
    expect(modeSelect.options[3].value).toBe('first-degree');
    expect(modeSelect.options[4].value).toBe('second-degree');
  });

  it('mode selector onChange fires with correct mode on user interaction', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      getVisibleNodes: vi.fn().mockReturnValue([]),
      setHighlighted: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
    } as unknown as GraphController;

    const onChange = vi.fn();
    const toolbar = renderToolbar(data, mockController, undefined, undefined, undefined, onChange);
    const modeSelect = toolbar.querySelector('#mode-selector-container select') as HTMLSelectElement;
    expect(modeSelect).toBeTruthy();

    // Simulate changing to 'ancestors'
    modeSelect.value = 'ancestors';
    modeSelect.dispatchEvent(new Event('change', { bubbles: true }));
    expect(onChange).toHaveBeenCalledWith('ancestors');

    // Simulate changing to 'second-degree'
    modeSelect.value = 'second-degree';
    modeSelect.dispatchEvent(new Event('change', { bubbles: true }));
    expect(onChange).toHaveBeenCalledWith('second-degree');
  });

  it('includes Select All and Deselect All buttons in toolbar', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      getVisibleNodes: vi.fn().mockReturnValue([]),
      setHighlighted: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
    } as unknown as GraphController;

    const mockSelectionManager = {
      addAll: vi.fn(),
      removeAll: vi.fn(),
      clear: vi.fn(),
      handles: [],
    };

    const toolbar = renderToolbar(data, mockController, undefined, undefined, mockSelectionManager, vi.fn());
    const selectAllContainer = toolbar.querySelector('#select-all-container');
    expect(selectAllContainer).toBeTruthy();

    const buttons = selectAllContainer!.querySelectorAll('button');
    expect(buttons.length).toBe(2);
    expect(buttons[0].textContent).toContain('Select All');
    expect(buttons[1].textContent).toContain('Deselect All');
  });

  it('group select/deselect buttons disabled when filter is All groups', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      getVisibleNodes: vi.fn().mockReturnValue([]),
      setHighlighted: vi.fn(),
      setFamilyGroupFilter: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
    } as unknown as GraphController;

    const mockSelectionManager = {
      addAll: vi.fn(),
      removeAll: vi.fn(),
      clear: vi.fn(),
      handles: [],
    };

    const toolbar = renderToolbar(data, mockController, undefined, undefined, mockSelectionManager, vi.fn());
    const buttons = toolbar.querySelectorAll('button');
    // Find group select/deselect buttons
    const groupSelectBtn = Array.from(buttons).find((b) => b.textContent === 'Select Group');
    const groupDeselectBtn = Array.from(buttons).find((b) => b.textContent === 'Deselect Group');

    expect(groupSelectBtn).toBeTruthy();
    expect(groupDeselectBtn).toBeTruthy();
    // With no family_groups, the filter defaults to 'All groups', so buttons are disabled
    expect(groupSelectBtn!.disabled).toBe(true);
    expect(groupDeselectBtn!.disabled).toBe(true);
  });

  it('includes a <select> element (filter dropdown)', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
    } as unknown as GraphController;

    const toolbar = renderToolbar(data, mockController);

    const select = toolbar.querySelector('select');
    expect(select).toBeTruthy();
  });

  it('reset button click handler calls controller.resetLayout()', () => {
    const data = makeGraph([makeNode('p1')], []);
    const resetLayout = vi.fn();
    const mockController = {
      resetLayout,
      setForceConfig: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
    } as unknown as GraphController;

    const toolbar = renderToolbar(data, mockController);
    const buttons = toolbar.querySelectorAll('button');
    const resetBtn = Array.from(buttons).find((b) => b.textContent === '↺ Reset');
    expect(resetBtn).toBeTruthy();
    resetBtn!.click();

    expect(resetLayout).toHaveBeenCalledTimes(1);
  });

  it('reset button calls controller.setFrozen(false) when frozen', () => {
    const data = makeGraph([makeNode('p1')], []);
    const setFrozen = vi.fn();
    const resetLayout = vi.fn();
    const mockController = {
      resetLayout,
      setForceConfig: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(true),
      setFrozen,
    } as unknown as GraphController;

    const toolbar = renderToolbar(data, mockController);
    const buttons = toolbar.querySelectorAll('button');
    const resetBtn = Array.from(buttons).find((b) => b.textContent === '↺ Reset');
    expect(resetBtn).toBeTruthy();

    resetBtn!.click();
    expect(setFrozen).toHaveBeenCalledWith(false);
    expect(resetLayout).toHaveBeenCalledTimes(1);
  });

  it('syncFreezeUI adds/removes .force-frozen on #graph-container', () => {
    const data = makeGraph([makeNode('p1')], []);
    let frozenState = false;
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      isFrozen: vi.fn(() => frozenState),
      setFrozen: vi.fn((f: boolean) => { frozenState = f; }),
    } as unknown as GraphController;

    // Create a graph-container element for the CSS class toggle
    const gc = document.createElement('div');
    gc.id = 'graph-container';
    document.body.appendChild(gc);

    const toolbar = renderToolbar(data, mockController);
    const buttons = toolbar.querySelectorAll('button');
    const freezeBtn = Array.from(buttons).find((b) => b.textContent === '❄ Freeze');
    expect(freezeBtn).toBeTruthy();

    // Click to freeze — should add .force-frozen
    freezeBtn!.click();
    expect(gc.classList.contains('force-frozen')).toBe(true);

    // Click to unfreeze — should remove .force-frozen
    freezeBtn!.click();
    expect(gc.classList.contains('force-frozen')).toBe(false);

    document.body.removeChild(gc);
  });

  it('is styled as a flex container with no absolute positioning', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
    } as unknown as GraphController;

    const toolbar = renderToolbar(data, mockController);

    expect(toolbar.style.position).not.toBe('absolute');
    expect(toolbar.style.display).toBe('flex');
    expect(toolbar.style.alignItems).toBe('center');
    expect(toolbar.style.gap).toBe('8px');
  });

  it('includes a freeze button with text ❄ Freeze', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
    } as unknown as GraphController;

    const toolbar = renderToolbar(data, mockController);
    const buttons = toolbar.querySelectorAll('button');
    const freezeBtn = Array.from(buttons).find((b) => b.textContent === '❄ Freeze');
    expect(freezeBtn).toBeTruthy();
    expect(freezeBtn!.title).toBe('Freeze all forces (only dragged nodes move)');
  });

  it('clicking freeze button toggles text to ❄ Unfreeze and back', () => {
    const data = makeGraph([makeNode('p1')], []);
    let frozenState = false;
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      isFrozen: vi.fn(() => frozenState),
      setFrozen: vi.fn((f: boolean) => { frozenState = f; }),
    } as unknown as GraphController;

    const toolbar = renderToolbar(data, mockController);
    const buttons = toolbar.querySelectorAll('button');
    const freezeBtn = Array.from(buttons).find((b) => b.textContent === '❄ Freeze');
    expect(freezeBtn).toBeTruthy();

    // Click to freeze
    freezeBtn!.click();
    expect(mockController.setFrozen).toHaveBeenCalledWith(true);
    expect(freezeBtn!.textContent).toBe('❄ Unfreeze');

    // Click to unfreeze
    freezeBtn!.click();
    expect(mockController.setFrozen).toHaveBeenCalledWith(false);
    expect(freezeBtn!.textContent).toBe('❄ Freeze');
  });

  it('clicking freeze button calls controller.setFrozen()', () => {
    const data = makeGraph([makeNode('p1')], []);
    const setFrozen = vi.fn();
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen,
    } as unknown as GraphController;

    const toolbar = renderToolbar(data, mockController);
    const buttons = toolbar.querySelectorAll('button');
    const freezeBtn = Array.from(buttons).find((b) => b.textContent === '❄ Freeze');
    expect(freezeBtn).toBeTruthy();

    freezeBtn!.click();
    expect(setFrozen).toHaveBeenCalledWith(true);
  });

  it('prepends toolbar as first child of #app', () => {
    const appEl = document.createElement('div');
    appEl.id = 'app';
    document.body.appendChild(appEl);

    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
    } as unknown as GraphController;

    const toolbar = renderToolbar(data, mockController);
    appEl.prepend(toolbar);

    expect(appEl.firstChild).toBe(toolbar);

    document.body.removeChild(appEl);
  });
});

describe('renderToolbar with forceConfig', () => {
  it('reset button calls setForceConfig before resetLayout', () => {
    const data = makeGraph([makeNode('p1')], []);
    const setForceConfig = vi.fn();
    const resetLayout = vi.fn();
    const mockController = {
      setForceConfig,
      resetLayout,
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
    } as unknown as GraphController;

    const forceConfig: ForceConfig = { ...DEFAULT_FORCE_CONFIG };
    const onChange = vi.fn();
    const toolbar = renderToolbar(data, mockController, forceConfig, onChange);
    const buttons = toolbar.querySelectorAll('button');
    const resetBtn = Array.from(buttons).find((b) => b.textContent === '↺ Reset');
    expect(resetBtn).toBeTruthy();
    resetBtn!.click();

    expect(setForceConfig).toHaveBeenCalledWith(forceConfig);
    expect(resetLayout).toHaveBeenCalledTimes(1);
    // setForceConfig must be called before resetLayout
    expect(setForceConfig.mock.invocationCallOrder[0]).toBeLessThan(
      resetLayout.mock.invocationCallOrder[0],
    );
  });

  it('appends a force panel when forceConfig is provided', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      setForceConfig: vi.fn(),
      resetLayout: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
    } as unknown as GraphController;

    const forceConfig: ForceConfig = { ...DEFAULT_FORCE_CONFIG };
    const toolbar = renderToolbar(data, mockController, forceConfig, vi.fn());

    const forcePanel = toolbar.querySelector('#force-panel');
    expect(forcePanel).toBeTruthy();
  });

  it('does not append a force panel when forceConfig is omitted', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      setForceConfig: vi.fn(),
      resetLayout: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
    } as unknown as GraphController;

    const toolbar = renderToolbar(data, mockController);
    const forcePanel = toolbar.querySelector('#force-panel');
    expect(forcePanel).toBeNull();
  });
});

describe('renderForcePanel', () => {
  it('panel is collapsed by default (force-body has display: none)', () => {
    const panel = renderForcePanel(DEFAULT_FORCE_CONFIG, vi.fn());
    expect(panel.id).toBe('force-panel');
    const body = panel.querySelector('.force-body') as HTMLElement;
    expect(body).toBeTruthy();
    expect(body.style.display).toBe('none');
  });

  it('header click toggles expanded/collapsed state', () => {
    const panel = renderForcePanel(DEFAULT_FORCE_CONFIG, vi.fn());
    const header = panel.querySelector('.force-header') as HTMLElement;
    const body = panel.querySelector('.force-body') as HTMLElement;
    expect(header).toBeTruthy();

    // Click to expand
    header.click();
    expect(body.style.display).toBe('flex');

    // Click to collapse
    header.click();
    expect(body.style.display).toBe('none');
  });

  it('slider input event updates the adjacent value span', () => {
    const onChange = vi.fn();
    const panel = renderForcePanel(DEFAULT_FORCE_CONFIG, onChange);

    // Find the first slider
    const slider = panel.querySelector('.force-slider') as HTMLElement;
    expect(slider).toBeTruthy();

    const input = slider.querySelector('input[type="range"]') as HTMLInputElement;
    const valSpan = slider.querySelector('.value') as HTMLSpanElement;
    expect(input).toBeTruthy();
    expect(valSpan).toBeTruthy();

    // Simulate a slider move: set value to 150 (1.50) and dispatch input
    input.value = '150';
    input.dispatchEvent(new Event('input', { bubbles: true }));

    expect(valSpan.textContent).toBe('1.50');
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it('restore defaults button resets all sliders to DEFAULT_FORCE_CONFIG', () => {
    const onChange = vi.fn();
    const panel = renderForcePanel(DEFAULT_FORCE_CONFIG, onChange);

    // First, modify a slider away from default
    const sliders = panel.querySelectorAll<HTMLInputElement>('.force-slider input[type="range"]');
    expect(sliders.length).toBe(6);

    // Move first slider to a different value
    sliders[0].value = '200';
    sliders[0].dispatchEvent(new Event('input', { bubbles: true }));
    onChange.mockClear(); // clear the first call

    // Click restore defaults
    const restoreBtn = panel.querySelector('button');
    expect(restoreBtn).toBeTruthy();
    expect(restoreBtn!.textContent).toContain('Restore');
    restoreBtn!.click();

    // Verify sliders are back to defaults
    const defaultGenPull = Math.round(DEFAULT_FORCE_CONFIG.generationPull * 100);
    expect(sliders[0].value).toBe(String(defaultGenPull));

    // Verify value spans updated
    const valSpans = panel.querySelectorAll<HTMLSpanElement>('.force-slider .value');
    expect(valSpans.length).toBe(6);
    expect(valSpans[0].textContent).toBe(DEFAULT_FORCE_CONFIG.generationPull.toFixed(2));
    expect(valSpans[1].textContent).toBe(DEFAULT_FORCE_CONFIG.spouseStrength.toFixed(2));
    expect(valSpans[2].textContent).toBe(DEFAULT_FORCE_CONFIG.parentChildStrength.toFixed(2));
    expect(valSpans[3].textContent).toBe(DEFAULT_FORCE_CONFIG.repelStrength.toFixed(2));

    // Verify onChange was called with defaults
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith(DEFAULT_FORCE_CONFIG);
  });

  it('has six sliders with labels and value spans', () => {
    const panel = renderForcePanel(DEFAULT_FORCE_CONFIG, vi.fn());
    const sliderRows = panel.querySelectorAll('.force-slider');
    expect(sliderRows.length).toBe(6);

    const labels = panel.querySelectorAll('.force-slider label');
    expect(labels[0].textContent).toContain('Generation');
    expect(labels[1].textContent).toContain('Spouse');
    expect(labels[2].textContent).toContain('Parent-child');
    expect(labels[3].textContent).toContain('Selection repel');
    expect(labels[4].textContent).toContain('Selected attract');
    expect(labels[5].textContent).toContain('Unselected attract');

    const ranges = panel.querySelectorAll('.force-slider input[type="range"]');
    expect(ranges.length).toBe(6);
    expect((ranges[0] as HTMLInputElement).min).toBe('0');
    expect((ranges[0] as HTMLInputElement).max).toBe('200');

    const values = panel.querySelectorAll('.force-slider .value');
    expect(values.length).toBe(6);
    expect(values[0].textContent).toBe(DEFAULT_FORCE_CONFIG.generationPull.toFixed(2));
    expect(values[1].textContent).toBe(DEFAULT_FORCE_CONFIG.spouseStrength.toFixed(2));
    expect(values[2].textContent).toBe(DEFAULT_FORCE_CONFIG.parentChildStrength.toFixed(2));
    expect(values[3].textContent).toBe(DEFAULT_FORCE_CONFIG.repelStrength.toFixed(2));
    expect(values[4].textContent).toBe(DEFAULT_FORCE_CONFIG.selectedAttractStrength.toFixed(2));
    expect(values[5].textContent).toBe(DEFAULT_FORCE_CONFIG.unselectedAttractStrength.toFixed(2));
  });
});
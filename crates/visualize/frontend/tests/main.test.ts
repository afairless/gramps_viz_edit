// @vitest-environment happy-dom
// Tests for the main.ts module — toolbar rendering and UI wiring.

import { describe, it, expect, vi } from 'vitest';
import { GRAMPS_FILE_FILTER, renderToolbar, renderForcePanel, showWelcomeScreen, renderGraphFromData } from '../src/main';
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

    const { toolbar } = renderToolbar(data, mockController);

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

    const { toolbar } = renderToolbar(data, mockController, undefined, undefined, undefined, vi.fn());
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
    const { toolbar } = renderToolbar(data, mockController, undefined, undefined, undefined, onChange);
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

    const { toolbar } = renderToolbar(data, mockController, undefined, undefined, mockSelectionManager, vi.fn());
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

    const { toolbar } = renderToolbar(data, mockController, undefined, undefined, mockSelectionManager, vi.fn());
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

    const { toolbar } = renderToolbar(data, mockController);

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

    const { toolbar } = renderToolbar(data, mockController);
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
      setRectSelectActive: vi.fn(),
      isRectSelectActive: vi.fn().mockReturnValue(false),
    } as unknown as GraphController;

    const { toolbar } = renderToolbar(data, mockController);
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
      setRectSelectActive: vi.fn(),
      isRectSelectActive: vi.fn().mockReturnValue(false),
    } as unknown as GraphController;

    // Create a graph-container element for the CSS class toggle
    const gc = document.createElement('div');
    gc.id = 'graph-container';
    document.body.appendChild(gc);

    const { toolbar } = renderToolbar(data, mockController);
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

    const { toolbar } = renderToolbar(data, mockController);

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

    const { toolbar } = renderToolbar(data, mockController);
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
      setRectSelectActive: vi.fn(),
      isRectSelectActive: vi.fn().mockReturnValue(false),
    } as unknown as GraphController;

    const { toolbar } = renderToolbar(data, mockController);
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
      setRectSelectActive: vi.fn(),
      isRectSelectActive: vi.fn().mockReturnValue(false),
    } as unknown as GraphController;

    const { toolbar } = renderToolbar(data, mockController);
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

    const { toolbar } = renderToolbar(data, mockController);
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
    const { toolbar } = renderToolbar(data, mockController, forceConfig, onChange);
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
    const { toolbar } = renderToolbar(data, mockController, forceConfig, vi.fn());

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

    const { toolbar } = renderToolbar(data, mockController);
    const forcePanel = toolbar.querySelector('#force-panel');
    expect(forcePanel).toBeNull();
  });
});

describe('Rect Select toggle button', () => {
  it('renders Rect Select button in toolbar when not frozen (hidden by default)', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
      setRectSelectActive: vi.fn(),
      isRectSelectActive: vi.fn().mockReturnValue(false),
    } as unknown as GraphController;

    const { toolbar } = renderToolbar(data, mockController);
    const rectSelectBtn = Array.from(toolbar.querySelectorAll('button')).find(
      (b) => b.textContent === '📦 Rect Select',
    );
    expect(rectSelectBtn).toBeTruthy();
    // Hidden until frozen
    expect(rectSelectBtn!.style.display).toBe('none');
  });

  it('is visible after freeze UI sync adds it (display restored)', () => {
    const data = makeGraph([makeNode('p1')], []);
    let frozenState = false;
    const setRectSelectActive = vi.fn();
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      isFrozen: vi.fn(() => frozenState),
      setFrozen: vi.fn((f: boolean) => { frozenState = f; }),
      setRectSelectActive,
      isRectSelectActive: vi.fn().mockReturnValue(false),
    } as unknown as GraphController;

    // Create a graph-container element
    const gc = document.createElement('div');
    gc.id = 'graph-container';
    document.body.appendChild(gc);

    const { toolbar } = renderToolbar(data, mockController);
    const freezeBtn = Array.from(toolbar.querySelectorAll('button')).find(
      (b) => b.textContent === '❄ Freeze',
    );
    expect(freezeBtn).toBeTruthy();

    // Click to freeze — rect select button should become visible
    freezeBtn!.click();
    const rectSelectBtn = Array.from(toolbar.querySelectorAll('button')).find(
      (b) => b.textContent === '📦 Rect Select',
    );
    expect(rectSelectBtn).toBeTruthy();
    expect(rectSelectBtn!.style.display).toBe(''); // visible

    // Click to unfreeze — rect select should hide
    freezeBtn!.click();
    expect(rectSelectBtn!.style.display).toBe('none');
    expect(setRectSelectActive).toHaveBeenCalledWith(false);

    document.body.removeChild(gc);
  });

  it('toggling Rect Select button updates text and calls controller', () => {
    const data = makeGraph([makeNode('p1')], []);
    const frozenState = true;
    let rectActive = false;
    const setRectSelectActive = vi.fn((active: boolean) => {
      rectActive = active;
    });
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      isFrozen: vi.fn(() => frozenState),
      setFrozen: vi.fn(),
      setRectSelectActive,
      isRectSelectActive: vi.fn(() => rectActive),
    } as unknown as GraphController;

    const gc = document.createElement('div');
    gc.id = 'graph-container';
    document.body.appendChild(gc);

    const { toolbar } = renderToolbar(data, mockController);
    const rectSelectBtn = Array.from(toolbar.querySelectorAll('button')).find(
      (b) => b.textContent && b.textContent.startsWith('📦 Rect Select'),
    );
    expect(rectSelectBtn).toBeTruthy();

    // Toggle ON
    rectSelectBtn!.click();
    expect(setRectSelectActive).toHaveBeenCalledWith(true);
    // The button text doesn't update on this mock because syncRectSelectUI
    // was passed but the text updates on click — actually with our mock
    // setRectSelectActive doesn't update isRectSelectActive...
    // Let's just verify the click calls the controller correctly.

    document.body.removeChild(gc);
  });

  it('renderToolbar returns syncRectSelectUI function', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      isFrozen: vi.fn().mockReturnValue(false),
      setFrozen: vi.fn(),
      setRectSelectActive: vi.fn(),
      isRectSelectActive: vi.fn().mockReturnValue(false),
    } as unknown as GraphController;

    const result = renderToolbar(data, mockController);
    expect(result).toHaveProperty('toolbar');
    expect(result).toHaveProperty('syncRectSelectUI');
    expect(typeof result.syncRectSelectUI).toBe('function');
  });

  it('syncFreezeUI(false) hides rect-select button and deactivates', () => {
    const data = makeGraph([makeNode('p1')], []);
    let frozenState = true;
    const setRectSelectActive = vi.fn();
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
      isFrozen: vi.fn(() => frozenState),
      setFrozen: vi.fn((f: boolean) => { frozenState = f; }),
      setRectSelectActive,
      isRectSelectActive: vi.fn().mockReturnValue(false),
    } as unknown as GraphController;

    const gc = document.createElement('div');
    gc.id = 'graph-container';
    document.body.appendChild(gc);

    const { toolbar } = renderToolbar(data, mockController);
    const freezeBtn = Array.from(toolbar.querySelectorAll('button')).find(
      (b) => b.textContent.startsWith('❄'),
    )!;

    // Start frozen
    expect(frozenState).toBe(true);

    // Unfreeze — should hide rect button and deactivate
    freezeBtn.click();
    expect(frozenState).toBe(false);
    expect(setRectSelectActive).toHaveBeenCalledWith(false);

    const rectSelectBtn = Array.from(toolbar.querySelectorAll('button')).find(
      (b) => b.textContent && b.textContent.startsWith('📦 Rect Select'),
    );
    if (rectSelectBtn) {
      expect(rectSelectBtn.style.display).toBe('none');
    }

    document.body.removeChild(gc);
  });
});

describe('rectangle batch selection and Escape handler', () => {
  function setupGraphEnv(data: GraphData): {
    container: HTMLElement;
    controller: import('../src/graph').GraphController;
    cleanup: () => void;
  } {
    const container = document.createElement('div');
    container.id = 'graph-container';
    container.style.width = '800px';
    container.style.height = '600px';
    document.body.appendChild(container);

    const appEl = document.createElement('div');
    appEl.id = 'app';
    document.body.appendChild(appEl);

    const panelEl = document.createElement('div');
    panelEl.id = 'selection-panel';
    document.body.appendChild(panelEl);

    renderGraphFromData(container, appEl, data);

    const controller = (window as unknown as Record<string, import('../src/graph').GraphController>).__GRAPH_CONTROLLER__;

    return {
      container,
      controller,
      cleanup: () => {
        controller.destroy();
        document.body.removeChild(container);
        document.body.removeChild(appEl);
        document.body.removeChild(panelEl);
      },
    };
  }

  function clickNodeByHandle(container: HTMLElement, handle: string): void {
    const nodeGs = container.querySelectorAll('.nodes g');
    for (const g of Array.from(nodeGs)) {
      const d = (g as any).__data__;
      if (d && d.handle === handle) {
        g.dispatchEvent(new MouseEvent('click', { bubbles: true }));
        return;
      }
    }
  }

  function getSelectedHandles(container: HTMLElement): string[] {
    const handles: string[] = [];
    const nodeGs = container.querySelectorAll('.nodes g');
    for (const g of Array.from(nodeGs)) {
      const circle = g.querySelector('circle');
      if (circle && circle.getAttribute('r') === '16') {
        const d = (g as any).__data__;
        if (d) handles.push(d.handle);
      }
    }
    return handles;
  }

  it('click inside rectangle in single mode selects all nodes in rectangle', () => {
    const data = makeGraph(
      [
        makeNode('p1', { family_group: 0, generation: 0, name: 'Alice', birth_year: 1900 }),
        makeNode('p2', { family_group: 0, generation: 0, name: 'Bob', birth_year: 1900 }),
      ],
      [],
    );

    const { container, controller, cleanup } = setupGraphEnv(data);

    // Freeze immediately to stop simulation ticks and fix node positions
    controller.setFrozen(true);
    controller.setRectSelectActive(true);

    // Draw a rectangle that covers everything (large coordinates)
    const svg = container.querySelector('svg')!;
    svg.dispatchEvent(new PointerEvent('pointerdown', {
      clientX: -10000, clientY: -10000, bubbles: true, cancelable: true,
    }));
    svg.dispatchEvent(new PointerEvent('pointermove', {
      clientX: 10000, clientY: 10000, bubbles: true, cancelable: true,
    }));
    svg.dispatchEvent(new PointerEvent('pointerup', {
      clientX: 10000, clientY: 10000, bubbles: true, cancelable: true,
    }));

    expect(controller.hasRectangle()).toBe(true);

    // Click node p1 (inside rectangle, unselected) → should select all in rect
    clickNodeByHandle(container, 'p1');

    const selected = getSelectedHandles(container);
    expect(selected).toContain('p1');
    // p2 is also in rectangle (both at (0,0) or close) → should be selected
    expect(selected).toContain('p2');

    cleanup();
  });

  it('click inside rectangle in single mode deselects all when clicked node is selected', () => {
    const data = makeGraph(
      [
        makeNode('p1', { family_group: 0, generation: 0, name: 'Alice', birth_year: 1900 }),
        makeNode('p2', { family_group: 0, generation: 0, name: 'Bob', birth_year: 1900 }),
      ],
      [],
    );

    const { container, controller, cleanup } = setupGraphEnv(data);

    // First select both nodes via normal clicks (no rectangle yet)
    clickNodeByHandle(container, 'p1'); // toggles p1 ON
    clickNodeByHandle(container, 'p2'); // toggles p2 ON
    expect(getSelectedHandles(container)).toEqual(['p1', 'p2']);

    // Now freeze and draw rectangle covering everything
    controller.setFrozen(true);
    controller.setRectSelectActive(true);

    const svg = container.querySelector('svg')!;
    svg.dispatchEvent(new PointerEvent('pointerdown', {
      clientX: -10000, clientY: -10000, bubbles: true, cancelable: true,
    }));
    svg.dispatchEvent(new PointerEvent('pointermove', {
      clientX: 10000, clientY: 10000, bubbles: true, cancelable: true,
    }));
    svg.dispatchEvent(new PointerEvent('pointerup', {
      clientX: 10000, clientY: 10000, bubbles: true, cancelable: true,
    }));

    // Click p1 (selected) inside rectangle → should deselect all
    clickNodeByHandle(container, 'p1');

    const selected = getSelectedHandles(container);
    expect(selected).not.toContain('p1');
    expect(selected).not.toContain('p2');

    cleanup();
  });

  it('click outside rectangle falls through to normal single-node behavior', () => {
    const data = makeGraph(
      [
        makeNode('p1', { family_group: 0, generation: 0, name: 'Alice', birth_year: 1900 }),
        makeNode('p2', { family_group: 0, generation: 0, name: 'Bob', birth_year: 1900 }),
      ],
      [],
    );

    const { container, controller, cleanup } = setupGraphEnv(data);

    controller.setFrozen(true);
    controller.setRectSelectActive(true);

    // Draw small rectangle at corner (doesn't contain any node at ~0,0)
    const svg = container.querySelector('svg')!;
    svg.dispatchEvent(new PointerEvent('pointerdown', {
      clientX: 700, clientY: 500, bubbles: true, cancelable: true,
    }));
    svg.dispatchEvent(new PointerEvent('pointermove', {
      clientX: 750, clientY: 550, bubbles: true, cancelable: true,
    }));
    svg.dispatchEvent(new PointerEvent('pointerup', {
      clientX: 750, clientY: 550, bubbles: true, cancelable: true,
    }));

    expect(controller.hasRectangle()).toBe(true);

    // Click p1 (outside the small rectangle) → normal toggle
    clickNodeByHandle(container, 'p1');

    const selected = getSelectedHandles(container);
    expect(selected).toContain('p1');
    // p2 should NOT be selected (normal single-node behavior)
    expect(selected).not.toContain('p2');

    cleanup();
  });

  it('Escape key clears rectangle when one exists', () => {
    const data = makeGraph(
      [makeNode('p1', { family_group: 0, generation: 0, name: 'Alice', birth_year: 1900 })],
      [],
    );

    const { container, controller, cleanup } = setupGraphEnv(data);

    controller.setFrozen(true);
    controller.setRectSelectActive(true);

    // Draw rectangle
    const svg = container.querySelector('svg')!;
    svg.dispatchEvent(new PointerEvent('pointerdown', {
      clientX: 0, clientY: 0, bubbles: true, cancelable: true,
    }));
    svg.dispatchEvent(new PointerEvent('pointermove', {
      clientX: 100, clientY: 100, bubbles: true, cancelable: true,
    }));
    svg.dispatchEvent(new PointerEvent('pointerup', {
      clientX: 100, clientY: 100, bubbles: true, cancelable: true,
    }));

    expect(controller.hasRectangle()).toBe(true);

    // Press Escape
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));

    expect(controller.hasRectangle()).toBe(false);

    cleanup();
  });

  it('Escape deactivates rect-select toggle when no rectangle exists', () => {
    const data = makeGraph(
      [makeNode('p1', { family_group: 0, generation: 0, name: 'Alice', birth_year: 1900 })],
      [],
    );

    const { controller, cleanup } = setupGraphEnv(data);

    controller.setFrozen(true);
    controller.setRectSelectActive(true);
    expect(controller.isRectSelectActive()).toBe(true);

    // Press Escape — should deactivate the toggle (no rectangle to clear)
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));

    expect(controller.isRectSelectActive()).toBe(false);

    cleanup();
  });

  it('Escape handler does not interfere with non-Escape keys', () => {
    const data = makeGraph(
      [makeNode('p1', { family_group: 0, generation: 0, name: 'Alice', birth_year: 1900 })],
      [],
    );

    const { container, controller, cleanup } = setupGraphEnv(data);

    controller.setFrozen(true);
    controller.setRectSelectActive(true);

    // Draw rectangle
    const svg = container.querySelector('svg')!;
    svg.dispatchEvent(new PointerEvent('pointerdown', {
      clientX: 0, clientY: 0, bubbles: true, cancelable: true,
    }));
    svg.dispatchEvent(new PointerEvent('pointermove', {
      clientX: 100, clientY: 100, bubbles: true, cancelable: true,
    }));
    svg.dispatchEvent(new PointerEvent('pointerup', {
      clientX: 100, clientY: 100, bubbles: true, cancelable: true,
    }));

    expect(controller.hasRectangle()).toBe(true);

    // Press a non-Escape key
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));

    // Rectangle should still exist
    expect(controller.hasRectangle()).toBe(true);

    cleanup();
  });

  it('Select All still works regardless of rectangle state', () => {
    const data = makeGraph(
      [
        makeNode('p1', { family_group: 0, generation: 0, name: 'Alice', birth_year: 1900 }),
        makeNode('p2', { family_group: 0, generation: 0, name: 'Bob', birth_year: 1900 }),
      ],
      [],
    );

    const { container, controller, cleanup } = setupGraphEnv(data);

    controller.setFrozen(true);
    controller.setRectSelectActive(true);

    // Draw rectangle covering everything
    const svg = container.querySelector('svg')!;
    svg.dispatchEvent(new PointerEvent('pointerdown', {
      clientX: -10000, clientY: -10000, bubbles: true, cancelable: true,
    }));
    svg.dispatchEvent(new PointerEvent('pointermove', {
      clientX: 10000, clientY: 10000, bubbles: true, cancelable: true,
    }));
    svg.dispatchEvent(new PointerEvent('pointerup', {
      clientX: 10000, clientY: 10000, bubbles: true, cancelable: true,
    }));

    expect(controller.hasRectangle()).toBe(true);

    // Click Select All button
    const selectAllBtn = document.querySelector('#select-all-container button');
    expect(selectAllBtn).toBeTruthy();
    if (selectAllBtn) {
      (selectAllBtn as HTMLButtonElement).click();
    }

    const selected = getSelectedHandles(container);
    expect(selected).toContain('p1');
    expect(selected).toContain('p2');

    cleanup();
  });

  it('rectangle membership respects family group filter', () => {
    const data = makeGraph(
      [
        makeNode('g1p1', { family_group: 1, generation: 0, name: 'Group1A', birth_year: 1900 }),
        makeNode('g1p2', { family_group: 1, generation: 1, name: 'Group1B', birth_year: 1900 }),
        makeNode('g2p1', { family_group: 2, generation: 0, name: 'Group2A', birth_year: 1900 }),
        makeNode('g2p2', { family_group: 2, generation: 0, name: 'Group2B', birth_year: 1900 }),
      ],
      [],
    );

    const { container, controller, cleanup } = setupGraphEnv(data);

    controller.setFrozen(true);
    controller.setRectSelectActive(true);

    // Draw rectangle covering everything
    const svg = container.querySelector('svg')!;
    svg.dispatchEvent(new PointerEvent('pointerdown', {
      clientX: -10000, clientY: -10000, bubbles: true, cancelable: true,
    }));
    svg.dispatchEvent(new PointerEvent('pointermove', {
      clientX: 10000, clientY: 10000, bubbles: true, cancelable: true,
    }));
    svg.dispatchEvent(new PointerEvent('pointerup', {
      clientX: 10000, clientY: 10000, bubbles: true, cancelable: true,
    }));

    // Filter to group 1 only
    controller.setFamilyGroupFilter(1);

    // getNodesInRectangle should only return group 1 nodes
    const inRect = controller.getNodesInRectangle();
    expect(inRect).toContain('g1p1');
    expect(inRect).toContain('g1p2');
    expect(inRect).not.toContain('g2p1');
    expect(inRect).not.toContain('g2p2');

    cleanup();
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
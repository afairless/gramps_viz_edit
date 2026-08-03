// @vitest-environment happy-dom
// Tests for the main.ts module — toolbar rendering and UI wiring.

import { describe, it, expect, vi } from 'vitest';
import { renderToolbar, renderForcePanel } from '../src/main';
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

describe('renderToolbar', () => {
  it('returns a toolbar element with a reset button containing ↺', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
    } as unknown as GraphController;

    const toolbar = renderToolbar(data, mockController);

    expect(toolbar.id).toBe('toolbar');
    const resetBtn = toolbar.querySelector('button');
    expect(resetBtn).toBeTruthy();
    expect(resetBtn!.textContent).toContain('↺');
    expect(resetBtn!.title).toBe('Reset node positions to force-directed layout');
  });

  it('includes a <select> element (filter dropdown)', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
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
    } as unknown as GraphController;

    const toolbar = renderToolbar(data, mockController);
    const resetBtn = toolbar.querySelector('button')!;
    resetBtn.click();

    expect(resetLayout).toHaveBeenCalledTimes(1);
  });

  it('is styled as a flex container with absolute positioning', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = {
      resetLayout: vi.fn(),
      setForceConfig: vi.fn(),
    } as unknown as GraphController;

    const toolbar = renderToolbar(data, mockController);

    expect(toolbar.style.position).toBe('absolute');
    expect(toolbar.style.display).toBe('flex');
    expect(toolbar.style.alignItems).toBe('center');
    expect(toolbar.style.gap).toBe('8px');
    expect(toolbar.style.top).toBe('20px');
    expect(toolbar.style.left).toBe('20px');
    expect(toolbar.style.zIndex).toBe('500');
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
    } as unknown as GraphController;

    const forceConfig: ForceConfig = { ...DEFAULT_FORCE_CONFIG };
    const onChange = vi.fn();
    const toolbar = renderToolbar(data, mockController, forceConfig, onChange);
    const resetBtn = toolbar.querySelector('button')!;
    resetBtn.click();

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
    expect(sliders.length).toBe(3);

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
    expect(valSpans.length).toBe(3);
    expect(valSpans[0].textContent).toBe(DEFAULT_FORCE_CONFIG.generationPull.toFixed(2));
    expect(valSpans[1].textContent).toBe(DEFAULT_FORCE_CONFIG.spouseStrength.toFixed(2));
    expect(valSpans[2].textContent).toBe(DEFAULT_FORCE_CONFIG.parentChildStrength.toFixed(2));

    // Verify onChange was called with defaults
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith(DEFAULT_FORCE_CONFIG);
  });

  it('has three sliders with labels and value spans', () => {
    const panel = renderForcePanel(DEFAULT_FORCE_CONFIG, vi.fn());
    const sliderRows = panel.querySelectorAll('.force-slider');
    expect(sliderRows.length).toBe(3);

    const labels = panel.querySelectorAll('.force-slider label');
    expect(labels[0].textContent).toContain('Generation');
    expect(labels[1].textContent).toContain('Spouse');
    expect(labels[2].textContent).toContain('Parent-child');

    const ranges = panel.querySelectorAll('.force-slider input[type="range"]');
    expect(ranges.length).toBe(3);
    expect((ranges[0] as HTMLInputElement).min).toBe('0');
    expect((ranges[0] as HTMLInputElement).max).toBe('200');

    const values = panel.querySelectorAll('.force-slider .value');
    expect(values.length).toBe(3);
    expect(values[0].textContent).toBe(DEFAULT_FORCE_CONFIG.generationPull.toFixed(2));
    expect(values[1].textContent).toBe(DEFAULT_FORCE_CONFIG.spouseStrength.toFixed(2));
    expect(values[2].textContent).toBe(DEFAULT_FORCE_CONFIG.parentChildStrength.toFixed(2));
  });
});
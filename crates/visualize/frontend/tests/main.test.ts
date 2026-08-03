// @vitest-environment happy-dom
// Tests for the main.ts module — toolbar rendering and UI wiring.

import { describe, it, expect, vi } from 'vitest';
import { renderToolbar } from '../src/main';
import type { GraphController } from '../src/graph';
import type { GraphData, PersonNode, FamilyLink } from '../src/types';

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
// @vitest-environment happy-dom
// Tests for the StatsPanel component.

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { StatsPanel } from '../src/stats-panel';
import type { StatsReport } from '../src/types';

function makeReport(overrides: Partial<StatsReport> = {}): StatsReport {
  return {
    file: 'test.gramps',
    counts: {
      people: 42,
      families: 10,
      events: 57,
      places: 12,
      sources: 3,
      citations: 9,
      repositories: 1,
      media: 4,
      notes: 15,
      tags: 2,
    },
    family_size_distribution: { '1': 2, '2': 3, '3': 5, '4': 1 },
    family_group_distribution: { '1': 3, '3': 1, '5': 1 },
    family_group_generation_table: {},
    people_not_in_family: 8,
    dangling_refs: 0,
    warnings: [],
    ...overrides,
  };
}

describe('StatsPanel', () => {
  let panel: StatsPanel;

  beforeEach(() => {
    panel = new StatsPanel();
    const el = panel.create();
    document.body.appendChild(el);
    // The tab element is created by create() and stored internally.
    // We need it in the DOM for toggle/destroy tests.
    const tab = document.querySelector('.stats-tab');
    if (tab) {
      document.body.appendChild(tab);
    }
  });

  afterEach(() => {
    panel.destroy();
  });

  describe('create()', () => {
    it('produces correct DOM structure', () => {
      const el = document.getElementById('stats-panel')!;
      expect(el.id).toBe('stats-panel');

      // Header with title and close button
      const header = el.querySelector('.stats-panel-header');
      expect(header).toBeTruthy();
      expect(header!.querySelector('span')!.textContent).toBe('File Statistics');

      const closeBtn = header!.querySelector('.stats-close-btn');
      expect(closeBtn).toBeTruthy();
      expect(closeBtn!.textContent).toBe('\u00D7');

      // Body
      const body = el.querySelector('.stats-panel-body');
      expect(body).toBeTruthy();

      // Tab should be in the DOM
      const tab = document.querySelector('.stats-tab');
      expect(tab).toBeTruthy();
      expect(tab!.textContent).toBe('Stats');
      expect((tab as HTMLElement).style.display).toBe('none');
    });
  });

  describe('render()', () => {
    it('populates all sections correctly', () => {
      const report = makeReport();
      panel.render(report);

      const body = document.querySelector('.stats-panel-body')!;
      expect(body.children.length).toBe(4); // 4 sections

      // 1. Object counts: all 10 types shown
      const sections = body.querySelectorAll('.stats-section');
      expect(sections.length).toBe(4);

      const countSection = sections[0] as HTMLElement;
      expect(countSection.querySelector('h3')!.textContent).toBe('Object counts');
      const countRows = countSection.querySelectorAll('.stats-table tr');
      expect(countRows.length).toBe(10);
      const labels = ['People', 'Families', 'Events', 'Places', 'Sources', 'Citations', 'Repositories', 'Media', 'Notes', 'Tags'];
      labels.forEach((label, i) => {
        expect(countRows[i].querySelector('.stats-label')!.textContent).toBe(label);
      });
      // First value is 42 (people)
      expect(countRows[0].querySelector('.stats-value')!.textContent).toBe('42');
    });

    it('renders family size distribution correctly', () => {
      const report = makeReport();
      panel.render(report);

      const sections = document.querySelectorAll('.stats-section');
      const famSizeSection = sections[1] as HTMLElement;
      expect(famSizeSection.querySelector('h3')!.textContent).toBe('Family size dist.');
      const rows = famSizeSection.querySelectorAll('.stats-table tr');
      expect(rows.length).toBe(4);
      expect(rows[0].querySelector('.stats-label')!.textContent).toBe('size 1');
      expect(rows[0].querySelector('.stats-value')!.textContent).toBe('2 families');
      expect(rows[1].querySelector('.stats-label')!.textContent).toBe('size 2');
      expect(rows[1].querySelector('.stats-value')!.textContent).toBe('3 families');
    });

    it('renders family group distribution correctly', () => {
      const report = makeReport();
      panel.render(report);

      const sections = document.querySelectorAll('.stats-section');
      const famGroupSection = sections[2] as HTMLElement;
      expect(famGroupSection.querySelector('h3')!.textContent).toBe('Family group dist.');
      const rows = famGroupSection.querySelectorAll('.stats-table tr');
      expect(rows.length).toBe(3);
      expect(rows[0].querySelector('.stats-label')!.textContent).toBe('size 1');
      expect(rows[0].querySelector('.stats-value')!.textContent).toBe('3 groups');
      expect(rows[1].querySelector('.stats-label')!.textContent).toBe('size 3');
      expect(rows[1].querySelector('.stats-value')!.textContent).toBe('1 group');
    });

    it('renders data quality section correctly', () => {
      const report = makeReport();
      panel.render(report);

      const sections = document.querySelectorAll('.stats-section');
      const dqSection = sections[3] as HTMLElement;
      expect(dqSection.querySelector('h3')!.textContent).toBe('Data quality');

      const dqRows = dqSection.querySelectorAll('.stats-table tr');
      expect(dqRows.length).toBe(2);
      expect(dqRows[0].querySelector('.stats-label')!.textContent).toBe('Not in family');
      expect(dqRows[0].querySelector('.stats-value')!.textContent).toBe('8');
      expect(dqRows[1].querySelector('.stats-label')!.textContent).toBe('Dangling refs');
      expect(dqRows[1].querySelector('.stats-value')!.textContent).toBe('0');
    });

    it('shows "None" for empty warnings list', () => {
      const report = makeReport({ warnings: [] });
      panel.render(report);

      const sections = document.querySelectorAll('.stats-section');
      const dqSection = sections[3] as HTMLElement;
      const emptyEl = dqSection.querySelector('.stats-empty');
      expect(emptyEl).toBeTruthy();
      expect(emptyEl!.textContent).toBe('None');
    });

    it('shows warning list items when warnings are present', () => {
      const report = makeReport({ warnings: ['No families found', 'Orphaned reference detected'] });
      panel.render(report);

      const sections = document.querySelectorAll('.stats-section');
      const dqSection = sections[3] as HTMLElement;
      const warningItems = dqSection.querySelectorAll('.stats-warning');
      expect(warningItems.length).toBe(2);
      expect(warningItems[0].textContent).toContain('No families found');
      expect(warningItems[1].textContent).toContain('Orphaned reference detected');
    });

    it('handles empty family size distribution', () => {
      const report = makeReport({ family_size_distribution: {} });
      panel.render(report);

      const sections = document.querySelectorAll('.stats-section');
      const famSizeSection = sections[1] as HTMLElement;
      const empty = famSizeSection.querySelector('.stats-empty');
      expect(empty).toBeTruthy();
      expect(empty!.textContent).toBe('None');
    });

    it('handles empty family group distribution', () => {
      const report = makeReport({ family_group_distribution: {} });
      panel.render(report);

      const sections = document.querySelectorAll('.stats-section');
      const famGroupSection = sections[2] as HTMLElement;
      const empty = famGroupSection.querySelector('.stats-empty');
      expect(empty).toBeTruthy();
      expect(empty!.textContent).toBe('None');
    });

    it('renders single-item family size distribution with correct pluralization', () => {
      const report = makeReport({ family_size_distribution: { '1': 1 } });
      panel.render(report);

      const sections = document.querySelectorAll('.stats-section');
      const famSizeSection = sections[1] as HTMLElement;
      const row = famSizeSection.querySelector('.stats-table tr')!;
      expect(row.querySelector('.stats-value')!.textContent).toBe('1 family');
    });
  });

  describe('toggle()', () => {
    it('hides panel and shows tab on first toggle', () => {
      const panelEl = document.getElementById('stats-panel')!;
      const tabEl = document.querySelector('.stats-tab') as HTMLElement;

      panel.toggle();

      expect(panelEl.style.display).toBe('none');
      expect(tabEl.style.display).toBe('block');
    });

    it('shows panel and hides tab on second toggle', () => {
      const panelEl = document.getElementById('stats-panel')!;
      const tabEl = document.querySelector('.stats-tab') as HTMLElement;

      panel.toggle(); // collapse
      panel.toggle(); // expand

      expect(panelEl.style.display).toBe('flex');
      expect(tabEl.style.display).toBe('none');
    });
  });

  describe('renderError()', () => {
    it('displays error message in the panel body', () => {
      panel.renderError('Failed to load data');

      const body = document.querySelector('.stats-panel-body')!;
      const errorEl = body.querySelector('.stats-error');
      expect(errorEl).toBeTruthy();
      expect(errorEl!.textContent).toBe('Failed to load data');
    });

    it('replaces previous content when renderError is called twice', () => {
      panel.render(makeReport());
      panel.renderError('New error');

      const body = document.querySelector('.stats-panel-body')!;
      const errorEl = body.querySelector('.stats-error');
      expect(errorEl).toBeTruthy();
      expect(errorEl!.textContent).toBe('New error');
      expect(body.querySelectorAll('.stats-section').length).toBe(0);
    });
  });

  describe('destroy()', () => {
    it('removes panel and tab from DOM', () => {
      expect(document.getElementById('stats-panel')).toBeTruthy();
      expect(document.querySelector('.stats-tab')).toBeTruthy();

      panel.destroy();

      expect(document.getElementById('stats-panel')).toBeNull();
      expect(document.querySelector('.stats-tab')).toBeNull();
    });

    it('is safe to call destroy() multiple times', () => {
      panel.destroy();
      // Second call should not throw
      panel.destroy();
    });

    it('is safe to call destroy() without create()', () => {
      // destroy() on a non-created panel should not throw
      panel.destroy();
    });
  });

  describe('main-row integration', () => {
    it('appends tab to #main-row when available', () => {
      const mainRow = document.createElement('div');
      mainRow.id = 'main-row';
      document.body.appendChild(mainRow);

      const newPanel = new StatsPanel();
      const el = newPanel.create();
      document.body.appendChild(el);

      const tab = mainRow.querySelector('.stats-tab')!;
      expect(tab).toBeTruthy();
      expect(tab.parentElement).toBe(mainRow);

      newPanel.destroy();
      document.body.removeChild(mainRow);
    });

    it('inserts panel into #main-row when available', () => {
      const mainRow = document.createElement('div');
      mainRow.id = 'main-row';
      document.body.appendChild(mainRow);

      const newPanel = new StatsPanel();
      const el = newPanel.create();
      mainRow.prepend(el);

      const panel = mainRow.querySelector('#stats-panel');
      expect(panel).toBeTruthy();
      expect(panel!.parentElement).toBe(mainRow);

      newPanel.destroy();
      document.body.removeChild(mainRow);
    });
  });
});
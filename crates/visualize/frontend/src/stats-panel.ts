// Stats panel — right-side collapsible sidebar summarizing file statistics.

import type { StatsReport } from './types';

/**
 * Collapsible right-sidebar panel that displays summary statistics for the
 * currently loaded .gramps file.
 *
 * Sections:
 *   - Object counts (10 primary Gramps types)
 *   - Family size distribution
 *   - Family group distribution
 *   - Data quality warnings
 */
export class StatsPanel {
  private panel: HTMLElement | null = null;
  private body: HTMLElement | null = null;
  private tab: HTMLElement | null = null;
  private expanded = true;

  /** Build the DOM elements for the sidebar panel. */
  create(): HTMLElement {
    // ---- container ----
    const panel = document.createElement('div');
    panel.id = 'stats-panel';

    // ---- header ----
    const header = document.createElement('div');
    header.className = 'stats-panel-header';

    const title = document.createElement('span');
    title.textContent = 'File Statistics';
    header.appendChild(title);

    const closeBtn = document.createElement('button');
    closeBtn.className = 'stats-close-btn';
    closeBtn.textContent = '\u00D7'; // ×
    closeBtn.title = 'Collapse stats panel';
    closeBtn.addEventListener('click', () => this.toggle());
    header.appendChild(closeBtn);

    panel.appendChild(header);

    // ---- body ----
    const body = document.createElement('div');
    body.className = 'stats-panel-body';
    this.body = body;
    panel.appendChild(body);

    // ---- collapsed tab ----
    const tab = document.createElement('div');
    tab.className = 'stats-tab';
    tab.textContent = 'Stats';
    tab.title = 'Show stats panel';
    tab.addEventListener('click', () => this.toggle());
    this.tab = tab;
    // Tab is hidden by default
    tab.style.display = 'none';

    this.panel = panel;
    this.expanded = true;
    // Append the tab to the document body so it's in the DOM for toggle/destroy
    document.body.appendChild(tab);
    return panel;
  }

  /** Show/hide the panel. */
  toggle(): void {
    if (!this.panel || !this.tab) return;
    this.expanded = !this.expanded;
    this.panel.style.display = this.expanded ? 'flex' : 'none';
    this.tab.style.display = this.expanded ? 'none' : 'block';
  }

  /** Populate the panel with data from a StatsReport. */
  render(report: StatsReport): void {
    if (!this.body) return;
    this.body.textContent = '';

    // 1. Object counts
    this.body.appendChild(this.renderObjectCounts(report.counts));

    // 2. Family size distribution
    this.body.appendChild(this.renderFamilySizeDistribution(report.family_size_distribution));

    // 3. Family group distribution
    this.body.appendChild(this.renderFamilyGroupDistribution(report.family_group_distribution));

    // 4. Data quality
    this.body.appendChild(this.renderDataQuality(report));
  }

  /** Show a non-intrusive error message in the panel body. */
  renderError(msg: string): void {
    if (!this.body) return;
    this.body.textContent = '';
    const errEl = document.createElement('div');
    errEl.className = 'stats-error';
    errEl.textContent = msg;
    this.body.appendChild(errEl);
  }

  /** Remove the panel and tab from the DOM. */
  destroy(): void {
    if (this.panel && this.panel.parentNode) {
      this.panel.parentNode.removeChild(this.panel);
    }
    if (this.tab && this.tab.parentNode) {
      this.tab.parentNode.removeChild(this.tab);
    }
    this.panel = null;
    this.body = null;
    this.tab = null;
  }

  // ---------------------------------------------------------------------------
  // Section renderers
  // ---------------------------------------------------------------------------

  private renderObjectCounts(counts: StatsReport['counts']): HTMLElement {
    const section = document.createElement('div');
    section.className = 'stats-section';

    const heading = document.createElement('h3');
    heading.textContent = 'Object counts';
    section.appendChild(heading);

    const table = document.createElement('table');
    table.className = 'stats-table';

    const rows: Array<{ label: string; value: number }> = [
      { label: 'People', value: counts.people },
      { label: 'Families', value: counts.families },
      { label: 'Events', value: counts.events },
      { label: 'Places', value: counts.places },
      { label: 'Sources', value: counts.sources },
      { label: 'Citations', value: counts.citations },
      { label: 'Repositories', value: counts.repositories },
      { label: 'Media', value: counts.media },
      { label: 'Notes', value: counts.notes },
      { label: 'Tags', value: counts.tags },
    ];

    for (const row of rows) {
      const tr = document.createElement('tr');
      const tdLabel = document.createElement('td');
      tdLabel.className = 'stats-label';
      tdLabel.textContent = row.label;
      tr.appendChild(tdLabel);
      const tdValue = document.createElement('td');
      tdValue.className = 'stats-value';
      tdValue.textContent = String(row.value);
      tr.appendChild(tdValue);
      table.appendChild(tr);
    }

    section.appendChild(table);
    return section;
  }

  private renderFamilySizeDistribution(dist: StatsReport['family_size_distribution']): HTMLElement {
    const section = document.createElement('div');
    section.className = 'stats-section';

    const heading = document.createElement('h3');
    heading.textContent = 'Family size dist.';
    section.appendChild(heading);

    const entries = Object.entries(dist);
    if (entries.length === 0) {
      const empty = document.createElement('p');
      empty.className = 'stats-empty';
      empty.textContent = 'None';
      section.appendChild(empty);
    } else {
      const table = document.createElement('table');
      table.className = 'stats-table';
      for (const [size, count] of entries) {
        const tr = document.createElement('tr');
        const tdLabel = document.createElement('td');
        tdLabel.className = 'stats-label';
        tdLabel.textContent = `size ${size}`;
        tr.appendChild(tdLabel);
        const tdValue = document.createElement('td');
        tdValue.className = 'stats-value';
        tdValue.textContent = `${count} ${count === 1 ? 'family' : 'families'}`;
        tr.appendChild(tdValue);
        table.appendChild(tr);
      }
      section.appendChild(table);
    }

    return section;
  }

  private renderFamilyGroupDistribution(dist: StatsReport['family_group_distribution']): HTMLElement {
    const section = document.createElement('div');
    section.className = 'stats-section';

    const heading = document.createElement('h3');
    heading.textContent = 'Family group dist.';
    section.appendChild(heading);

    const entries = Object.entries(dist);
    if (entries.length === 0) {
      const empty = document.createElement('p');
      empty.className = 'stats-empty';
      empty.textContent = 'None';
      section.appendChild(empty);
    } else {
      const table = document.createElement('table');
      table.className = 'stats-table';
      for (const [size, count] of entries) {
        const tr = document.createElement('tr');
        const tdLabel = document.createElement('td');
        tdLabel.className = 'stats-label';
        tdLabel.textContent = `size ${size}`;
        tr.appendChild(tdLabel);
        const tdValue = document.createElement('td');
        tdValue.className = 'stats-value';
        tdValue.textContent = `${count} ${count === 1 ? 'group' : 'groups'}`;
        tr.appendChild(tdValue);
        table.appendChild(tr);
      }
      section.appendChild(table);
    }

    return section;
  }

  private renderDataQuality(report: StatsReport): HTMLElement {
    const section = document.createElement('div');
    section.className = 'stats-section';

    const heading = document.createElement('h3');
    heading.textContent = 'Data quality';
    section.appendChild(heading);

    const table = document.createElement('table');
    table.className = 'stats-table';

    // Not in family
    const tr1 = document.createElement('tr');
    const td1Label = document.createElement('td');
    td1Label.className = 'stats-label';
    td1Label.textContent = 'Not in family';
    tr1.appendChild(td1Label);
    const td1Value = document.createElement('td');
    td1Value.className = 'stats-value';
    td1Value.textContent = String(report.people_not_in_family);
    tr1.appendChild(td1Value);
    table.appendChild(tr1);

    // Dangling refs
    const tr2 = document.createElement('tr');
    const td2Label = document.createElement('td');
    td2Label.className = 'stats-label';
    td2Label.textContent = 'Dangling refs';
    tr2.appendChild(td2Label);
    const td2Value = document.createElement('td');
    td2Value.className = 'stats-value';
    td2Value.textContent = String(report.dangling_refs);
    tr2.appendChild(td2Value);
    table.appendChild(tr2);

    section.appendChild(table);

    // Warnings list
    const warningsHeading = document.createElement('h4');
    warningsHeading.textContent = 'Warnings';
    section.appendChild(warningsHeading);

    if (report.warnings.length === 0) {
      const none = document.createElement('p');
      none.className = 'stats-empty';
      none.textContent = 'None';
      section.appendChild(none);
    } else {
      const ul = document.createElement('ul');
      ul.className = 'stats-warnings';
      for (const warning of report.warnings) {
        const li = document.createElement('li');
        li.className = 'stats-warning';
        li.textContent = warning;
        ul.appendChild(li);
      }
      section.appendChild(ul);
    }

    return section;
  }
}
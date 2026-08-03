// Color gradient mapping for the family graph visualization.
// Uses d3.interpolateViridis over the birth year range.
// Undated nodes get neutral gray; imputed nodes get dashed borders.

import * as d3 from 'd3';
import type { LinkType } from './types';

// ---------------------------------------------------------------------------
// Color scale
// ---------------------------------------------------------------------------

const NEUTRAL_GRAY = '#999999';

/**
 * Create a viridis-based color scale from a list of birth years.
 * The scale maps the min..max range of known years to viridis.
 * Returns a function that maps a year to a hex color string.
 */
export function buildColorScale(birthYears: (number | null)[]): d3.ScaleLinear<string, string> {
  const knownYears = birthYears.filter((y): y is number => y !== null);
  if (knownYears.length === 0) {
    // All null — return a flat scale that always returns gray
    return d3
      .scaleLinear<string>()
      .domain([0, 1])
      .range([NEUTRAL_GRAY, NEUTRAL_GRAY])
      .clamp(true);
  }

  const min = Math.min(...knownYears);
  const max = Math.max(...knownYears);

  if (min === max) {
    // Single year — all dated nodes get the same color (midpoint of viridis)
    const midColor = d3.interpolateViridis(0.5);
    return d3
      .scaleLinear<string>()
      .domain([min, min])
      .range([midColor, midColor])
      .clamp(true);
  }

  return d3
    .scaleLinear<string>()
    .domain([min, max])
    .range([d3.interpolateViridis(0), d3.interpolateViridis(1)])
    .interpolate(d3.interpolateRgb);
}

/**
 * Get the fill color for a node.
 */
export function getNodeColor(
  birthYear: number | null,
  colorScale: d3.ScaleLinear<string, string>,
): string {
  if (birthYear === null) return NEUTRAL_GRAY;
  return colorScale(birthYear);
}

/**
 * Get the stroke dash array for a node.
 * Imputed nodes get a dashed border; known nodes get solid.
 */
export function getNodeStrokeDash(isImputed: boolean): string {
  return isImputed ? '4,3' : 'none';
}

/**
 * Get the stroke opacity for a node.
 * Imputed nodes get slightly reduced opacity.
 */
export function getNodeOpacity(isImputed: boolean): number {
  return isImputed ? 0.85 : 1.0;
}

// ---------------------------------------------------------------------------
// Legend rendering
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Link style constants
// ---------------------------------------------------------------------------

const LINK_PARENT_CHILD_COLOR = '#e67e22';  // orange
const LINK_SPOUSE_COLOR = '#3498db';         // blue
const LINK_PARENT_CHILD_DASH = 'none';       // solid
const LINK_SPOUSE_DASH = '4,3';              // dotted
const LINK_PARENT_CHILD_WIDTH = 1.5;         // thin
const LINK_SPOUSE_WIDTH = 3;                 // thick

// Default fallback for unknown link types
const LINK_FALLBACK_COLOR = '#999999';
const LINK_FALLBACK_DASH = 'none';
const LINK_FALLBACK_WIDTH = 1.5;

/**
 * Get the stroke color for a link based on its type.
 * Parent-child links are orange; spouse links are blue.
 */
export function getLinkColor(linkType: LinkType): string {
  switch (linkType) {
    case 'ParentChild':
      return LINK_PARENT_CHILD_COLOR;
    case 'Spouse':
      return LINK_SPOUSE_COLOR;
    default:
      console.warn('Unknown link type:', linkType);
      return LINK_FALLBACK_COLOR;
  }
}

/**
 * Get the stroke dash array for a link based on its type.
 * Parent-child links are solid; spouse links are dotted.
 */
export function getLinkStrokeDash(linkType: LinkType): string {
  switch (linkType) {
    case 'ParentChild':
      return LINK_PARENT_CHILD_DASH;
    case 'Spouse':
      return LINK_SPOUSE_DASH;
    default:
      console.warn('Unknown link type:', linkType);
      return LINK_FALLBACK_DASH;
  }
}

/**
 * Get the stroke width for a link based on its type.
 * Parent-child links are thin (1.5px); spouse links are thick (3px).
 */
export function getLinkStrokeWidth(linkType: LinkType): number {
  switch (linkType) {
    case 'ParentChild':
      return LINK_PARENT_CHILD_WIDTH;
    case 'Spouse':
      return LINK_SPOUSE_WIDTH;
    default:
      console.warn('Unknown link type:', linkType);
      return LINK_FALLBACK_WIDTH;
  }
}

// ---------------------------------------------------------------------------
// Legend rendering
// ---------------------------------------------------------------------------

export interface LegendConfig {
  /** The color scale to render. */
  colorScale: d3.ScaleLinear<string, string>;
  /** The min birth year (label). */
  minYear: number | null;
  /** The max birth year (label). */
  maxYear: number | null;
  /** Whether any nodes are imputed (show dashed-border legend item). */
  hasImputed: boolean;
  /** Whether any nodes are undated (show gray legend item). */
  hasUndated: boolean;
  /** Whether to show the Spouse (blue dotted) legend item. */
  hasSpouseLinks?: boolean;
  /** Whether to show the Parent-child (orange solid) legend item. */
  hasParentChildLinks?: boolean;
}

/**
 * Render the color legend into a container element.
 * Call this once after the graph is rendered.
 */
export function renderLegend(containerEl: HTMLElement, config: LegendConfig): void {
  containerEl.textContent = '';

  const { colorScale, minYear, maxYear, hasImputed, hasUndated, hasSpouseLinks, hasParentChildLinks } = config;

  // Title
  const title = document.createElement('div');
  title.textContent = 'Birth Year';
  title.style.fontWeight = 'bold';
  title.style.marginBottom = '4px';
  title.style.fontSize = '12px';
  containerEl.appendChild(title);

  if (minYear !== null && maxYear !== null && minYear < maxYear) {
    // Gradient bar
    const gradientId = 'legend-gradient';
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('width', '150');
    svg.setAttribute('height', '24');
    svg.style.display = 'block';
    svg.style.marginBottom = '4px';

    const defs = document.createElementNS('http://www.w3.org/2000/svg', 'defs');
    const gradient = document.createElementNS('http://www.w3.org/2000/svg', 'linearGradient');
    gradient.setAttribute('id', gradientId);
    gradient.setAttribute('x1', '0%');
    gradient.setAttribute('y1', '0%');
    gradient.setAttribute('x2', '100%');
    gradient.setAttribute('y2', '0%');

    const steps = 20;
    for (let i = 0; i <= steps; i++) {
      const t = i / steps;
      const year = minYear + t * (maxYear - minYear);
      const stop = document.createElementNS('http://www.w3.org/2000/svg', 'stop');
      stop.setAttribute('offset', `${t * 100}%`);
      stop.setAttribute('stop-color', colorScale(year));
      gradient.appendChild(stop);
    }
    defs.appendChild(gradient);
    svg.appendChild(defs);

    const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
    rect.setAttribute('x', '0');
    rect.setAttribute('y', '4');
    rect.setAttribute('width', '150');
    rect.setAttribute('height', '10');
    rect.setAttribute('fill', `url(#${gradientId})`);
    rect.setAttribute('rx', '2');
    svg.appendChild(rect);

    containerEl.appendChild(svg);

    // Labels row
    const labels = document.createElement('div');
    labels.style.display = 'flex';
    labels.style.justifyContent = 'space-between';
    labels.style.fontSize = '10px';
    labels.style.color = '#666';

    const minLabel = document.createElement('span');
    minLabel.textContent = String(minYear);
    labels.appendChild(minLabel);

    const maxLabel = document.createElement('span');
    maxLabel.textContent = String(maxYear);
    labels.appendChild(maxLabel);

    containerEl.appendChild(labels);
  } else if (minYear !== null && maxYear !== null && minYear === maxYear) {
    // Single year — show it
    const label = document.createElement('div');
    label.textContent = String(minYear);
    label.style.fontSize = '11px';
    label.style.color = '#666';
    containerEl.appendChild(label);
  }

  // Legend items for special cases
  const items = document.createElement('div');
  items.style.marginTop = '6px';
  items.style.fontSize = '11px';
  items.style.color = '#666';

  if (hasUndated) {
    const row = document.createElement('div');
    row.style.display = 'flex';
    row.style.alignItems = 'center';
    row.style.gap = '4px';

    const swatch = document.createElement('span');
    swatch.style.display = 'inline-block';
    swatch.style.width = '10px';
    swatch.style.height = '10px';
    swatch.style.backgroundColor = NEUTRAL_GRAY;
    swatch.style.borderRadius = '2px';
    row.appendChild(swatch);

    const text = document.createTextNode(' Undated');
    row.appendChild(text);
    items.appendChild(row);
  }

  if (hasImputed) {
    const row = document.createElement('div');
    row.style.display = 'flex';
    row.style.alignItems = 'center';
    row.style.gap = '4px';

    const swatch = document.createElement('span');
    swatch.style.display = 'inline-block';
    swatch.style.width = '10px';
    swatch.style.height = '10px';
    swatch.style.backgroundColor = NEUTRAL_GRAY;
    swatch.style.border = '1px dashed #666';
    swatch.style.borderRadius = '2px';
    row.appendChild(swatch);

    const text = document.createTextNode(' Imputed');
    row.appendChild(text);
    items.appendChild(row);
  }

  containerEl.appendChild(items);

  // Link legend items
  const hasSpouse = hasSpouseLinks ?? false;
  const hasParentChild = hasParentChildLinks ?? false;

  if (hasSpouse || hasParentChild) {
    const linkSection = document.createElement('div');
    linkSection.style.marginTop = '6px';
    linkSection.style.fontSize = '11px';
    linkSection.style.color = '#666';

    // Sub-heading only when both link types are present
    if (hasSpouse && hasParentChild) {
      const heading = document.createElement('div');
      heading.textContent = 'Links';
      heading.style.fontWeight = 'bold';
      heading.style.fontSize = '11px';
      heading.style.marginBottom = '4px';
      linkSection.appendChild(heading);
    }

    function createLinkLegendItem(
      color: string,
      dash: string,
      width: number,
      label: string,
    ): HTMLDivElement {
      const row = document.createElement('div');
      row.style.display = 'flex';
      row.style.alignItems = 'center';
      row.style.gap = '4px';

      const lineSvg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      lineSvg.setAttribute('width', '30');
      lineSvg.setAttribute('height', '12');
      lineSvg.style.display = 'block';

      const line = document.createElementNS('http://www.w3.org/2000/svg', 'line');
      line.setAttribute('x1', '0');
      line.setAttribute('y1', '6');
      line.setAttribute('x2', '30');
      line.setAttribute('y2', '6');
      line.setAttribute('stroke', color);
      line.setAttribute('stroke-width', String(width));
      line.setAttribute('stroke-dasharray', dash);
      lineSvg.appendChild(line);
      row.appendChild(lineSvg);

      const text = document.createTextNode(' ' + label);
      row.appendChild(text);
      return row;
    }

    if (hasSpouse) {
      linkSection.appendChild(
        createLinkLegendItem(LINK_SPOUSE_COLOR, LINK_SPOUSE_DASH, LINK_SPOUSE_WIDTH, 'Spouse'),
      );
    }
    if (hasParentChild) {
      linkSection.appendChild(
        createLinkLegendItem(LINK_PARENT_CHILD_COLOR, LINK_PARENT_CHILD_DASH, LINK_PARENT_CHILD_WIDTH, 'Parent-child'),
      );
    }

    containerEl.appendChild(linkSection);
  }
}
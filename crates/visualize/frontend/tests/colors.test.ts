// Tests for the color gradient module.

import { describe, it, expect, vi } from 'vitest';
import {
  buildColorScale,
  getNodeColor,
  getNodeStrokeDash,
  getNodeOpacity,
  getLinkColor,
  getLinkStrokeDash,
  getLinkStrokeWidth,
} from '../src/colors';

describe('buildColorScale', () => {
  it('creates a viridis scale from known birth years', () => {
    const scale = buildColorScale([1800, 1850, 1900, 1950, 2000]);
    expect(scale(1800)).toBeDefined();
    expect(scale(2000)).toBeDefined();
    // Lower years should map to darker viridis values
    expect(scale(1800)).not.toBe(scale(2000));
  });

  it('returns gray for all-null input', () => {
    const scale = buildColorScale([null, null, null]);
    expect(scale(0)).toBe('rgb(153, 153, 153)');
  });

  it('handles single-year range (all same color)', () => {
    const scale = buildColorScale([1850, 1850, 1850]);
    // Should return the same color for the single year
    expect(scale(1850)).toBeDefined();
    expect(scale(1850)).toBe('rgb(33, 145, 140)');
  });

  it('handles empty array', () => {
    const scale = buildColorScale([]);
    expect(scale(0)).toBe('rgb(153, 153, 153)');
  });

  it('handles mixed null and known values', () => {
    const scale = buildColorScale([null, 1900, null, 1950]);
    expect(scale(1900)).toBeDefined();
    expect(scale(1950)).toBeDefined();
    expect(scale(1900)).not.toBe(scale(1950));
  });

  it('handles year 0', () => {
    const scale = buildColorScale([0, 100]);
    expect(scale(0)).toBeDefined();
    expect(scale(100)).toBeDefined();
  });

  it('handles negative years', () => {
    const scale = buildColorScale([-500, 0, 500]);
    expect(scale(-500)).toBeDefined();
    expect(scale(500)).toBeDefined();
  });
});

describe('getNodeColor', () => {
  it('returns gray for null birth year', () => {
    const scale = buildColorScale([1800, 2000]);
    expect(getNodeColor(null, scale)).toBe('#999999');
  });

  it('returns scale color for known birth year', () => {
    const scale = buildColorScale([1800, 2000]);
    expect(getNodeColor(1900, scale)).toBe(scale(1900));
  });
});

describe('getNodeStrokeDash', () => {
  it('returns dashed for imputed', () => {
    expect(getNodeStrokeDash(true)).toBe('4,3');
  });

  it('returns none for non-imputed', () => {
    expect(getNodeStrokeDash(false)).toBe('none');
  });
});

describe('getNodeOpacity', () => {
  it('returns 0.85 for imputed', () => {
    expect(getNodeOpacity(true)).toBe(0.85);
  });

  it('returns 1.0 for non-imputed', () => {
    expect(getNodeOpacity(false)).toBe(1.0);
  });
});

describe('getLinkColor', () => {
  it('returns orange for ParentChild', () => {
    expect(getLinkColor('ParentChild')).toBe('#e67e22');
  });

  it('returns blue for Spouse', () => {
    expect(getLinkColor('Spouse')).toBe('#3498db');
  });

  it('returns fallback and warns for unknown type', () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const result = getLinkColor('Unknown' as never);
    expect(result).toBe('#999999');
    expect(warnSpy).toHaveBeenCalledWith('Unknown link type:', 'Unknown');
    warnSpy.mockRestore();
  });

  it('returns deterministic values for known types', () => {
    expect(getLinkColor('ParentChild')).toBe('#e67e22');
    expect(getLinkColor('Spouse')).toBe('#3498db');
  });
});

describe('getLinkStrokeDash', () => {
  it('returns none for ParentChild', () => {
    expect(getLinkStrokeDash('ParentChild')).toBe('none');
  });

  it('returns 4,3 for Spouse', () => {
    expect(getLinkStrokeDash('Spouse')).toBe('4,3');
  });

  it('returns fallback and warns for unknown type', () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const result = getLinkStrokeDash('Unknown' as never);
    expect(result).toBe('none');
    expect(warnSpy).toHaveBeenCalledWith('Unknown link type:', 'Unknown');
    warnSpy.mockRestore();
  });

  it('returns deterministic values for known types', () => {
    expect(getLinkStrokeDash('ParentChild')).toBe('none');
    expect(getLinkStrokeDash('Spouse')).toBe('4,3');
  });
});

describe('getLinkStrokeWidth', () => {
  it('returns 1.5 for ParentChild', () => {
    expect(getLinkStrokeWidth('ParentChild')).toBe(1.5);
  });

  it('returns 3 for Spouse', () => {
    expect(getLinkStrokeWidth('Spouse')).toBe(3);
  });

  it('returns fallback and warns for unknown type', () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const result = getLinkStrokeWidth('Unknown' as never);
    expect(result).toBe(1.5);
    expect(warnSpy).toHaveBeenCalledWith('Unknown link type:', 'Unknown');
    warnSpy.mockRestore();
  });

  it('returns deterministic values for known types', () => {
    expect(getLinkStrokeWidth('ParentChild')).toBe(1.5);
    expect(getLinkStrokeWidth('Spouse')).toBe(3);
  });
});
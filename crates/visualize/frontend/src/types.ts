// TypeScript interfaces matching the Rust GraphData types sent over Tauri IPC.

export interface PersonNode {
  handle: string;
  name: string;
  birth_date: string | null;
  death_date: string | null;
  birth_year: number | null;
  is_imputed: boolean;
  gender: 'male' | 'female' | 'unknown';
  family_group: number;
  generation: number;
}

export type LinkType = 'Spouse' | 'ParentChild';

export interface FamilyLink {
  source: string;
  target: string;
  link_type: LinkType;
}

export interface FamilyGroupMeta {
  id: number;
  size: number;
  span: number;
}

export interface GraphData {
  nodes: PersonNode[];
  links: FamilyLink[];
  family_groups: FamilyGroupMeta[];
}

export interface SelectedPerson {
  handle: string;
  name: string;
  birth_date: string | null;
  death_date: string | null;
  gender: string;
  family_group: number;
}

export interface SelectionExport {
  exported_at: string;
  file: string;
  selections: SelectedPerson[];
}

/**
 * Configuration for the per-generation Y-field force and per-type link strengths.
 * Each value is a multiplier in [0, 2] on a sensible default.
 */
export interface ForceConfig {
  /** Multiplier for the Y-field pull toward a node's generation band. */
  generationPull: number;
  /** Multiplier for spouse-link spring stiffness. */
  spouseStrength: number;
  /** Multiplier for parent-child link spring stiffness. */
  parentChildStrength: number;
}

/** Sensible defaults for ForceConfig (produces visible bands in typical 2-5 gen trees). */
export const DEFAULT_FORCE_CONFIG: ForceConfig = {
  generationPull: 0.30,
  spouseStrength: 0.80,
  parentChildStrength: 0.50,
};

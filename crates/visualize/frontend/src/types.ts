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
  /** Multiplier for the selection-repel pairwise force. */
  repelStrength: number;
  /** Multiplier for the selected-attract centroid pull. */
  selectedAttractStrength: number;
  /** Multiplier for the unselected-attract centroid pull. */
  unselectedAttractStrength: number;
}

/** Sensible defaults for ForceConfig (produces visible bands in typical 2-5 gen trees). */
export const DEFAULT_FORCE_CONFIG: ForceConfig = {
  generationPull: 0.30,
  spouseStrength: 0.80,
  parentChildStrength: 0.50,
  repelStrength: 0.00,
  selectedAttractStrength: 0.00,
  unselectedAttractStrength: 0.00,
};

// ---------------------------------------------------------------------------
// StatsReport types (matching gramps_reader::StatsReport over IPC)
// ---------------------------------------------------------------------------

export interface PrimaryTypeCounts {
  people: number;
  families: number;
  events: number;
  places: number;
  sources: number;
  citations: number;
  repositories: number;
  media: number;
  notes: number;
  tags: number;
}

export type FamilySizeDistribution = Record<string, number>;
export type FamilyGroupDistribution = Record<string, number>;

export interface StatsReport {
  file: string;
  counts: PrimaryTypeCounts;
  family_size_distribution: FamilySizeDistribution;
  family_group_distribution: FamilyGroupDistribution;
  /** Row: group-size, Column: generation-span, Cell: group count. Unused by frontend. */
  family_group_generation_table: Record<string, Record<string, number>>;
  people_not_in_family: number;
  dangling_refs: number;
  warnings: string[];
}

// ---------------------------------------------------------------------------
// LoadedGraph — combined result from the load_graph IPC command
// ---------------------------------------------------------------------------

export interface LoadedGraph {
  graph_data: GraphData;
  stats: StatsReport;
}

// ---------------------------------------------------------------------------
// Selection mode types
// ---------------------------------------------------------------------------

export type SelectionMode = 'single' | 'ancestors' | 'descendants' | 'first-degree' | 'second-degree';

export interface SelectionModeOption {
  value: SelectionMode;
  label: string;
  description: string;
}

export const SELECTION_MODES: SelectionModeOption[] = [
  { value: 'single', label: 'Single node', description: 'Select one node at a time' },
  { value: 'ancestors', label: 'Ancestors', description: 'Select node + all ancestors' },
  { value: 'descendants', label: 'Descendants', description: 'Select node + all descendants' },
  { value: 'first-degree', label: '1st-degree', description: 'Select node + spouses, parents, children' },
  { value: 'second-degree', label: '2nd-degree', description: 'Select node + 2-hop connections' },
];

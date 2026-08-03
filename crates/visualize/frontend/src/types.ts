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
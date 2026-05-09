// TypeScript mirror of journal_core::template::NotebookTemplate.
// Keep field names + casing serde-faithful so a future TOML round-trip
// via journal-web-shim emits bytes the desktop loads unchanged.

import type { Uuid } from "./index";

export type Weekday =
  | "Mon"
  | "Tue"
  | "Wed"
  | "Thu"
  | "Fri"
  | "Sat"
  | "Sun";

export const WEEKDAYS: Weekday[] = [
  "Mon",
  "Tue",
  "Wed",
  "Thu",
  "Fri",
  "Sat",
  "Sun",
];

export type PlannerGrouping = "Month" | "Week";

export interface DailySlot {
  days: Weekday[];
  templates: Uuid[];
}

export interface EntryFlags {
  bridge_previous: boolean;
  bridge_next: boolean;
}

export interface SectionTitleFormats {
  year: string;
  month: string;
  week: string;
}

export interface NotebookTemplate {
  id: Uuid;
  name: string;
  description: string;
  year_start: Uuid[];
  before_quarter: Uuid[];
  before_month: Uuid[];
  before_week: Uuid[];
  daily_slots: DailySlot[];
  grouping: PlannerGrouping;
  page_title_format: string;
  section_title_formats: SectionTitleFormats;
  /** Keys: "year_start:N" / "before_quarter:N" / "before_month:N" /
   * "before_week:N" / "daily:S:N". */
  entry_options: Record<string, EntryFlags>;
}

export const DEFAULT_NOTEBOOK_TEMPLATE: NotebookTemplate = {
  id: "00000000-0000-0000-0000-000000000000",
  name: "Untitled Planner",
  description: "",
  year_start: [],
  before_quarter: [],
  before_month: [],
  before_week: [],
  daily_slots: [],
  grouping: "Month",
  page_title_format: "{date}",
  section_title_formats: {
    year: "{year}",
    month: "{month_name} {year}",
    week: "Week of {date}",
  },
  entry_options: {},
};

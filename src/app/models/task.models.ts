/**
 * Shapes returned by the Rust side. These mirror `int-tasks-core`; changing one
 * without the other is the main way this app can break, so keep the field names
 * in step with `crates/int-tasks-core/src/model.rs`.
 */

export type Status = "todo" | "doing" | "done";
export type SessionKind = "focus" | "break";
export type TodayReason = "overdue" | "due" | "flagged" | "inprogress";

export interface Task {
  id: string;
  title: string;
  notes?: string;
  status: Status;
  list_id: string;
  order: number;
  due?: string;
  today?: boolean;
  project?: string;
  tags?: string[];
  impact?: number;
  effort?: number;
  priority?: number;
  estimate_minutes?: number;
  created_at: number;
  updated_at: number;
  completed_at?: number;
  external_id?: string;
  /** Who the task belongs to. Absent means you. */
  assignee?: string;
  assigned_by?: string;
  /** `scheme:reference` — where the task came from. */
  origin?: string;
  /** Bumped per task, so two devices editing different tasks cannot conflict. */
  revision?: number;
}

export interface List {
  id: string;
  name: string;
  order: number;
  status?: Status;
}

export interface Board {
  id: string;
  name: string;
  order: number;
  lists: List[];
}

export interface Settings {
  dailyFocusGoal: number;
  hideCompletedAfterDays: number;
  /** Days of the week that count as working days, 0 = Sunday. */
  workingDays: number[];
  /** Individual non-working dates, `YYYY-MM-DD`. */
  holidays: string[];
}

/** A project or tag, with how much work carries it. */
export interface LabelUse {
  name: string;
  open: number;
  done: number;
}

export interface Data {
  boards: Board[];
  tasks: Task[];
  revision: number;
  settings: Settings;
}

export type Quadrant = "quick-win" | "big-bet" | "fill-in" | "thankless";

/** A task placed on the impact/effort matrix. */
export interface Plotted extends Task {
  impact: number;
  effort: number;
  /** Derived from the due date and priority, not stored. */
  urgency: number;
  quadrant: Quadrant;
  ratio: number;
  score: number;
}

export interface Stats {
  streakDays: number;
  sessionsToday: number;
  dailyGoal: number;
  focusMinutesToday: number;
  pointsToday: number;
  pointsTotal: number;
  completedToday: number;
  goalMet: boolean;
}

export interface TodayEntry extends Task {
  reason: TodayReason;
}

/**
 * A recorded stretch of work.
 *
 * Snake case because `Session` is serialised as declared — unlike `Settings`
 * and `Stats`, which carry a camelCase rename.
 */
export interface Session {
  id: string;
  task_id?: string;
  started_at: number;
  ended_at: number;
  seconds: number;
  kind: SessionKind;
  completed: boolean;
}

export interface TimerState {
  running: boolean;
  /** Running but held; keeps its task and the time already worked. */
  paused: boolean;
  taskId?: string;
  taskTitle?: string;
  kind: SessionKind;
  startedAt: number;
  plannedSeconds: number;
  remainingSeconds: number;
}

/** One working day on the Flow trend. */
export interface DayProgress {
  date: string;
  focusMinutes: number;
  points: number;
}

export interface TaskTime {
  task_id: string;
  title?: string;
  seconds: number;
  sessions: number;
}

export interface TimeSummary {
  total_seconds: number;
  focus_sessions: number;
  by_task: TaskTime[];
  unattributed_seconds: number;
}

/** Everything the UI needs for a render, fetched in one call. */
export interface Snapshot {
  data: Data;
  today: TodayEntry[];
  timer: TimerState;
  summary: TimeSummary;
  matrix: Plotted[];
  stats: Stats;
  progress: DayProgress[];
  projects: LabelUse[];
  tags: LabelUse[];
  settings: Settings;
  date: string;
}

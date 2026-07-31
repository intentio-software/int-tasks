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

export interface Data {
  boards: Board[];
  tasks: Task[];
  revision: number;
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

export interface TimerState {
  running: boolean;
  taskId?: string;
  taskTitle?: string;
  kind: SessionKind;
  startedAt: number;
  plannedSeconds: number;
  remainingSeconds: number;
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
  date: string;
}

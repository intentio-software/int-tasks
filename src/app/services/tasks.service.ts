import { Injectable, computed, signal } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import { UnlistenFn, listen } from "@tauri-apps/api/event";

import { TeamMember } from "../components/team-view.component";
import { Board, DayProgress, LabelUse, List, Plotted, Session, Settings, Snapshot, Stats, Task, TaskContext, TimeSummary, TimerState, TodayEntry } from "../models/task.models";

/**
 * The app's single source of truth.
 *
 * All state lives in Rust; this holds the signals the UI renders from. Every
 * mutation re-fetches a whole snapshot rather than patching in memory, because
 * the MCP server may be writing to the same store at the same time — and one
 * round trip is cheaper than reasoning about which local copy is stale.
 */
@Injectable({ providedIn: "root" })
export class TasksService {
  readonly boards = signal<Board[]>([]);
  readonly tasks = signal<Task[]>([]);
  readonly today = signal<TodayEntry[]>([]);
  readonly timer = signal<TimerState | null>(null);
  readonly matrix = signal<Plotted[]>([]);
  readonly stats = signal<Stats | null>(null);
  readonly projectLabels = signal<LabelUse[]>([]);
  readonly tagLabels = signal<LabelUse[]>([]);
  readonly settings = signal<Settings | null>(null);
  readonly sessions = signal<Session[]>([]);
  readonly date = signal<string>("");
  readonly error = signal<string | null>(null);
  readonly storePath = signal<string>("");

  /** Total focus time recorded today, in seconds. */
  readonly focusSecondsToday = signal(0);
  readonly summary = signal<TimeSummary | null>(null);
  readonly progress = signal<DayProgress[]>([]);
  readonly team = signal<TeamMember[]>([]);

  private unlistenTimer: UnlistenFn | null = null;

  readonly activeBoard = signal<Board | null>(null);

  /** Tasks grouped by list id, in display order. */
  readonly byList = computed<Record<string, Task[]>>(() => {
    const grouped: Record<string, Task[]> = {};
    for (const task of this.tasks()) {
      (grouped[task.list_id] ??= []).push(task);
    }
    for (const list of Object.values(grouped)) {
      list.sort((a, b) => a.order - b.order || a.created_at - b.created_at);
    }
    return grouped;
  });

  readonly openCount = computed(() => this.tasks().filter((task) => task.status !== "done").length);

  async load(): Promise<void> {
    try {
      const snapshot = await invoke<Snapshot>("snapshot");
      this.apply(snapshot);
      this.error.set(null);
    } catch (error) {
      this.error.set(messageFor(error));
    }
    if (!this.storePath()) {
      this.storePath.set(await invoke<string>("store_path").catch(() => ""));
    }
  }

  private apply(snapshot: Snapshot): void {
    this.boards.set(snapshot.data.boards);
    this.tasks.set(snapshot.data.tasks);
    this.today.set(snapshot.today);
    this.timer.set(snapshot.timer);
    this.matrix.set(snapshot.matrix);
    this.stats.set(snapshot.stats);
    this.projectLabels.set(snapshot.projects);
    this.tagLabels.set(snapshot.tags);
    this.settings.set(snapshot.settings);
    this.date.set(snapshot.date);
    this.focusSecondsToday.set(snapshot.summary.total_seconds);
    this.summary.set(snapshot.summary);
    this.progress.set(snapshot.progress ?? []);

    // Keep the selected board if it still exists, otherwise fall back.
    const current = this.activeBoard();
    const match = snapshot.data.boards.find((board) => board.id === current?.id);
    this.activeBoard.set(match ?? snapshot.data.boards[0] ?? null);
  }

  /** Follow the timer without polling; the countdown ticks in Rust. */
  async watchTimer(): Promise<void> {
    this.unlistenTimer?.();
    try {
      this.unlistenTimer = await listen<TimerState>("timer", (event) => {
        this.timer.set(event.payload);
        // A finished session changes recorded time, so refresh totals.
        if (!event.payload.running) {
          void this.load();
        }
      });
    } catch {
      // Outside Tauri there is nothing to listen to.
    }
  }

  stopWatching(): void {
    this.unlistenTimer?.();
    this.unlistenTimer = null;
  }

  // -------------------------------------------------------------------------
  // mutations
  // -------------------------------------------------------------------------

  async addTask(title: string, listId?: string): Promise<Task | null> {
    return this.guard(() => invoke<Task>("add_task", { title, listId: listId ?? null }));
  }

  async setDone(taskId: string, done: boolean): Promise<void> {
    await this.guard(() => invoke<Task>("set_done", { taskId, done }));
  }

  async moveTask(taskId: string, listId: string, position?: number): Promise<void> {
    await this.guard(() => invoke<Task>("move_task", { taskId, listId, position: position ?? null }));
  }

  async deleteTask(taskId: string): Promise<void> {
    await this.guard(() => invoke<Task>("delete_task", { taskId }));
  }

  /** Patch a task. Omitted fields are untouched; `null` clears one. */
  async updateTask(taskId: string, patch: Record<string, unknown>): Promise<void> {
    await this.guard(() => invoke<Task>("update_task", { taskId, ...patch }));
  }

  /** Set or clear the matrix scores. */
  async scoreTask(taskId: string, impact: number | null, effort: number | null): Promise<void> {
    await this.guard(() => invoke<Task>("score_task", { taskId, impact, effort }));
  }

  /** Ask for something worth doing now. */
  async suggest(lowEnergy: boolean): Promise<Plotted | null> {
    try {
      return await invoke<Plotted | null>("suggest_task", { lowEnergy });
    } catch {
      return null;
    }
  }

  async setDailyGoal(sessions: number): Promise<void> {
    await this.guard(() => invoke("set_daily_goal", { sessions }));
  }

  /** Project names in use, for filters and autocomplete. */
  readonly projects = computed<string[]>(() => this.projectLabels().map((label) => label.name));

  /** Tag names in use, for autocomplete. */
  readonly tagNames = computed<string[]>(() => this.tagLabels().map((label) => label.name));

  async renameProject(from: string, to: string): Promise<void> {
    await this.guard(() => invoke<number>("rename_project", { from, to }));
  }

  async deleteProject(name: string): Promise<void> {
    await this.guard(() => invoke<number>("delete_project", { name }));
  }

  async renameTag(from: string, to: string): Promise<void> {
    await this.guard(() => invoke<number>("rename_tag", { from, to }));
  }

  async deleteTag(name: string): Promise<void> {
    await this.guard(() => invoke<number>("delete_tag", { name }));
  }

  async setWorkingDays(days: number[]): Promise<void> {
    await this.guard(() => invoke("set_working_days", { days }));
  }

  async setHolidays(holidays: string[]): Promise<void> {
    await this.guard(() => invoke("set_holidays", { holidays }));
  }

  async setHideCompletedAfterDays(days: number): Promise<void> {
    await this.guard(() => invoke("set_hide_completed_after_days", { days }));
  }

  async addBoard(name: string): Promise<Board | null> {
    return this.guard(() => invoke<Board>("add_board", { name }));
  }

  async addList(boardId: string, name: string): Promise<List | null> {
    return this.guard(() => invoke<List>("add_list", { boardId, name }));
  }

  // -------------------------------------------------------------------------
  // timer
  // -------------------------------------------------------------------------

  async startTimer(taskId?: string, minutes?: number, isBreak = false): Promise<void> {
    await this.guard(() =>
      invoke<TimerState>("start_timer", {
        taskId: taskId ?? null,
        minutes: minutes ?? null,
        breakSession: isBreak
      })
    );
  }

  async stopTimer(): Promise<void> {
    await this.guard(() => invoke<TimerState>("stop_timer"));
  }

  async pauseTimer(): Promise<void> {
    // No reload: pausing changes the timer, not the store.
    this.timer.set(await invoke<TimerState>("pause_timer"));
  }

  async resumeTimer(): Promise<void> {
    this.timer.set(await invoke<TimerState>("resume_timer"));
  }

  /** Recorded sessions, newest first. */
  async loadSessions(): Promise<void> {
    try {
      const sessions = await invoke<Session[]>("sessions");
      this.sessions.set([...sessions].sort((a, b) => b.started_at - a.started_at));
    } catch {
      // The history is a review surface; failing to read it must not break the app.
    }
  }

  async deleteSession(sessionId: string): Promise<void> {
    await this.guard(() => invoke<Session>("delete_session", { sessionId }));
    await this.loadSessions();
  }

  /** Attribute a recorded session to a task, or to nothing when null. */
  /** Resolve a task's origin into the note or map behind it. */
  // ---------------------------------------------------------------------------
  // team
  // ---------------------------------------------------------------------------

  /** Everyone whose store sits beside ours. Empty when working alone. */
  async loadTeam(): Promise<void> {
    try {
      this.team.set(await invoke<TeamMember[]>("team"));
    } catch {
      this.team.set([]);
    }
  }

  async assignTo(member: string, line: string): Promise<void> {
    await this.guard(() => invoke<Task>("assign_to", { member, line }));
    await this.loadTeam();
  }

  async syncTasks(): Promise<string | null> {
    try {
      const outcome = await invoke<{ changed: boolean; message: string; blocked?: string }>(
        "tasks_sync_now"
      );
      await this.load();
      await this.loadTeam();
      return outcome.blocked ?? null;
    } catch (error) {
      return String(error);
    }
  }

  async chooseStoreRoot(root: string | null): Promise<void> {
    await this.guard(() => invoke("set_store_root", { root }));
  }

  async taskContext(origin: string): Promise<TaskContext | null> {
    try {
      return await invoke<TaskContext>("task_context", { origin });
    } catch {
      // Context is a convenience; failing to fetch it must not break the panel.
      return null;
    }
  }

  async assignSession(sessionId: string, taskId: string | null): Promise<void> {
    await this.guard(() => invoke<Session>("assign_session", { sessionId, taskId }));
    await this.loadSessions();
  }

  /**
   * Run an operation, surface any error, and re-read the store.
   *
   * Reloading after every change is what keeps the UI honest when an agent is
   * writing to the same files.
   */
  private async guard<T>(action: () => Promise<T>): Promise<T | null> {
    try {
      const result = await action();
      this.error.set(null);
      await this.load();
      return result;
    } catch (error) {
      this.error.set(messageFor(error));
      return null;
    }
  }
}

function messageFor(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Something went wrong.";
}

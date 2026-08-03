import {
  Component,
  HostListener,
  OnDestroy,
  OnInit,
  computed,
  inject,
  signal
} from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { CdkDragDrop, DragDropModule, moveItemInArray, transferArrayItem } from "@angular/cdk/drag-drop";
import { UnlistenFn, listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "primeng/button";
import { Toast } from "primeng/toast";

import { AboutDialogComponent } from "./components/about-dialog.component";
import { AssignSessionComponent } from "./components/assign-session.component";
import { LabelEdit, LabelKind, LabelsDialogComponent } from "./components/labels-dialog.component";
import { MatrixViewComponent } from "./components/matrix-view.component";
import { TaskDetailComponent } from "./components/task-detail.component";
import { TaskAction, TaskMenuComponent } from "./components/task-menu.component";
import { FlowStatsComponent } from "./components/flow-stats.component";
import { ProgressTrendComponent } from "./components/progress-trend.component";
import { SessionLogComponent } from "./components/session-log.component";
import { WorkingRhythmComponent } from "./components/working-rhythm.component";
import { TaskRowComponent } from "./components/task-row.component";
import { List, Plotted, Session, Task } from "./models/task.models";
import { TasksService } from "./services/tasks.service";
import { ThemeService } from "./services/theme.service";
import { UpdaterService } from "./services/updater.service";

type View = "today" | "board" | "matrix" | "flow";

@Component({
  selector: "app-root",
  standalone: true,
  imports: [
    CommonModule,
    FormsModule,
    DragDropModule,
    Toast,
    Button,
    AboutDialogComponent,
    AssignSessionComponent,
    LabelsDialogComponent,
    MatrixViewComponent,
    TaskDetailComponent,
    FlowStatsComponent,
    ProgressTrendComponent,
    SessionLogComponent,
    TaskMenuComponent,
    TaskRowComponent,
    WorkingRhythmComponent
  ],
  templateUrl: "./app.component.html",
  styleUrls: ["./app.component.css"]
})
export class AppComponent implements OnInit, OnDestroy {
  readonly tasks = inject(TasksService);
  readonly theme = inject(ThemeService);
  readonly updater = inject(UpdaterService);

  readonly view = signal<View>("today");
  readonly aboutOpen = signal(false);
  readonly labelsOpen = signal(false);
  /** A finished focus session that landed against no task. */
  readonly unassignedSession = signal<Session | null>(null);
  /** The task the context menu is open on, and where it was opened. */
  readonly taskMenu = signal<{ task: Task; x: number; y: number } | null>(null);
  readonly appVersion = signal("0.1.0");
  readonly draft = signal("");
  /** Task shown in the detail panel, if any. */
  readonly editing = signal<Task | null>(null);
  /** The last suggestion, so it can be shown and acted on. */
  readonly suggestion = signal<Plotted | null>(null);
  /** Empty means every project. */
  readonly projectFilter = signal("");

  /** Goal options that stay achievable; a target never met means nothing. */
  readonly goalOptions = [2, 3, 4, 5, 6, 8, 10];

  /**
   * What the typed line will become, shown under the capture box.
   *
   * The Rust parser is authoritative — this only previews, and only while a
   * marker is actually being typed, so it costs nothing to be a little less
   * exhaustive than the real thing.
   */
  readonly capturePreview = computed(() => {
    let rest = this.draft().trim();
    if (!rest.startsWith("(") && !rest.startsWith("[")) {
      return null;
    }
    let project: string | null = null;
    const tags: string[] = [];
    for (;;) {
      // Brackets must match their own kind, as the Rust parser requires.
      const match = /^(?:\(([^\s()[\]]+)\)|\[([^\s()[\]]+)\])\s*/.exec(rest);
      if (!match) {
        break;
      }
      if (match[1] !== undefined) {
        if (project) {
          break;
        }
        project = match[1];
      } else {
        tags.push(match[2]);
      }
      rest = rest.slice(match[0].length);
    }
    // Markers with nothing after them are kept as the title, as the store does.
    if (!rest || (!project && !tags.length)) {
      return null;
    }
    return { title: rest, project, tags };
  });

  /** Today, narrowed to the chosen project. */
  readonly visibleToday = computed(() => {
    const project = this.projectFilter();
    const entries = this.tasks.today();
    return project ? entries.filter((entry) => entry.project === project) : entries;
  });

  /** The matrix, narrowed to the chosen project. */
  readonly visibleMatrix = computed(() => {
    const project = this.projectFilter();
    const plotted = this.tasks.matrix();
    return project ? plotted.filter((entry) => entry.project === project) : plotted;
  });

  private menuUnlisten: UnlistenFn | null = null;
  private finishedUnlisten: UnlistenFn | null = null;
  private unassignedUnlisten: UnlistenFn | null = null;

  /** Remaining time as `24:31`, for the in-app timer bar. */
  readonly clock = computed(() => {
    const timer = this.tasks.timer();
    if (!timer) {
      return "";
    }
    const seconds = timer.remainingSeconds;
    return `${String(Math.floor(seconds / 60)).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
  });

  readonly focusToday = computed(() => {
    const minutes = Math.round(this.tasks.focusSecondsToday() / 60);
    if (minutes < 60) {
      return `${minutes}m`;
    }
    return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
  });

  async ngOnInit(): Promise<void> {
    await this.tasks.load();
    await this.tasks.watchTimer();
    void this.connectMenu();
    void this.loadVersion();

    void this.tasks.loadSessions();

    this.finishedUnlisten = await listen("timer-finished", () => {
      void this.tasks.load();
      void this.tasks.loadSessions();
    }).catch(() => null as unknown as UnlistenFn);

    // Rust raises the window before emitting this, so the question arrives on
    // screen rather than behind whatever the user moved on to.
    this.unassignedUnlisten = await listen<Session>("session-unassigned", (event) => {
      void this.tasks.load();
      void this.tasks.loadSessions();
      this.unassignedSession.set(event.payload);
    }).catch(() => null as unknown as UnlistenFn);

    // Delay so the shell has rendered before a toast can appear.
    setTimeout(() => void this.updater.checkForUpdates(), 3000);
  }

  ngOnDestroy(): void {
    this.tasks.stopWatching();
    this.menuUnlisten?.();
    this.finishedUnlisten?.();
    this.unassignedUnlisten?.();
  }

  private async loadVersion(): Promise<void> {
    try {
      const { getVersion } = await import("@tauri-apps/api/app");
      this.appVersion.set(await getVersion());
    } catch {
      // Running outside the desktop shell.
    }
  }

  installUpdate(message: { data?: unknown }): void {
    void this.updater.installUpdate(message.data);
  }

  // -------------------------------------------------------------------------
  // capture
  // -------------------------------------------------------------------------

  /**
   * Add whatever is in the box.
   *
   * No validation beyond "not empty", no fields, no dialog — the entire point
   * is that a thought can be recorded before it is gone.
   */
  async capture(): Promise<void> {
    const title = this.draft().trim();
    if (!title) {
      return;
    }
    // Cleared first so typing can continue while the write happens.
    this.draft.set("");
    await this.tasks.addTask(title, this.captureListId());
  }

  /** On the board, capture into the visible board's first list. */
  private captureListId(): string | undefined {
    if (this.view() !== "board") {
      return undefined;
    }
    const board = this.tasks.activeBoard();
    return board?.lists.slice().sort((a, b) => a.order - b.order)[0]?.id;
  }

  async toggleDone(task: Task, done: boolean): Promise<void> {
    await this.tasks.setDone(task.id, done);
  }

  async remove(task: Task): Promise<void> {
    await this.tasks.deleteTask(task.id);
  }

  /** Star a task onto Today without giving it a due date. */
  async toggleToday(task: Task): Promise<void> {
    await this.tasks.updateTask(task.id, { today: !task.today });
  }

  open(task: Task): void {
    this.editing.set(task);
  }

  /** Re-read the edited task so the panel shows what was actually stored. */
  private refreshEditing(): void {
    const current = this.editing();
    if (!current) {
      return;
    }
    const updated = this.tasks.tasks().find((task) => task.id === current.id);
    this.editing.set(updated ?? null);
  }

  async applyEdit(patch: Record<string, unknown>): Promise<void> {
    const task = this.editing();
    if (task) {
      await this.tasks.updateTask(task.id, patch);
      this.refreshEditing();
    }
  }

  async applyScore(scores: { impact: number | null; effort: number | null }): Promise<void> {
    const task = this.editing();
    if (task) {
      await this.tasks.scoreTask(task.id, scores.impact, scores.effort);
      this.refreshEditing();
    }
  }

  async deleteEditing(): Promise<void> {
    const task = this.editing();
    if (task) {
      this.editing.set(null);
      await this.tasks.deleteTask(task.id);
    }
  }

  /**
   * Hand back something worth doing. Low energy asks for the cheapest thing
   * that still pays rather than the most valuable.
   */
  async suggest(lowEnergy: boolean): Promise<void> {
    const picked = await this.tasks.suggest(lowEnergy);
    this.suggestion.set(picked);
    if (picked) {
      this.view.set("flow");
    }
  }

  async startSuggested(): Promise<void> {
    const picked = this.suggestion();
    if (picked) {
      await this.tasks.startTimer(picked.id);
    }
  }

  // -------------------------------------------------------------------------
  // timer
  // -------------------------------------------------------------------------

  isRunningFor(task: Task): boolean {
    const timer = this.tasks.timer();
    return !!timer?.running && timer.taskId === task.id;
  }

  async toggleTimerFor(task: Task): Promise<void> {
    if (this.isRunningFor(task)) {
      await this.tasks.stopTimer();
    } else {
      await this.tasks.startTimer(task.id);
    }
  }

  async startBreak(): Promise<void> {
    await this.tasks.startTimer(undefined, undefined, true);
  }

  async stopTimer(): Promise<void> {
    await this.tasks.stopTimer();
  }

  // -------------------------------------------------------------------------
  // board
  // -------------------------------------------------------------------------

  sortedLists(): List[] {
    return (this.tasks.activeBoard()?.lists ?? []).slice().sort((a, b) => a.order - b.order);
  }

  tasksIn(listId: string): Task[] {
    return this.tasks.byList()[listId] ?? [];
  }

  /**
   * The cards actually drawn in a column.
   *
   * With no filter this hands back the stored array itself, which is what lets
   * a dragged card settle in place instead of snapping back while the write
   * happens. Filtering necessarily produces a new array, so that trick is lost
   * — see `storedIndex` for how the drop position survives it.
   */
  visibleTasksIn(listId: string): Task[] {
    const project = this.projectFilter();
    const tasks = this.tasksIn(listId);
    return project ? tasks.filter((task) => task.project === project) : tasks;
  }

  /** Ids of every list, so cdkDropList knows what it can exchange with. */
  listIds(): string[] {
    return this.sortedLists().map((list) => list.id);
  }

  async onDrop(event: CdkDragDrop<Task[]>, listId: string): Promise<void> {
    const task = event.item.data as Task;
    if (event.previousContainer === event.container) {
      moveItemInArray(event.container.data, event.previousIndex, event.currentIndex);
    } else {
      transferArrayItem(
        event.previousContainer.data,
        event.container.data,
        event.previousIndex,
        event.currentIndex
      );
    }
    // The local arrays are moved first so the card does not visibly snap back
    // while the write happens; the reload afterwards is the authority.
    await this.tasks.moveTask(task.id, listId, this.storedIndex(listId, event.currentIndex, task.id));
  }

  /**
   * Translate a drop position in a filtered column to a position in the column.
   *
   * Dropping between two cards of one project must not send the task to that
   * index of the whole list — with a filter on, the third visible card can be
   * the twentieth stored one. The task lands immediately before whichever card
   * it was dropped above, or at the end if it was dropped past the last.
   */
  private storedIndex(listId: string, droppedAt: number, movingId: string): number {
    const project = this.projectFilter();
    if (!project) {
      return droppedAt;
    }
    // Untouched by the drag: filtering hands cdkDropList a throwaway array, so
    // the stored order is still exactly as it was before the drop.
    const others = this.tasksIn(listId).filter((task) => task.id !== movingId);
    const neighbour = others.filter((task) => task.project === project)[droppedAt];
    return neighbour ? others.indexOf(neighbour) : others.length;
  }

  selectBoard(boardId: string): void {
    const board = this.tasks.boards().find((candidate) => candidate.id === boardId);
    if (board) {
      this.tasks.activeBoard.set(board);
    }
  }

  /**
   * Create a board and switch to it.
   *
   * Making one and leaving the user on the old board was the bug here: boards
   * existed but were unreachable.
   */
  async newBoard(): Promise<void> {
    const created = await this.tasks.addBoard(`Board ${this.tasks.boards().length + 1}`);
    if (created) {
      this.selectBoard(created.id);
      this.view.set("board");
    }
  }

  async addList(): Promise<void> {
    const board = this.tasks.activeBoard();
    if (!board) {
      return;
    }
    const name = `List ${board.lists.length + 1}`;
    await this.tasks.addList(board.id, name);
  }

  // -------------------------------------------------------------------------
  // chrome
  // -------------------------------------------------------------------------

  private async connectMenu(): Promise<void> {
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
      return;
    }
    try {
      this.menuUnlisten = await listen<string>("menu-action", (event) => {
        void this.runMenuAction(event.payload);
      });
    } catch {
      // Without the native menu the in-app controls still work.
    }
  }

  private async runMenuAction(action: string): Promise<void> {
    switch (action) {
      case "new-task":
        this.focusCapture();
        break;
      case "new-board":
        await this.newBoard();
        break;
      case "view-today":
        this.view.set("today");
        break;
      case "view-board":
        this.view.set("board");
        break;
      case "view-matrix":
        this.view.set("matrix");
        break;
      case "view-flow":
        this.view.set("flow");
        break;
      case "manage-labels":
        this.labelsOpen.set(true);
        break;
      case "toggle-theme":
        this.theme.cycle();
        break;
      case "timer-start": {
        const first = this.tasks.today()[0];
        await this.tasks.startTimer(first?.id);
        break;
      }
      case "timer-break":
        await this.startBreak();
        break;
      case "timer-stop":
        await this.stopTimer();
        break;
      case "about":
        this.aboutOpen.set(true);
        break;
      case "check-updates":
        await this.updater.manualCheck();
        break;
      case "website":
        await openUrl("https://intentiosoftware.com").catch(() => undefined);
        break;
      default:
        break;
    }
  }

  /** Keyed by id so completing a task does not re-render the whole list. */
  trackTask(_: number, task: Task): string {
    return task.id;
  }

  focusCapture(): void {
    const input = document.getElementById("capture") as HTMLInputElement | null;
    input?.focus();
  }

  @HostListener("window:keydown", ["$event"])
  onKey(event: KeyboardEvent): void {
    if (event.key === "Escape" && this.taskMenu()) {
      event.preventDefault();
      this.taskMenu.set(null);
      return;
    }
    if (event.key === "Escape" && this.labelsOpen()) {
      event.preventDefault();
      this.labelsOpen.set(false);
      return;
    }
    if (event.key === "Escape" && this.aboutOpen()) {
      event.preventDefault();
      this.aboutOpen.set(false);
      return;
    }
    // A bare keystroke should land in the capture box, so a thought can be
    // typed without reaching for the mouse first.
    const typing = (event.target as HTMLElement | null)?.closest("input, textarea");
    if (!typing && !event.metaKey && !event.ctrlKey && event.key.length === 1) {
      this.focusCapture();
    }
  }

  /** Rename a project or tag everywhere it is used, merging duplicates. */
  async renameLabel(edit: LabelEdit): Promise<void> {
    if (edit.kind === "projects") {
      await this.tasks.renameProject(edit.from, edit.to);
      // The filter holds a name, not an id, so it has to follow the rename.
      if (this.projectFilter() === edit.from) {
        this.projectFilter.set(edit.to);
      }
    } else {
      await this.tasks.renameTag(edit.from, edit.to);
    }
  }

  /** Clear a project or tag from every task that carries it. */
  async removeLabel(target: { kind: LabelKind; name: string }): Promise<void> {
    if (target.kind === "projects") {
      await this.tasks.deleteProject(target.name);
      if (this.projectFilter() === target.name) {
        this.projectFilter.set("");
      }
    } else {
      await this.tasks.deleteTag(target.name);
    }
  }

  /** Open the context menu against a task, at the cursor. */
  openTaskMenu(task: Task, event: MouseEvent): void {
    this.taskMenu.set({ task, x: event.clientX, y: event.clientY });
  }

  async runTaskAction(action: TaskAction): Promise<void> {
    const open = this.taskMenu();
    if (!open) {
      return;
    }
    // Close first: every branch below is either instant or awaits the store,
    // and a menu left hanging over a task that has just been deleted is worse
    // than one that closes a fraction early.
    this.taskMenu.set(null);
    const task = open.task;

    switch (action) {
      case "open":
        this.open(task);
        break;
      case "done":
        await this.toggleDone(task, true);
        break;
      case "reopen":
        await this.toggleDone(task, false);
        break;
      case "today":
        await this.toggleToday(task);
        break;
      case "timer":
        await this.toggleTimerFor(task);
        break;
      case "delete":
        // The detail panel may be showing the task that is about to go.
        if (this.editing()?.id === task.id) {
          this.editing.set(null);
        }
        await this.remove(task);
        break;
    }
  }

  async assignUnassigned(taskId: string): Promise<void> {
    const session = this.unassignedSession();
    this.unassignedSession.set(null);
    if (session) {
      await this.tasks.assignSession(session.id, taskId);
    }
  }

  /** Leave it as unattributed time, which is a real answer rather than a dodge. */
  dismissUnassigned(): void {
    this.unassignedSession.set(null);
  }

  async toggleTimerPause(): Promise<void> {
    const timer = this.tasks.timer();
    if (!timer?.running) {
      return;
    }
    if (timer.paused) {
      await this.tasks.resumeTimer();
    } else {
      await this.tasks.pauseTimer();
    }
  }

  async reassignSession(change: { sessionId: string; taskId: string | null }): Promise<void> {
    await this.tasks.assignSession(change.sessionId, change.taskId);
  }

  async removeSession(sessionId: string): Promise<void> {
    await this.tasks.deleteSession(sessionId);
  }

}

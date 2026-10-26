import {
  ChangeDetectionStrategy,
  Component,
  EventEmitter,
  Input,
  Output,
  computed,
  inject,
  signal
} from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";

import { CapturePreview, Session, Task } from "../models/task.models";
import { TasksService } from "../services/tasks.service";

/**
 * Asking what a finished session counted towards.
 *
 * Only ever shown for focus time that was recorded against no task, and only
 * once the session is already safely in the store — the question is about
 * attribution, so nothing is lost by dismissing it.
 */
@Component({
  selector: "app-assign-session",
  standalone: true,
  imports: [CommonModule, FormsModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="backdrop">
      <div class="panel">
        <header>
          <i class="pi" [ngClass]="isMeeting ? 'pi-users' : 'pi-clock'"></i>
          <div>
            <h2>{{ minutes }} minutes {{ isMeeting ? "in a meeting" : "recorded" }}</h2>
            <p>What was that against?</p>
          </div>
        </header>

        @if (candidates().length) {
          <input
            class="filter"
            type="text"
            placeholder="Find a task, or type a new one…"
            [(ngModel)]="query"
            (ngModelChange)="queryChanged($event)"
            autofocus
          />
          <ul class="list">
            <!-- What you typed, as a new task. First rather than last: when
                 nothing matches, this is the answer, and having to look past
                 an empty list to find it would be backwards. -->
            @if (typed().trim()) {
              <li>
                <button type="button" class="make" (click)="created.emit(typed().trim())">
                  <span class="title">
                    Create <strong>{{ captured().title }}</strong>
                  </span>
                  <span class="made">
                    @if (captured().project) {
                      <span class="chip">{{ captured().project }}</span>
                    }
                    @for (tag of captured().tags; track tag) {
                      <span class="chip tag">{{ tag }}</span>
                    }
                    @if (captured().due) {
                      <span class="chip">due {{ captured().due }}</span>
                    }
                    <span class="chip time">+{{ minutes }}m</span>
                  </span>
                </button>
              </li>
            }

            @for (task of candidates(); track task.id) {
              <li>
                <button type="button" (click)="assigned.emit(task.id)">
                  <span class="title">{{ task.title }}</span>
                  <span class="project" *ngIf="task.project">{{ task.project }}</span>
                </button>
              </li>
            }
          </ul>
        } @else {
          <input
            class="filter"
            type="text"
            placeholder="What was it about?"
            [(ngModel)]="query"
            (ngModelChange)="queryChanged($event)"
            (keydown.enter)="query.trim() && created.emit(query.trim())"
            autofocus
          />
          @if (query.trim()) {
            <ul class="list">
              <li>
                <button type="button" class="make" (click)="created.emit(query.trim())">
                  <span class="title">Create <strong>{{ captured().title }}</strong></span>
                  <span class="made"><span class="chip time">+{{ minutes }}m</span></span>
                </button>
              </li>
            </ul>
          } @else {
            <p class="empty">
              Nothing open to attribute it to. Type what it was about and it becomes a task with
              this time already on it, or leave it unattributed — which is a real answer.
            </p>
          }
        }

        <footer>
          <button type="button" class="ghost" (click)="skipped.emit()">Leave unattributed</button>
        </footer>
      </div>
    </div>
  `,
  styles: [
    `
      .backdrop {
        position: fixed;
        inset: 0;
        z-index: 70;
        display: flex;
        align-items: center;
        justify-content: center;
        background: rgba(2, 10, 20, 0.55);
        backdrop-filter: blur(3px);
      }
      .panel {
        width: min(26rem, calc(100vw - 3rem));
        max-height: 70vh;
        display: flex;
        flex-direction: column;
        padding: 1.1rem 1.2rem 1rem;
        background: var(--panel-raised);
        border: 1px solid var(--border);
        border-radius: 14px;
        box-shadow: 0 30px 70px rgba(0, 0, 0, 0.45);
      }
      header {
        display: flex;
        gap: 0.7rem;
        align-items: flex-start;
        margin-bottom: 0.9rem;
      }
      header .pi {
        margin-top: 0.15rem;
        color: var(--accent);
      }
      h2 {
        margin: 0;
        font-size: 1rem;
        color: var(--ink-strong);
      }
      header p {
        margin: 0.15rem 0 0;
        font-size: 0.8rem;
        color: var(--ink-faint);
      }
      .filter {
        padding: 0.4rem 0.6rem;
        margin-bottom: 0.6rem;
        border: 1px solid var(--border);
        border-radius: 8px;
        background: var(--surface);
        color: var(--ink-strong);
        font: inherit;
        font-size: 0.85rem;
        outline: none;
      }
      .filter:focus {
        border-color: var(--accent);
      }
      .list {
        list-style: none;
        margin: 0;
        padding: 0;
        overflow-y: auto;
        flex: 1;
      }
      .list button {
        display: flex;
        align-items: baseline;
        gap: 0.5rem;
        width: 100%;
        padding: 0.45rem 0.5rem;
        border: none;
        border-radius: 8px;
        background: transparent;
        color: var(--ink);
        font: inherit;
        font-size: 0.88rem;
        text-align: left;
        cursor: pointer;
      }
      .list button:hover {
        background: var(--hover);
      }
      .title {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .project {
        font-size: 0.7rem;
        color: var(--ink-faint);
      }
      /* The "create this" row. Marked out from the tasks below it, because it
         does something different: those attribute, this makes. */
      .make {
        border-left: 2px solid var(--accent);
      }
      .make .title strong {
        color: var(--ink-strong);
        font-weight: 600;
      }
      .made {
        display: flex;
        align-items: center;
        gap: 4px;
        flex-wrap: wrap;
      }
      .chip {
        padding: 1px 6px;
        border-radius: 5px;
        background: var(--hover);
        color: var(--ink-faint);
        font-size: 10.5px;
      }
      .chip.tag {
        color: var(--link);
      }
      /* The minutes about to be recorded against it, so there is no doubt
         what pressing this does with the time. */
      .chip.time {
        background: var(--selected);
        color: var(--accent);
      }

      .empty {
        margin: 0;
        font-size: 0.8rem;
        line-height: 1.5;
        color: var(--ink-faint);
      }
      footer {
        display: flex;
        justify-content: flex-end;
        margin-top: 0.8rem;
      }
      .ghost {
        border: 1px solid var(--border);
        border-radius: 8px;
        padding: 0.3rem 0.7rem;
        background: transparent;
        color: var(--ink-muted);
        font-size: 0.78rem;
        cursor: pointer;
      }
      .ghost:hover {
        color: var(--ink-strong);
        border-color: var(--ink-faint);
      }
    `
  ]
})
export class AssignSessionComponent {
  @Input({ required: true }) session!: Session;
  /** Open tasks, most recently worked first. */
  @Input() tasks: Task[] = [];

  @Output() readonly assigned = new EventEmitter<string>();
  /** A line to make a task from, in the usual capture syntax. */
  @Output() readonly created = new EventEmitter<string>();
  @Output() readonly skipped = new EventEmitter<void>();

  private readonly tasksService = inject(TasksService);

  query = "";
  private readonly filter = signal("");

  get isMeeting(): boolean {
    return this.session?.kind === "meeting";
  }

  get minutes(): number {
    return Math.max(1, Math.round(this.session.seconds / 60));
  }

  readonly candidates = computed(() => {
    const needle = this.filter().trim().toLowerCase();
    const open = this.tasks.filter((task) => task.status !== "done");
    const matching = needle
      ? open.filter(
          (task) =>
            task.title.toLowerCase().includes(needle) ||
            (task.project ?? "").toLowerCase().includes(needle)
        )
      : open;
    // A long list is a worse prompt than a short one; filtering reaches the rest.
    return matching.slice(0, 8);
  });

  /** What is in the box, for the "create this" row. */
  readonly typed = computed(() => this.filter());

  /**
   * What the typed line would become.
   *
   * Parsed by the same code the real capture uses, through a command, rather
   * than a lighter version here: two sets of rules about what `(dbc/dfm)` or
   * `due:tomorrow` mean would drift, and the preview would start promising
   * something the capture does not deliver.
   */
  readonly captured = signal<CapturePreview>({ title: "", tags: [] });

  queryChanged(value: string): void {
    this.filter.set(value);
    const line = value.trim();
    if (!line) {
      this.captured.set({ title: "", tags: [] });
      return;
    }
    void this.tasksService.previewCapture(line).then((preview) => {
      if (preview && this.filter().trim() === line) {
        this.captured.set(preview);
      }
    });
  }
}

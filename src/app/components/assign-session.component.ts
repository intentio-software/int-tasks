import { ChangeDetectionStrategy, Component, EventEmitter, Input, Output, computed, signal } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";

import { Session, Task } from "../models/task.models";

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
          <i class="pi pi-clock"></i>
          <div>
            <h2>{{ minutes }} minutes recorded</h2>
            <p>What was that against?</p>
          </div>
        </header>

        @if (candidates().length) {
          <input
            class="filter"
            type="text"
            placeholder="Filter tasks…"
            [(ngModel)]="query"
            (ngModelChange)="queryChanged($event)"
            autofocus
          />
          <ul class="list">
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
          <p class="empty">
            No open tasks to attribute it to. The time is still recorded — it will show as
            unattributed in the time summary.
          </p>
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
  @Output() readonly skipped = new EventEmitter<void>();

  query = "";
  private readonly filter = signal("");

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

  queryChanged(value: string): void {
    this.filter.set(value);
  }
}

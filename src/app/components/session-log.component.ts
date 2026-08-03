import { ChangeDetectionStrategy, Component, EventEmitter, Input, Output, computed, signal } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";

import { Session, Task } from "../models/task.models";

/** A day's worth of recorded work. */
interface SessionDay {
  label: string;
  minutes: number;
  sessions: Session[];
}

/**
 * What the focus timer actually recorded, and the means to correct it.
 *
 * Recorded time is a fact about the past, so this is deliberately not a general
 * editor: what it offers is the two corrections that are genuinely needed —
 * time attributed to the wrong task, and a timer that was left running.
 */
@Component({
  selector: "app-session-log",
  standalone: true,
  imports: [CommonModule, FormsModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="log">
      <div class="log-head">
        <h3>Recorded time</h3>
        <span class="total">{{ totalLabel() }}</span>
      </div>

      @if (days().length) {
        @for (day of days(); track day.label) {
          <div class="day">
            <div class="day-head">
              <span class="day-label">{{ day.label }}</span>
              <span class="day-total">{{ day.minutes }}m</span>
            </div>

            @for (session of day.sessions; track session.id) {
              <div class="entry" [class.break]="session.kind === 'break'">
                <span class="when">{{ time(session) }}</span>
                <span class="length">{{ minutes(session) }}m</span>

                @if (session.kind === 'break') {
                  <span class="against break-label">Break</span>
                } @else if (editing() === session.id) {
                  <select
                    class="reassign"
                    [ngModel]="session.task_id ?? ''"
                    (ngModelChange)="reassign(session, $event)"
                  >
                    <option value="">Unattributed</option>
                    @for (task of tasks; track task.id) {
                      <option [value]="task.id">{{ task.title }}</option>
                    }
                  </select>
                } @else {
                  <button
                    type="button"
                    class="against"
                    [class.unattributed]="!session.task_id"
                    title="Change what this counted towards"
                    (click)="editing.set(session.id)"
                  >
                    {{ titleFor(session) }}
                  </button>
                }

                <span class="flag" *ngIf="!session.completed" title="Stopped early">·</span>
                <button
                  type="button"
                  class="remove"
                  title="Remove this session"
                  (click)="removed.emit(session.id)"
                >
                  <i class="pi pi-times"></i>
                </button>
              </div>
            }
          </div>
        }
      } @else {
        <p class="empty">No time recorded yet. Start a focus session and it will show up here.</p>
      }
    </div>
  `,
  styles: [
    `
      .log {
        max-width: 34rem;
        margin: 1.5rem auto 0;
        padding: 0 1rem 1rem;
      }
      .log-head {
        display: flex;
        align-items: baseline;
        gap: 0.6rem;
        padding-bottom: 0.4rem;
        border-bottom: 1px solid var(--border);
      }
      h3 {
        margin: 0;
        flex: 1;
        font-size: 0.85rem;
        font-weight: 600;
        color: var(--ink-muted);
        text-transform: uppercase;
        letter-spacing: 0.06em;
      }
      .total {
        font-size: 0.78rem;
        color: var(--ink-faint);
      }
      .day {
        margin-top: 0.8rem;
      }
      .day-head {
        display: flex;
        align-items: baseline;
        gap: 0.5rem;
        margin-bottom: 0.15rem;
      }
      .day-label {
        flex: 1;
        font-size: 0.75rem;
        color: var(--ink-muted);
      }
      .day-total {
        font-size: 0.7rem;
        color: var(--ink-faint);
      }
      .entry {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        padding: 0.3rem 0.3rem;
        border-radius: 8px;
        font-size: 0.82rem;
      }
      .entry:hover {
        background: var(--hover);
      }
      .entry.break {
        opacity: 0.6;
      }
      .when {
        width: 3.2rem;
        color: var(--ink-faint);
        font-variant-numeric: tabular-nums;
      }
      .length {
        width: 2.6rem;
        color: var(--ink);
        font-variant-numeric: tabular-nums;
      }
      .against {
        flex: 1;
        min-width: 0;
        padding: 0.1rem 0.3rem;
        border: none;
        border-radius: 6px;
        background: transparent;
        color: var(--ink);
        font: inherit;
        font-size: 0.82rem;
        text-align: left;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        cursor: pointer;
      }
      .against:hover {
        background: var(--surface);
      }
      .against.unattributed {
        color: var(--ink-faint);
        font-style: italic;
      }
      .break-label {
        cursor: default;
        color: var(--ink-faint);
      }
      .reassign {
        flex: 1;
        min-width: 0;
        padding: 0.1rem 0.3rem;
        border: 1px solid var(--accent);
        border-radius: 6px;
        background: var(--surface);
        color: var(--ink-strong);
        font: inherit;
        font-size: 0.8rem;
      }
      .flag {
        color: var(--ink-faint);
      }
      .remove {
        border: none;
        background: transparent;
        color: var(--ink-faint);
        font-size: 0.7rem;
        cursor: pointer;
        opacity: 0;
      }
      .entry:hover .remove {
        opacity: 1;
      }
      .remove:hover {
        color: var(--danger);
      }
      .empty {
        margin: 1rem 0 0;
        font-size: 0.8rem;
        color: var(--ink-faint);
      }
    `
  ]
})
export class SessionLogComponent {
  @Input() set sessions(value: Session[]) {
    this.entries.set(value ?? []);
  }
  /** Open tasks, for reattributing a session. */
  @Input() tasks: Task[] = [];

  @Output() readonly assigned = new EventEmitter<{ sessionId: string; taskId: string | null }>();
  @Output() readonly removed = new EventEmitter<string>();

  readonly editing = signal<string | null>(null);
  private readonly entries = signal<Session[]>([]);

  minutes(session: Session): number {
    return Math.max(1, Math.round(session.seconds / 60));
  }

  time(session: Session): string {
    return new Date(session.started_at).toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit"
    });
  }

  titleFor(session: Session): string {
    const task = this.tasks.find((candidate) => candidate.id === session.task_id);
    if (task) {
      return task.title;
    }
    // The task may have been deleted since; the time it took is still real.
    return session.task_id ? "Deleted task" : "Unattributed";
  }

  reassign(session: Session, taskId: string): void {
    this.editing.set(null);
    const next = taskId || null;
    if (next !== (session.task_id ?? null)) {
      this.assigned.emit({ sessionId: session.id, taskId: next });
    }
  }

  readonly totalLabel = computed(() => {
    const seconds = this.entries()
      .filter((session) => session.kind === "focus")
      .reduce((sum, session) => sum + session.seconds, 0);
    const hours = Math.floor(seconds / 3600);
    const mins = Math.round((seconds % 3600) / 60);
    return hours ? `${hours}h ${mins}m focused` : `${mins}m focused`;
  });

  readonly days = computed<SessionDay[]>(() => {
    const grouped = new Map<string, Session[]>();
    for (const session of this.entries()) {
      const key = new Date(session.started_at).toDateString();
      const bucket = grouped.get(key);
      if (bucket) {
        bucket.push(session);
      } else {
        grouped.set(key, [session]);
      }
    }
    const today = new Date().toDateString();
    const yesterday = new Date(Date.now() - 86_400_000).toDateString();
    return [...grouped.entries()].map(([key, sessions]) => ({
      label: key === today ? "Today" : key === yesterday ? "Yesterday" : key,
      minutes: Math.round(
        sessions.filter((s) => s.kind === "focus").reduce((sum, s) => sum + s.seconds, 0) / 60
      ),
      sessions
    }));
  });
}

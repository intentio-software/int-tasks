import {
  ChangeDetectionStrategy,
  Component,
  EventEmitter,
  Input,
  Output,
  computed,
  signal
} from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";

import { Task } from "../models/task.models";

/**
 * The panel for everything a task can carry beyond its title.
 *
 * Nothing here is required. Capture stays a single line; this is where detail
 * gets added later, if it is ever worth adding at all.
 */
@Component({
  selector: "app-task-detail",
  standalone: true,
  imports: [CommonModule, FormsModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="backdrop" (click)="closed.emit()">
      <aside class="panel" (click)="$event.stopPropagation()">
        <header>
          <input
            class="title"
            type="text"
            [(ngModel)]="draftTitle"
            (blur)="commitTitle()"
            (keydown.enter)="commitTitle()"
          />
          <button type="button" class="close" aria-label="Close" (click)="closed.emit()">
            <i class="pi pi-times"></i>
          </button>
        </header>

        <label class="field">
          <span>Description</span>
          <textarea
            rows="4"
            placeholder="Anything worth remembering about this one."
            [(ngModel)]="draftNotes"
            (blur)="patch({ notes: draftNotes.trim() || null })"
          ></textarea>
        </label>

        <div class="row">
          <label class="field">
            <span>Due</span>
            <input type="date" [(ngModel)]="draftDue" (change)="patch({ due: draftDue || null })" />
          </label>
          <label class="field">
            <span>Priority</span>
            <select [(ngModel)]="draftPriority" (change)="patch({ priority: numberOrNull(draftPriority) })">
              <option value="">None</option>
              <option value="1">1 — highest</option>
              <option value="2">2</option>
              <option value="3">3</option>
              <option value="4">4</option>
            </select>
          </label>
        </div>

        <div class="row">
          <label class="field">
            <span>Project</span>
            <!-- A datalist rather than a select: typing a new project must stay
                 as quick as picking an existing one, but offering what already
                 exists is what stops Intentio and intentio both appearing. -->
            <input
              type="text"
              list="known-projects"
              placeholder="e.g. Intentio Tasks"
              [(ngModel)]="draftProject"
              (blur)="patch({ project: draftProject.trim() || null })"
            />
            <datalist id="known-projects">
              @for (name of knownProjects; track name) {
                <option [value]="name"></option>
              }
            </datalist>
          </label>
          <label class="field">
            <span>Estimate (min)</span>
            <input
              type="number"
              min="0"
              [(ngModel)]="draftEstimate"
              (change)="patch({ estimateMinutes: numberOrNull(draftEstimate) })"
            />
          </label>
        </div>

        <label class="field">
          <span>Type tags</span>
          <input
            type="text"
            placeholder="bug, admin, deep-work"
            [(ngModel)]="draftTags"
            (blur)="commitTags()"
          />
          @if (knownTags.length) {
            <div class="suggested">
              @for (name of knownTags; track name) {
                <button type="button" (click)="addTag(name)" [disabled]="hasTag(name)">
                  {{ name }}
                </button>
              }
            </div>
          }
        </label>

        <!-- Scoring is the part that puts a task on the matrix, so it gets the
             most room and says what the numbers will mean. -->
        <div class="scoring">
          <div class="score">
            <span class="label">Impact <b>{{ draftImpact || '—' }}</b></span>
            <input type="range" min="0" max="10" [(ngModel)]="draftImpact" (change)="commitScores()" />
            <span class="hint">How much finishing it is worth</span>
          </div>
          <div class="score">
            <span class="label">Effort <b>{{ draftEffort || '—' }}</b></span>
            <input type="range" min="0" max="10" [(ngModel)]="draftEffort" (change)="commitScores()" />
            <span class="hint">How much it will cost you</span>
          </div>
          <p class="quadrant" *ngIf="quadrantLabel()">
            On the matrix: <b>{{ quadrantLabel() }}</b>
          </p>
          <p class="quadrant muted" *ngIf="!quadrantLabel()">
            Score both to place this on the matrix. Zero means unscored.
          </p>
        </div>

        <footer>
          <button type="button" class="danger" (click)="deleted.emit()">Delete</button>
          <button type="button" class="ghost" (click)="closed.emit()">Done</button>
        </footer>
      </aside>
    </div>
  `,
  styles: [
    `
      .backdrop {
        position: fixed;
        inset: 0;
        z-index: 50;
        display: flex;
        justify-content: flex-end;
        background: rgba(2, 10, 20, 0.45);
      }
      .panel {
        width: min(24rem, 100%);
        height: 100%;
        overflow-y: auto;
        padding: 1rem 1.1rem 2rem;
        background: var(--panel-raised);
        border-left: 1px solid var(--border);
        display: flex;
        flex-direction: column;
        gap: 0.8rem;
      }
      header {
        display: flex;
        align-items: flex-start;
        gap: 0.5rem;
      }
      .title {
        flex: 1;
        border: none;
        background: transparent;
        color: var(--ink-strong);
        font-size: 1rem;
        font-weight: 600;
        outline: none;
        padding: 0.2rem 0;
      }
      .close {
        border: none;
        background: transparent;
        color: var(--ink-faint);
        cursor: pointer;
      }
      .close:hover {
        color: var(--ink-strong);
      }
      .field {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
        flex: 1;
        min-width: 0;
      }
      .field > span {
        font-size: 0.7rem;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        color: var(--ink-faint);
      }
      input,
      select,
      textarea {
        width: 100%;
        padding: 0.4rem 0.55rem;
        border: 1px solid var(--border);
        border-radius: 8px;
        background: var(--surface);
        color: var(--ink);
        font: inherit;
        font-size: 0.85rem;
        outline: none;
      }
      input:focus,
      select:focus,
      textarea:focus {
        border-color: var(--accent);
      }
      textarea {
        resize: vertical;
        line-height: 1.5;
      }
      .row {
        display: flex;
        gap: 0.6rem;
      }
      .scoring {
        border: 1px solid var(--border);
        border-radius: 10px;
        padding: 0.7rem 0.8rem;
        display: flex;
        flex-direction: column;
        gap: 0.7rem;
      }
      .score {
        display: flex;
        flex-direction: column;
        gap: 0.2rem;
      }
      .score .label {
        font-size: 0.78rem;
        color: var(--ink-muted);
      }
      .score .label b {
        color: var(--accent);
      }
      .score input[type="range"] {
        padding: 0;
        border: none;
        background: transparent;
        accent-color: var(--accent);
      }
      .hint {
        font-size: 0.68rem;
        color: var(--ink-faint);
      }
      .quadrant {
        margin: 0;
        font-size: 0.78rem;
        color: var(--ink-muted);
      }
      .quadrant.muted {
        color: var(--ink-faint);
      }
      footer {
        display: flex;
        justify-content: space-between;
        margin-top: auto;
        padding-top: 0.8rem;
      }
      button.ghost,
      button.danger {
        padding: 0.35rem 0.9rem;
        border-radius: 999px;
        border: 1px solid var(--border);
        background: transparent;
        color: var(--ink);
        font-size: 0.8rem;
        cursor: pointer;
      }
      button.danger {
        color: var(--danger);
        border-color: transparent;
      }
      button.danger:hover {
        border-color: var(--danger);
      }
      button.ghost:hover {
        border-color: var(--accent);
      }
    `
  ]
})
export class TaskDetailComponent {
  /** Projects already in use, offered as autocomplete. */
  @Input() knownProjects: string[] = [];
  /** Tags already in use, offered as one-click chips. */
  @Input() knownTags: string[] = [];

  @Input({ required: true }) set task(value: Task) {
    this.current = value;
    this.draftTitle = value.title;
    this.draftNotes = value.notes ?? "";
    this.draftDue = value.due ?? "";
    this.draftProject = value.project ?? "";
    this.draftTags = (value.tags ?? []).join(", ");
    this.draftPriority = value.priority?.toString() ?? "";
    this.draftEstimate = value.estimate_minutes?.toString() ?? "";
    this.impact.set(value.impact ?? 0);
    this.effort.set(value.effort ?? 0);
  }

  @Output() readonly changed = new EventEmitter<Record<string, unknown>>();
  @Output() readonly scored = new EventEmitter<{ impact: number | null; effort: number | null }>();
  @Output() readonly deleted = new EventEmitter<void>();
  @Output() readonly closed = new EventEmitter<void>();

  private current!: Task;

  draftTitle = "";
  draftNotes = "";
  draftDue = "";
  draftProject = "";
  draftTags = "";
  draftPriority = "";
  draftEstimate = "";

  private readonly impact = signal(0);
  private readonly effort = signal(0);

  get draftImpact(): number {
    return this.impact();
  }
  set draftImpact(value: number) {
    this.impact.set(Number(value));
  }

  get draftEffort(): number {
    return this.effort();
  }
  set draftEffort(value: number) {
    this.effort.set(Number(value));
  }

  /** Mirrors the quadrant rule in `matrix.rs`, so the panel agrees with the plot. */
  readonly quadrantLabel = computed(() => {
    const impact = this.impact();
    const effort = this.effort();
    if (!impact || !effort) {
      return "";
    }
    if (impact > 5) {
      return effort > 5 ? "Big bets" : "Quick wins";
    }
    return effort > 5 ? "Thankless" : "Fill-ins";
  });

  patch(fields: Record<string, unknown>): void {
    this.changed.emit(fields);
  }

  commitTitle(): void {
    const title = this.draftTitle.trim();
    // An empty title would leave a task that cannot be identified.
    if (!title || title === this.current.title) {
      this.draftTitle = this.current.title;
      return;
    }
    this.patch({ title });
  }

  commitTags(): void {
    const tags = this.draftTags
      .split(",")
      .map((tag) => tag.trim())
      .filter(Boolean);
    this.patch({ tags });
  }

  commitScores(): void {
    // Zero is the UI's way of saying "unscored", which clears the field rather
    // than pinning the task to the corner of the matrix.
    this.scored.emit({
      impact: this.impact() || null,
      effort: this.effort() || null
    });
  }

  numberOrNull(value: string): number | null {
    const parsed = Number(value);
    return value === "" || Number.isNaN(parsed) ? null : parsed;
  }
  /** Tags currently in the draft field, normalised for comparison. */
  private currentTags(): string[] {
    return this.draftTags
      .split(",")
      .map((tag) => tag.trim())
      .filter(Boolean);
  }

  hasTag(name: string): boolean {
    return this.currentTags().some((tag) => tag.toLowerCase() === name.toLowerCase());
  }

  /** Append a known tag from the chip row. */
  addTag(name: string): void {
    if (this.hasTag(name)) {
      return;
    }
    this.draftTags = [...this.currentTags(), name].join(", ");
    this.commitTags();
  }

}

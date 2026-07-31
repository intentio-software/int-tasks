import { ChangeDetectionStrategy, Component, EventEmitter, Input, Output, signal } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";

import { LabelUse } from "../models/task.models";

export type LabelKind = "projects" | "tags";
export interface LabelEdit {
  kind: LabelKind;
  from: string;
  to: string;
}

/**
 * Managing projects and type tags.
 *
 * Both are free text on a task, which is what makes them frictionless to add
 * and what guarantees they drift — `Intentio`, `intentio`, `Intentio `. Renaming
 * is therefore the important operation, not creating: it is how near-duplicates
 * get merged back together.
 */
@Component({
  selector: "app-labels-dialog",
  standalone: true,
  imports: [CommonModule, FormsModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="backdrop" (click)="closed.emit()">
      <div class="panel" (click)="$event.stopPropagation()">
        <header>
          <h2>Projects &amp; tags</h2>
          <button type="button" class="close" aria-label="Close" (click)="closed.emit()">
            <i class="pi pi-times"></i>
          </button>
        </header>

        <div class="tabs" role="group">
          <button type="button" [class.on]="tab() === 'projects'" (click)="tab.set('projects')">
            Projects <span class="n">{{ projects.length }}</span>
          </button>
          <button type="button" [class.on]="tab() === 'tags'" (click)="tab.set('tags')">
            Tags <span class="n">{{ tags.length }}</span>
          </button>
        </div>

        @if (current().length) {
          <ul class="list">
            @for (label of current(); track label.name) {
              <li>
                @if (editing() === label.name) {
                  <input
                    class="rename"
                    type="text"
                    [(ngModel)]="draft"
                    (keydown.enter)="commit(label.name)"
                    (keydown.escape)="editing.set(null)"
                    autofocus
                  />
                  <button type="button" class="link" (click)="commit(label.name)">Save</button>
                  <button type="button" class="link muted" (click)="editing.set(null)">Cancel</button>
                } @else {
                  <span class="name">{{ label.name }}</span>
                  <span class="counts">
                    {{ label.open }} open<span *ngIf="label.done">, {{ label.done }} done</span>
                  </span>
                  <button type="button" class="link" (click)="startRename(label.name)">Rename</button>
                  <button type="button" class="link danger" (click)="remove.emit({ kind: tab(), name: label.name })">
                    Remove
                  </button>
                }
              </li>
            }
          </ul>
          <p class="hint">
            Renaming one onto another merges them. Removing clears the label from its tasks — the
            tasks themselves are untouched.
          </p>
        } @else {
          <p class="empty">
            No {{ tab() }} yet. Add one on a task and it will appear here.
          </p>
        }
      </div>
    </div>
  `,
  styles: [
    `
      .backdrop {
        position: fixed;
        inset: 0;
        z-index: 55;
        display: flex;
        align-items: center;
        justify-content: center;
        background: rgba(2, 10, 20, 0.5);
        backdrop-filter: blur(3px);
      }
      .panel {
        width: min(30rem, calc(100vw - 3rem));
        max-height: 70vh;
        display: flex;
        flex-direction: column;
        padding: 1rem 1.1rem 1.2rem;
        background: var(--panel-raised);
        border: 1px solid var(--border);
        border-radius: 14px;
        box-shadow: 0 30px 70px rgba(0, 0, 0, 0.45);
      }
      header {
        display: flex;
        align-items: center;
        margin-bottom: 0.7rem;
      }
      h2 {
        margin: 0;
        flex: 1;
        font-size: 1rem;
        color: var(--ink-strong);
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
      .tabs {
        display: inline-flex;
        padding: 2px;
        margin-bottom: 0.8rem;
        border: 1px solid var(--border);
        border-radius: 999px;
        align-self: flex-start;
      }
      .tabs button {
        padding: 0.2rem 0.85rem;
        border: none;
        border-radius: 999px;
        background: transparent;
        color: var(--ink-faint);
        font-size: 0.78rem;
        cursor: pointer;
      }
      .tabs button.on {
        background: var(--accent);
        color: #fff;
      }
      .tabs .n {
        opacity: 0.7;
        margin-left: 0.2rem;
      }
      .list {
        list-style: none;
        margin: 0;
        padding: 0;
        overflow-y: auto;
        flex: 1;
      }
      .list li {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        padding: 0.45rem 0.2rem;
        border-bottom: 1px solid var(--border);
      }
      .name {
        color: var(--ink);
        font-size: 0.88rem;
      }
      .counts {
        flex: 1;
        color: var(--ink-faint);
        font-size: 0.72rem;
      }
      .rename {
        flex: 1;
        padding: 0.25rem 0.45rem;
        border: 1px solid var(--accent);
        border-radius: 6px;
        background: var(--surface);
        color: var(--ink-strong);
        font: inherit;
        font-size: 0.85rem;
        outline: none;
      }
      .link {
        border: none;
        background: transparent;
        color: var(--ink-muted);
        font-size: 0.75rem;
        cursor: pointer;
        padding: 0;
      }
      .link:hover {
        color: var(--accent);
      }
      .link.danger:hover {
        color: var(--danger);
      }
      .link.muted {
        color: var(--ink-faint);
      }
      .hint,
      .empty {
        margin: 0.8rem 0 0;
        font-size: 0.75rem;
        line-height: 1.5;
        color: var(--ink-faint);
      }
    `
  ]
})
export class LabelsDialogComponent {
  @Input() projects: LabelUse[] = [];
  @Input() tags: LabelUse[] = [];

  @Output() readonly renamed = new EventEmitter<LabelEdit>();
  @Output() readonly remove = new EventEmitter<{ kind: LabelKind; name: string }>();
  @Output() readonly closed = new EventEmitter<void>();

  readonly tab = signal<LabelKind>("projects");
  readonly editing = signal<string | null>(null);
  draft = "";

  current(): LabelUse[] {
    return this.tab() === "projects" ? this.projects : this.tags;
  }

  startRename(name: string): void {
    this.draft = name;
    this.editing.set(name);
  }

  commit(from: string): void {
    const to = this.draft.trim();
    this.editing.set(null);
    // A no-op rename would still rewrite every task's timestamp.
    if (to && to !== from) {
      this.renamed.emit({ kind: this.tab(), from, to });
    }
  }
}

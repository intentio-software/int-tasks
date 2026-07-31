import {
  AfterViewInit,
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  EventEmitter,
  HostListener,
  Input,
  Output,
  ViewChild,
  signal
} from "@angular/core";
import { CommonModule } from "@angular/common";

import { Task } from "../models/task.models";

/** What the menu can do to a task. */
export type TaskAction = "open" | "done" | "reopen" | "today" | "timer" | "delete";

/**
 * The right-click menu on a task.
 *
 * Everything here is already reachable from the row itself; the menu exists so
 * that acting on a task does not depend on finding a control that only appears
 * on hover. Delete is the one thing with no other home on the row, which is
 * why it is here at all — and why it sits alone at the bottom.
 */
@Component({
  selector: "app-task-menu",
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div
      #menu
      class="menu"
      role="menu"
      [style.left.px]="left()"
      [style.top.px]="top()"
      (contextmenu)="$event.preventDefault()"
    >
      <button type="button" role="menuitem" (click)="pick('open')">
        <i class="pi pi-pencil"></i> Open details
      </button>

      @if (task.status === 'done') {
        <button type="button" role="menuitem" (click)="pick('reopen')">
          <i class="pi pi-replay"></i> Reopen
        </button>
      } @else {
        <button type="button" role="menuitem" (click)="pick('done')">
          <i class="pi pi-check"></i> Complete
        </button>
      }

      <button type="button" role="menuitem" (click)="pick('today')">
        <i class="pi" [ngClass]="task.today ? 'pi-star-fill' : 'pi-star'"></i>
        {{ task.today ? "Remove from Today" : "Pull onto Today" }}
      </button>

      <button type="button" role="menuitem" (click)="pick('timer')">
        <i class="pi" [ngClass]="running ? 'pi-stop-circle' : 'pi-play-circle'"></i>
        {{ running ? "Stop the timer" : "Start a focus session" }}
      </button>

      <div class="sep"></div>

      <button type="button" role="menuitem" class="danger" (click)="pick('delete')">
        <i class="pi pi-trash"></i> Delete
      </button>
    </div>
  `,
  styles: [
    `
      .menu {
        position: fixed;
        z-index: 60;
        min-width: 12.5rem;
        padding: 0.25rem;
        background: var(--panel-raised);
        border: 1px solid var(--border);
        border-radius: 10px;
        box-shadow: 0 16px 40px rgba(0, 0, 0, 0.4);
      }
      button {
        display: flex;
        align-items: center;
        gap: 0.55rem;
        width: 100%;
        padding: 0.38rem 0.55rem;
        border: none;
        border-radius: 7px;
        background: transparent;
        color: var(--ink);
        font: inherit;
        font-size: 0.82rem;
        text-align: left;
        cursor: pointer;
      }
      button:hover {
        background: var(--hover);
      }
      button i {
        width: 0.9rem;
        font-size: 0.78rem;
        color: var(--ink-faint);
      }
      button:hover i {
        color: var(--accent);
      }
      button.danger:hover,
      button.danger:hover i {
        color: var(--danger);
      }
      .sep {
        height: 1px;
        margin: 0.25rem 0.3rem;
        background: var(--border);
      }
    `
  ]
})
export class TaskMenuComponent implements AfterViewInit {
  @Input({ required: true }) task!: Task;
  /** True when the timer is running against this task. */
  @Input() running = false;
  /** Where the click happened, in viewport coordinates. */
  @Input() x = 0;
  @Input() y = 0;

  @Output() readonly chosen = new EventEmitter<TaskAction>();
  @Output() readonly closed = new EventEmitter<void>();

  @ViewChild("menu") private menu?: ElementRef<HTMLElement>;

  readonly left = signal(0);
  readonly top = signal(0);

  ngAfterViewInit(): void {
    this.left.set(this.x);
    this.top.set(this.y);
    // Flip back over the cursor rather than off the edge. Measured after the
    // first render because the height depends on which items are shown.
    const element = this.menu?.nativeElement;
    if (!element) {
      return;
    }
    const { width, height } = element.getBoundingClientRect();
    const margin = 8;
    if (this.x + width + margin > window.innerWidth) {
      this.left.set(Math.max(margin, this.x - width));
    }
    if (this.y + height + margin > window.innerHeight) {
      this.top.set(Math.max(margin, this.y - height));
    }
  }

  pick(action: TaskAction): void {
    this.chosen.emit(action);
  }

  /**
   * Any press outside closes. `mousedown` rather than `click` so the menu is
   * gone before whatever was pressed reacts.
   */
  @HostListener("document:mousedown", ["$event"])
  onOutside(event: MouseEvent): void {
    if (!this.menu?.nativeElement.contains(event.target as Node)) {
      this.closed.emit();
    }
  }

  @HostListener("window:blur")
  @HostListener("window:resize")
  onDismiss(): void {
    this.closed.emit();
  }
}

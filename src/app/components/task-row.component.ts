import { ChangeDetectionStrategy, Component, EventEmitter, Input, Output } from "@angular/core";
import { CommonModule } from "@angular/common";

import { Task, TodayReason } from "../models/task.models";

/**
 * One task, as it appears in a list.
 *
 * The whole row is a target: the checkbox completes, the title opens, the play
 * button starts a session. Nothing is hidden behind a menu, because the point of
 * the Today list is to act on things without hunting for the control.
 */
@Component({
  selector: "app-task-row",
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div
      class="row"
      [class.done]="task.status === 'done'"
      [class.running]="running"
      [class.menued]="menuOpen"
      [class.compact]="compact"
      (contextmenu)="onContextMenu($event)"
    >
      <button
        type="button"
        class="check"
        [class.checked]="task.status === 'done'"
        [attr.aria-label]="task.status === 'done' ? 'Mark not done' : 'Mark done'"
        (click)="toggled.emit(task.status !== 'done')"
      >
        <i class="pi pi-check" *ngIf="task.status === 'done'"></i>
      </button>

      <div class="body" (click)="opened.emit()">
        <span class="title" [attr.title]="compact ? task.title : null">{{ task.title }}</span>
        <div class="meta">
          <span class="badge reason" *ngIf="reason" [ngClass]="reason">{{ reasonLabel }}</span>
          <span class="due" *ngIf="task.due" [class.overdue]="reason === 'overdue'">
            <i class="pi pi-calendar"></i> {{ task.due }}
          </span>
          <span class="owner" *ngIf="task.assignee" title="Owner">@{{ task.assignee }}</span>
          <span class="tag" *ngFor="let tag of task.tags">{{ tag }}</span>
          <span class="estimate" *ngIf="task.estimate_minutes">{{ task.estimate_minutes }}m</span>
        </div>
      </div>

      <button
        type="button"
        class="star"
        [class.on]="task.today"
        [title]="task.today ? 'Remove from Today' : 'Pull onto Today'"
        (click)="starToggled.emit()"
      >
        <i class="pi" [ngClass]="task.today ? 'pi-star-fill' : 'pi-star'"></i>
      </button>

      <button
        type="button"
        class="play"
        [class.active]="running"
        [title]="running ? 'Stop the timer' : 'Start a focus session on this task'"
        (click)="timerToggled.emit()"
      >
        <i class="pi" [ngClass]="running ? 'pi-stop-circle' : 'pi-play-circle'"></i>
      </button>
    </div>
  `,
  styles: [
    `
      /* An inline host lets the row be laid out as text rather than as the
         block it is. It happens to survive in a wide list and does not on a
         board card. */
      :host {
        display: block;
      }

      .row {
        display: flex;
        align-items: flex-start;
        gap: 0.7rem;
        padding: 0.55rem 0.7rem;
        border-radius: 10px;
        transition: background 0.12s ease;
      }
      .row:hover,
      .row.menued {
        background: var(--hover);
      }
      .row.running {
        background: color-mix(in srgb, var(--accent) 12%, transparent);
        box-shadow: inset 2px 0 0 var(--accent);
      }
      .row.done .title {
        text-decoration: line-through;
        color: var(--ink-faint);
      }

      .check {
        flex: none;
        width: 1.15rem;
        height: 1.15rem;
        margin-top: 0.15rem;
        border: 1.5px solid var(--border-strong, var(--ink-faint));
        border-radius: 50%;
        background: transparent;
        color: #fff;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        font-size: 0.6rem;
        cursor: pointer;
        transition: background 0.12s ease, border-color 0.12s ease;
      }
      .check:hover {
        border-color: var(--accent);
      }
      .check.checked {
        background: var(--accent);
        border-color: var(--accent);
      }

      .body {
        flex: 1;
        min-width: 0;
        cursor: pointer;
      }
      .title {
        display: block;
        color: var(--ink);
        font-size: 0.92rem;
        line-height: 1.4;
        /* 'anywhere' breaks mid-word whenever it is convenient, which in a
           narrow column turns "handling" into "handlin" and "g". Break a word
           only when the word itself cannot fit. */
        overflow-wrap: break-word;
      }
      .meta {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 0.4rem;
        margin-top: 0.2rem;
        font-size: 0.72rem;
        color: var(--ink-faint);
      }
      .meta i {
        font-size: 0.65rem;
      }
      .due.overdue {
        color: var(--danger);
      }
      .badge {
        padding: 0.05rem 0.4rem;
        border-radius: 999px;
        font-size: 0.65rem;
        text-transform: uppercase;
        letter-spacing: 0.05em;
      }
      .badge.overdue {
        background: color-mix(in srgb, var(--danger) 20%, transparent);
        color: var(--danger);
      }
      .badge.inprogress {
        background: color-mix(in srgb, var(--accent) 22%, transparent);
        color: var(--accent);
      }
      .badge.due,
      .badge.flagged {
        background: var(--hover);
      }
      .tag {
        padding: 0.05rem 0.4rem;
        border: 1px solid var(--border);
        border-radius: 999px;
      }

      .star {
        flex: none;
        border: none;
        background: transparent;
        color: var(--ink-faint);
        font-size: 0.9rem;
        cursor: pointer;
        opacity: 0;
        transition: opacity 0.12s ease, color 0.12s ease;
      }
      .row:hover .star,
      .row.menued .star,
      .star.on {
        opacity: 1;
      }
      .star.on,
      .star:hover {
        color: var(--accent);
      }

      .play {
        flex: none;
        border: none;
        background: transparent;
        color: var(--ink-faint);
        font-size: 1.05rem;
        cursor: pointer;
        opacity: 0;
        transition: opacity 0.12s ease, color 0.12s ease;
      }
      .row:hover .play,
      .row.menued .play,
      .play.active {
        opacity: 1;
      }
      .play:hover,
      .play.active {
        color: var(--accent);
      }

      /* ------------------------------------------------------- board cards */

      /* A card is a fifth of the width a list row has. The star and the play
         button are worth the same space there as here, which is to say most
         of the card, so on a card they stop taking any and appear over the
         corner when the pointer is on it. */
      .row.compact {
        position: relative;
        padding: 0.5rem 0.6rem;
        gap: 0.5rem;
      }
      .row.compact .star,
      .row.compact .play {
        position: absolute;
        top: 0.25rem;
        padding: 0 0.15rem;
        border-radius: 6px;
        background: var(--panel-raised, var(--surface));
      }
      .row.compact .star {
        right: 1.75rem;
      }
      .row.compact .play {
        right: 0.3rem;
      }
      /* Three lines, then an ellipsis: cards that are all the same height can
         be scanned, and the whole title is a click away in the detail panel
         and under the pointer as a tooltip. */
      .row.compact .title {
        display: -webkit-box;
        -webkit-line-clamp: 3;
        line-clamp: 3;
        -webkit-box-orient: vertical;
        overflow: hidden;
      }
    `
  ]
})
export class TaskRowComponent {
  @Input({ required: true }) task!: Task;
  /** Why this task is on Today, when shown there. */
  @Input() reason: TodayReason | null = null;
  /** True when the timer is currently running against this task. */
  @Input() running = false;

  @Output() readonly toggled = new EventEmitter<boolean>();
  @Output() readonly opened = new EventEmitter<void>();
  @Output() readonly timerToggled = new EventEmitter<void>();
  @Output() readonly starToggled = new EventEmitter<void>();
  @Output() readonly menuRequested = new EventEmitter<MouseEvent>();

  /** True while this row's context menu is showing, so it stays highlighted. */
  @Input() menuOpen = false;

  /**
   * Laid out for a board card rather than a list row.
   *
   * A card is narrow enough that the row's fixed chrome - check, star, play -
   * was taking more of it than the title.
   */
  @Input() compact = false;

  onContextMenu(event: MouseEvent): void {
    event.preventDefault();
    this.menuRequested.emit(event);
  }

  get reasonLabel(): string {
    switch (this.reason) {
      case "overdue":
        return "overdue";
      case "inprogress":
        return "in progress";
      case "due":
        return "today";
      case "flagged":
        return "starred";
      default:
        return "";
    }
  }
}

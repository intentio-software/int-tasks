import { ChangeDetectionStrategy, Component, EventEmitter, Input, Output, signal } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";

import { Task, TodayEntry } from "../models/task.models";

export interface TasksSyncState {
  status: {
    isRepo: boolean;
    hasRemote: boolean;
    branch?: string;
    dirty: boolean;
    ahead: number;
    behind: number;
    blocked?: string;
  };
  settings: { enabled: boolean; intervalSeconds: number };
  root: string;
}

export interface TeamMember {
  name: string;
  isMe: boolean;
  open: number;
  completedToday: number;
  pointsToday: number;
  today: TodayEntry[];
  assigned: Task[];
  recentlyDone: Task[];
  stats?: { streakDays: number; focusMinutesToday: number } | null;
  unavailable?: string;
}

/**
 * What the team is working on, and what they have finished.
 *
 * Finished work leads, because that is the part worth showing other people —
 * it says what moved. Focus time and streaks appear on your own card only;
 * they are not read for anyone else, so there is nothing here to compare and
 * nothing to game.
 */
@Component({
  selector: "app-team-view",
  standalone: true,
  imports: [CommonModule, FormsModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="team">
      <div class="section-head">
        <h2>Team</h2>
        <span class="count">{{ members.length }}</span>
        <span class="finished" *ngIf="finishedToday() as done">
          {{ done }} finished today between you
        </span>
        <span class="head-spacer"></span>
        <button type="button" class="ghost small" [disabled]="syncing" (click)="syncRequested.emit()">
          {{ syncing ? "Syncing…" : "Sync now" }}
        </button>
      </div>

      @if (sync?.status?.isRepo) {
        <div class="sync-bar">
          <label class="toggle">
            <input
              type="checkbox"
              [checked]="sync!.settings.enabled"
              (change)="onToggle($event)"
            />
            <span>Keep the team folder in sync</span>
          </label>

          <label class="interval" [class.dim]="!sync!.settings.enabled">
            <span>Fetch every</span>
            <select [disabled]="!sync!.settings.enabled" (change)="onInterval($event)">
              @for (choice of intervals; track choice.seconds) {
                <option [value]="choice.seconds" [selected]="choice.seconds === sync!.settings.intervalSeconds">
                  {{ choice.label }}
                </option>
              }
            </select>
          </label>

          <span class="detail">
            {{ sync!.status.branch }}
            <span *ngIf="sync!.status.ahead"> · {{ sync!.status.ahead }} to push</span>
            <span *ngIf="sync!.status.behind"> · {{ sync!.status.behind }} to pull</span>
            <span *ngIf="!sync!.status.hasRemote"> · no remote</span>
          </span>
        </div>

        <p class="behaviour">
          Your own work is committed once you have stopped for a couple of
          minutes, so a working session becomes one commit rather than twenty.
        </p>
      } @else if (sync) {
        <p class="blocked">
          {{ sync.root }} is not a Git repository, so there is nothing to sync
          with. Clone the team repository and point at your folder in it.
        </p>
      }

      <p class="blocked" *ngIf="syncMessage">{{ syncMessage }}</p>

      <div class="cards">
        @for (member of members; track member.name) {
          <article class="member" [class.me]="member.isMe">
            <header>
              <span class="name">{{ member.name }}</span>
              <span class="you" *ngIf="member.isMe">you</span>
              <span class="spacer"></span>
              <span class="open">{{ member.open }} open</span>
            </header>

            @if (member.unavailable) {
              <p class="unavailable">{{ member.unavailable }}</p>
            } @else {
              <p class="scoreline">
                <span [class.dim]="!member.completedToday">
                  {{ member.completedToday }} finished today
                </span>
                <span class="points" *ngIf="member.pointsToday">+{{ member.pointsToday }}</span>
                <!-- Only ever your own: a colleague's sessions are not read. -->
                <span class="mine" *ngIf="member.isMe && member.stats?.streakDays">
                  · {{ member.stats?.streakDays }} day streak
                </span>
              </p>

              @if (member.recentlyDone.length) {
                <ul class="done">
                  @for (task of member.recentlyDone.slice(0, 3); track task.id) {
                    <li><i class="pi pi-check"></i> {{ task.title }}</li>
                  }
                </ul>
              }

              @if (member.today.length) {
                <div class="group">
                  <span class="label">On today</span>
                  <ul>
                    @for (entry of member.today.slice(0, 4); track entry.id) {
                      <li>{{ entry.title }}</li>
                    }
                  </ul>
                </div>
              }

              @if (member.assigned.length) {
                <div class="group">
                  <span class="label">Handed over</span>
                  <ul>
                    @for (task of member.assigned.slice(0, 4); track task.id) {
                      <li>
                        {{ task.title }}
                        <span class="from" *ngIf="task.assigned_by">from {{ task.assigned_by }}</span>
                      </li>
                    }
                  </ul>
                </div>
              }

              @if (!member.isMe) {
                <form class="assign" (submit)="hand(member.name, $event)">
                  <input
                    type="text"
                    [placeholder]="'Give ' + member.name + ' something…'"
                    [(ngModel)]="drafts[member.name]"
                    [ngModelOptions]="{ standalone: true }"
                  />
                  <button type="submit" [disabled]="!drafts[member.name]">Assign</button>
                </form>
              }
            }
          </article>
        }
      </div>
    </section>
  `,
  styles: [
    `
      .team {
        max-width: 60rem;
        margin: 0 auto;
        padding: 0 1rem;
      }
      /* Declared here rather than inherited: view encapsulation keeps the
         shell's .section-head rules out of this component. */
      .section-head {
        display: flex;
        align-items: baseline;
        gap: 0.5rem;
        padding-bottom: 0.4rem;
      }
      .section-head h2 {
        margin: 0;
        font-size: 1rem;
        color: var(--ink-strong);
      }
      .section-head .count {
        font-size: 0.75rem;
        color: var(--ink-faint);
      }
      .ghost.small {
        border: 1px solid var(--border);
        border-radius: 8px;
        padding: 0.2rem 0.6rem;
        background: transparent;
        color: var(--ink-muted);
        font: inherit;
        font-size: 0.75rem;
        cursor: pointer;
      }
      .ghost.small:hover:not(:disabled) {
        color: var(--ink-strong);
        border-color: var(--ink-faint);
      }
      .ghost.small:disabled {
        opacity: 0.5;
        cursor: default;
      }
      .finished {
        margin-left: 0.5rem;
        font-size: 0.78rem;
        color: var(--accent);
      }
      /* Pushes the sync control to the far edge, away from the counts. */
      .head-spacer {
        flex: 1;
      }
      .sync-bar {
        display: flex;
        align-items: center;
        flex-wrap: wrap;
        gap: 0.9rem;
        margin-top: 0.5rem;
        padding: 0.5rem 0.7rem;
        border: 1px solid var(--border);
        border-radius: 10px;
        background: var(--panel);
        font-size: 0.78rem;
      }
      .toggle,
      .interval {
        display: flex;
        align-items: center;
        gap: 0.4rem;
      }
      .interval.dim {
        opacity: 0.5;
      }
      .interval select {
        padding: 0.12rem 0.3rem;
        border: 1px solid var(--border);
        border-radius: 6px;
        background: var(--surface);
        color: inherit;
        font: inherit;
        font-size: 0.75rem;
      }
      .detail {
        margin-left: auto;
        color: var(--ink-faint);
      }
      .behaviour {
        margin: 0.4rem 0 0;
        font-size: 0.72rem;
        line-height: 1.5;
        color: var(--ink-faint);
      }
      .blocked {
        margin: 0.4rem 0 0;
        font-size: 0.78rem;
        color: var(--accent);
      }
      .cards {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(17rem, 1fr));
        gap: 0.8rem;
        margin-top: 0.9rem;
      }
      .member {
        display: flex;
        flex-direction: column;
        padding: 0.8rem 0.9rem;
        border: 1px solid var(--border);
        border-radius: 12px;
        background: var(--panel);
      }
      .member.me {
        border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
      }
      header {
        display: flex;
        align-items: baseline;
        gap: 0.4rem;
      }
      .name {
        font-weight: 600;
        color: var(--ink-strong);
        text-transform: capitalize;
      }
      .you {
        font-size: 0.66rem;
        color: var(--accent);
      }
      .spacer {
        flex: 1;
      }
      .open {
        font-size: 0.72rem;
        color: var(--ink-faint);
      }
      .scoreline {
        margin: 0.45rem 0 0;
        font-size: 0.8rem;
        color: var(--ink);
      }
      .scoreline .dim {
        color: var(--ink-faint);
      }
      .points {
        margin-left: 0.35rem;
        color: var(--accent);
      }
      .mine {
        color: var(--ink-faint);
      }
      .done {
        list-style: none;
        margin: 0.5rem 0 0;
        padding: 0;
      }
      .done li {
        display: flex;
        gap: 0.35rem;
        align-items: baseline;
        font-size: 0.78rem;
        color: var(--ink-muted);
        padding: 0.1rem 0;
      }
      .done i {
        color: var(--accent);
        font-size: 0.62rem;
      }
      .group {
        margin-top: 0.6rem;
      }
      .label {
        font-size: 0.66rem;
        text-transform: uppercase;
        letter-spacing: 0.06em;
        color: var(--ink-faint);
      }
      .group ul {
        list-style: none;
        margin: 0.2rem 0 0;
        padding: 0;
      }
      .group li {
        font-size: 0.8rem;
        color: var(--ink);
        padding: 0.08rem 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .from {
        font-size: 0.68rem;
        color: var(--ink-faint);
      }
      .assign {
        display: flex;
        gap: 0.35rem;
        margin-top: 0.75rem;
      }
      .assign input {
        flex: 1;
        min-width: 0;
        padding: 0.28rem 0.45rem;
        border: 1px solid var(--border);
        border-radius: 7px;
        background: var(--surface);
        color: var(--ink-strong);
        font: inherit;
        font-size: 0.78rem;
        outline: none;
      }
      .assign input:focus {
        border-color: var(--accent);
      }
      .assign button {
        border: 1px solid var(--border);
        border-radius: 7px;
        background: transparent;
        color: var(--ink-muted);
        font: inherit;
        font-size: 0.75rem;
        padding: 0.2rem 0.55rem;
        cursor: pointer;
      }
      .assign button:disabled {
        opacity: 0.45;
        cursor: default;
      }
      .unavailable {
        margin: 0.5rem 0 0;
        font-size: 0.78rem;
        color: var(--ink-faint);
      }
    `
  ]
})
export class TeamViewComponent {
  @Input() members: TeamMember[] = [];
  @Input() sync: TasksSyncState | null = null;
  @Input() syncing = false;
  @Input() syncMessage: string | null = null;

  @Output() readonly assigned = new EventEmitter<{ member: string; line: string }>();
  @Output() readonly syncRequested = new EventEmitter<void>();
  @Output() readonly syncChanged = new EventEmitter<{ enabled: boolean; intervalSeconds?: number }>();

  /** Same choices as Knowledge, for the same reason: one knob worth varying. */
  readonly intervals = [
    { seconds: 60, label: "1 minute" },
    { seconds: 180, label: "3 minutes" },
    { seconds: 300, label: "5 minutes" },
    { seconds: 900, label: "15 minutes" },
    { seconds: 1800, label: "30 minutes" }
  ];

  onToggle(event: Event): void {
    this.syncChanged.emit({ enabled: (event.target as HTMLInputElement).checked });
  }

  onInterval(event: Event): void {
    const seconds = Number((event.target as HTMLSelectElement).value);
    if (Number.isFinite(seconds)) {
      this.syncChanged.emit({ enabled: this.sync?.settings.enabled ?? true, intervalSeconds: seconds });
    }
  }

  /** One draft per colleague, so switching cards does not lose what was typed. */
  drafts: Record<string, string> = {};

  finishedToday(): number {
    return this.members.reduce((sum, member) => sum + member.completedToday, 0);
  }

  hand(member: string, event: Event): void {
    event.preventDefault();
    const line = (this.drafts[member] ?? "").trim();
    if (!line) {
      return;
    }
    this.drafts[member] = "";
    this.assigned.emit({ member, line });
  }
}

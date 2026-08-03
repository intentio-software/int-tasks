import { ChangeDetectionStrategy, Component, Input, computed, signal } from "@angular/core";
import { CommonModule } from "@angular/common";

import { Stats, TimeSummary } from "../models/task.models";

/**
 * The standing: what the streak is, how today is going, and where the time went.
 *
 * The title bar carries the same numbers as three glanceable chips. This is the
 * version you come to look at rather than catch out of the corner of your eye,
 * so it shows the working — the goal as a row of sessions, the time broken down
 * by task — rather than repeating the chips at a larger size.
 */
@Component({
  selector: "app-flow-stats",
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (stats; as s) {
      <div class="stats">
        <div class="cards">
          <div class="card" [class.lit]="s.streakDays > 0">
            <i class="pi pi-bolt"></i>
            <span class="value">{{ s.streakDays }}</span>
            <span class="label">day{{ s.streakDays === 1 ? "" : "s" }} running</span>
          </div>
          <div class="card">
            <i class="pi pi-clock"></i>
            <span class="value">{{ focusLabel(s.focusMinutesToday) }}</span>
            <span class="label">focused today</span>
          </div>
          <div class="card" [class.lit]="s.pointsToday > 0">
            <i class="pi pi-star-fill"></i>
            <span class="value">+{{ s.pointsToday }}</span>
            <span class="label">impact today</span>
          </div>
          <div class="card">
            <i class="pi pi-check-circle"></i>
            <span class="value">{{ s.completedToday }}</span>
            <span class="label">finished today</span>
          </div>
        </div>

        <!-- The goal as a row of sessions rather than a number: four of five
             filled says more at a glance than "4/5" does. -->
        <div class="goal" [class.met]="s.goalMet">
          <div class="goal-head">
            <span>Today's focus goal</span>
            <span class="goal-count">{{ s.sessionsToday }} / {{ s.dailyGoal }}</span>
          </div>
          <div class="pips">
            @for (pip of pips(s); track $index) {
              <span class="pip" [class.done]="pip"></span>
            }
          </div>
          @if (s.goalMet) {
            <span class="met-note">Goal met — anything more is a bonus.</span>
          }
        </div>

        <div class="lifetime">
          <span>{{ s.pointsTotal }} impact banked all time</span>
        </div>
      </div>
    }

    @if (breakdown().length) {
      <div class="breakdown">
        <div class="breakdown-head">
          <h3>Where the time went</h3>
          <span class="scope">all time</span>
        </div>
        @for (row of breakdown(); track row.id) {
          <div class="row">
            <span class="name" [class.unattributed]="!row.id">{{ row.title }}</span>
            <span class="bar"><span class="fill" [style.width.%]="row.share"></span></span>
            <span class="time">{{ row.label }}</span>
          </div>
        }
      </div>
    }
  `,
  styles: [
    `
      :host {
        display: block;
        max-width: 34rem;
        margin: 0 auto;
        padding: 0 1rem;
      }
      .cards {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(7rem, 1fr));
        gap: 0.5rem;
      }
      .card {
        display: flex;
        flex-direction: column;
        gap: 0.1rem;
        padding: 0.6rem 0.7rem;
        border: 1px solid var(--border);
        border-radius: 10px;
        background: var(--panel);
      }
      .card i {
        font-size: 0.75rem;
        color: var(--ink-faint);
      }
      .card.lit i {
        color: var(--accent);
      }
      .value {
        font-size: 1.15rem;
        font-weight: 600;
        color: var(--ink-strong);
        font-variant-numeric: tabular-nums;
      }
      .label {
        font-size: 0.7rem;
        color: var(--ink-faint);
      }

      .goal {
        margin-top: 0.8rem;
        padding: 0.7rem 0.8rem;
        border: 1px solid var(--border);
        border-radius: 10px;
        background: var(--panel);
      }
      .goal.met {
        border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
      }
      .goal-head {
        display: flex;
        align-items: baseline;
        font-size: 0.78rem;
        color: var(--ink-muted);
      }
      .goal-head span:first-child {
        flex: 1;
      }
      .goal-count {
        font-variant-numeric: tabular-nums;
        color: var(--ink-faint);
      }
      .pips {
        display: flex;
        gap: 0.25rem;
        margin-top: 0.45rem;
      }
      .pip {
        flex: 1;
        height: 6px;
        border-radius: 3px;
        background: var(--hover);
      }
      .pip.done {
        background: var(--accent);
      }
      .met-note {
        display: block;
        margin-top: 0.4rem;
        font-size: 0.7rem;
        color: var(--accent);
      }
      .lifetime {
        margin-top: 0.5rem;
        font-size: 0.72rem;
        color: var(--ink-faint);
        text-align: right;
      }

      .breakdown {
        margin-top: 1.4rem;
      }
      .breakdown-head {
        display: flex;
        align-items: baseline;
        gap: 0.5rem;
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
      .scope {
        font-size: 0.72rem;
        color: var(--ink-faint);
      }
      .row {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        padding: 0.32rem 0.2rem;
        font-size: 0.82rem;
      }
      .name {
        width: 40%;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--ink);
      }
      .name.unattributed {
        color: var(--ink-faint);
        font-style: italic;
      }
      .bar {
        flex: 1;
        height: 6px;
        border-radius: 3px;
        background: var(--hover);
        overflow: hidden;
      }
      .fill {
        display: block;
        height: 100%;
        background: var(--accent);
        border-radius: 3px;
      }
      .time {
        width: 3.4rem;
        text-align: right;
        color: var(--ink-faint);
        font-variant-numeric: tabular-nums;
      }
    `
  ]
})
export class FlowStatsComponent {
  @Input() stats: Stats | null = null;
  @Input() set summary(value: TimeSummary | null) {
    this.currentSummary.set(value);
  }

  private readonly currentSummary = signal<TimeSummary | null>(null);

  focusLabel(minutes: number): string {
    if (minutes < 60) {
      return `${minutes}m`;
    }
    const hours = Math.floor(minutes / 60);
    return `${hours}h ${minutes % 60}m`;
  }

  /** One pip per session towards the goal, plus any overshoot. */
  pips(stats: Stats): boolean[] {
    const total = Math.max(stats.dailyGoal, stats.sessionsToday);
    return Array.from({ length: total }, (_, index) => index < stats.sessionsToday);
  }

  readonly breakdown = computed(() => {
    const summary = this.currentSummary();
    if (!summary) {
      return [];
    }
    const rows = summary.by_task.map((entry) => ({
      id: entry.task_id,
      title: entry.title ?? "Deleted task",
      seconds: entry.seconds
    }));
    if (summary.unattributed_seconds > 0) {
      rows.push({ id: "", title: "Unattributed", seconds: summary.unattributed_seconds });
    }
    rows.sort((a, b) => b.seconds - a.seconds);

    // Shares are relative to the largest row, not the total: the point is to
    // compare tasks with each other, and a total-relative bar is unreadable
    // once there are more than a handful.
    const largest = rows[0]?.seconds ?? 0;
    return rows.slice(0, 8).map((row) => ({
      ...row,
      share: largest ? (row.seconds / largest) * 100 : 0,
      label: this.focusLabel(Math.round(row.seconds / 60))
    }));
  });
}

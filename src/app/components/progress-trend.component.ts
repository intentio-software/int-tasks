import { ChangeDetectionStrategy, Component, Input, computed, signal } from "@angular/core";
import { CommonModule } from "@angular/common";

import { DayProgress } from "../models/task.models";

/**
 * Focus time and impact over the last ten working days.
 *
 * Two separate rows of bars rather than one chart with two axes. A dual axis
 * would let two unrelated units share a shape and invite the reader to see a
 * relationship that was never measured; stacked rows make the same comparison
 * honestly, and at this size read just as quickly.
 */
@Component({
  selector: "app-progress-trend",
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (days().length > 1) {
      <div class="trend">
        <div class="trend-head">
          <h3>Progress</h3>
          <span class="scope">last {{ days().length }} working days</span>
        </div>

        <div class="series">
          <div class="series-head">
            <span class="name">Focus</span>
            <span class="delta" [ngClass]="focusTrend().tone">{{ focusTrend().label }}</span>
          </div>
          <div class="bars">
            @for (day of days(); track day.date) {
              <span
                class="col"
                [title]="day.date + ' — ' + day.focusMinutes + ' minutes'"
              >
                <span class="bar focus" [style.height.%]="height(day.focusMinutes, peakFocus())"></span>
              </span>
            }
          </div>
          <span class="peak">peak {{ peakFocus() }}m</span>
        </div>

        <div class="series">
          <div class="series-head">
            <span class="name">Impact</span>
            <span class="delta" [ngClass]="impactTrend().tone">{{ impactTrend().label }}</span>
          </div>
          <div class="bars">
            @for (day of days(); track day.date) {
              <span class="col" [title]="day.date + ' — ' + day.points + ' impact'">
                <span class="bar impact" [style.height.%]="height(day.points, peakImpact())"></span>
              </span>
            }
          </div>
          <span class="peak">peak {{ peakImpact() }}</span>
        </div>
      </div>
    }
  `,
  styles: [
    `
      :host {
        display: block;
        max-width: 34rem;
        margin: 1.4rem auto 0;
        padding: 0 1rem;
      }
      .trend-head {
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
      .series {
        position: relative;
        margin-top: 0.7rem;
      }
      .series-head {
        display: flex;
        align-items: baseline;
        gap: 0.5rem;
        margin-bottom: 0.25rem;
      }
      .name {
        flex: 1;
        font-size: 0.75rem;
        color: var(--ink-muted);
      }
      .delta {
        font-size: 0.72rem;
        color: var(--ink-faint);
      }
      .delta.up {
        color: var(--accent);
      }
      .delta.down {
        color: var(--ink-faint);
      }
      .bars {
        display: flex;
        align-items: flex-end;
        gap: 3px;
        height: 40px;
        padding-bottom: 2px;
        border-bottom: 1px solid var(--border);
      }
      .col {
        flex: 1;
        display: flex;
        align-items: flex-end;
        height: 100%;
      }
      .bar {
        width: 100%;
        min-height: 2px;
        border-radius: 2px 2px 0 0;
        background: var(--hover);
      }
      /* A day with nothing on it keeps its column, so the gap is visible. */
      .bar.focus {
        background: var(--accent);
      }
      .bar.impact {
        background: color-mix(in srgb, var(--accent) 55%, var(--ink-faint));
      }
      .peak {
        display: block;
        margin-top: 0.2rem;
        font-size: 0.68rem;
        color: var(--ink-faint);
        text-align: right;
      }
    `
  ]
})
export class ProgressTrendComponent {
  @Input() set progress(value: DayProgress[]) {
    this.days.set(value ?? []);
  }

  readonly days = signal<DayProgress[]>([]);

  readonly peakFocus = computed(() =>
    Math.max(1, ...this.days().map((day) => day.focusMinutes))
  );
  readonly peakImpact = computed(() => Math.max(1, ...this.days().map((day) => day.points)));

  height(value: number, peak: number): number {
    return peak > 0 ? Math.round((value / peak) * 100) : 0;
  }

  readonly focusTrend = computed(() =>
    this.compare(this.days().map((day) => day.focusMinutes))
  );
  readonly impactTrend = computed(() => this.compare(this.days().map((day) => day.points)));

  /**
   * The recent half against the half before it.
   *
   * Halves rather than first-against-last: one heroic Tuesday should not read
   * as a trend, and one quiet day should not undo a good fortnight.
   */
  private compare(values: number[]): { label: string; tone: string } {
    if (values.length < 4) {
      return { label: "", tone: "" };
    }
    const half = Math.floor(values.length / 2);
    const mean = (xs: number[]) => xs.reduce((sum, x) => sum + x, 0) / (xs.length || 1);
    const before = mean(values.slice(0, half));
    const after = mean(values.slice(values.length - half));

    if (before === 0 && after === 0) {
      return { label: "", tone: "" };
    }
    if (before === 0) {
      return { label: "up on nothing", tone: "up" };
    }
    const change = Math.round(((after - before) / before) * 100);
    if (Math.abs(change) < 5) {
      return { label: "holding steady", tone: "" };
    }
    return {
      label: `${change > 0 ? "↑" : "↓"} ${Math.abs(change)}% on the half before`,
      tone: change > 0 ? "up" : "down"
    };
  }
}

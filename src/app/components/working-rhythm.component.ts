import { ChangeDetectionStrategy, Component, EventEmitter, Input, Output, signal } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";

import { Settings } from "../models/task.models";

/**
 * Which days count as working days, and how long finished work stays visible.
 *
 * It lives on Flow rather than in a settings dialog because it is the thing
 * that explains the numbers directly above it — a streak that survived the
 * weekend makes sense once you can see that Saturday is not a working day.
 */
@Component({
  selector: "app-working-rhythm",
  standalone: true,
  imports: [CommonModule, FormsModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="rhythm">
      <div class="rhythm-head">
        <h3>Working rhythm</h3>
        <button type="button" class="toggle" (click)="open.set(!open())">
          {{ open() ? "Done" : "Adjust" }}
        </button>
      </div>

      <p class="summary">
        {{ summary() }}
      </p>

      @if (open()) {
        <div class="fields">
          <div class="field">
            <span class="field-label">Working days</span>
            <div class="days">
              @for (day of dayNames; track day.value) {
                <button
                  type="button"
                  class="day"
                  [class.on]="isWorking(day.value)"
                  (click)="toggleDay(day.value)"
                >
                  {{ day.short }}
                </button>
              }
            </div>
          </div>

          <div class="field">
            <span class="field-label">Hide finished work after</span>
            <select
              [ngModel]="settings?.hideCompletedAfterDays ?? 2"
              (ngModelChange)="hideAfterChanged.emit(+$event)"
            >
              @for (n of [1, 2, 3, 5, 10]; track n) {
                <option [value]="n">{{ n }} working day{{ n === 1 ? "" : "s" }}</option>
              }
            </select>
          </div>

          <div class="field wide">
            <span class="field-label">Public holidays</span>
            <input
              type="text"
              placeholder="2026-12-16, 2026-12-25"
              [(ngModel)]="holidayText"
              (blur)="commitHolidays()"
            />
            <span class="hint">
              Dates you do not work, comma separated. Kept as your own list — holidays differ by
              country and employer, and a wrong one is worse than none.
            </span>
          </div>
        </div>
      }
    </div>
  `,
  styles: [
    `
      :host {
        display: block;
        max-width: 34rem;
        margin: 1.4rem auto 0;
        padding: 0 1rem;
      }
      .rhythm-head {
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
      .toggle {
        border: none;
        background: transparent;
        color: var(--ink-faint);
        font-size: 0.75rem;
        cursor: pointer;
      }
      .toggle:hover {
        color: var(--accent);
      }
      .summary {
        margin: 0.5rem 0 0;
        font-size: 0.78rem;
        line-height: 1.5;
        color: var(--ink-faint);
      }
      .fields {
        display: flex;
        flex-wrap: wrap;
        gap: 0.9rem;
        margin-top: 0.8rem;
      }
      .field {
        display: flex;
        flex-direction: column;
        gap: 0.3rem;
      }
      .field.wide {
        width: 100%;
      }
      .field-label {
        font-size: 0.72rem;
        color: var(--ink-muted);
      }
      .days {
        display: flex;
        gap: 0.2rem;
      }
      .day {
        width: 1.9rem;
        padding: 0.2rem 0;
        border: 1px solid var(--border);
        border-radius: 6px;
        background: transparent;
        color: var(--ink-faint);
        font: inherit;
        font-size: 0.72rem;
        cursor: pointer;
      }
      .day.on {
        background: var(--accent);
        border-color: var(--accent);
        color: #fff;
      }
      select,
      input {
        padding: 0.25rem 0.4rem;
        border: 1px solid var(--border);
        border-radius: 6px;
        background: var(--surface);
        color: var(--ink-strong);
        font: inherit;
        font-size: 0.8rem;
        outline: none;
      }
      input:focus,
      select:focus {
        border-color: var(--accent);
      }
      .hint {
        font-size: 0.7rem;
        line-height: 1.5;
        color: var(--ink-faint);
      }
    `
  ]
})
export class WorkingRhythmComponent {
  @Input() set settings(value: Settings | null) {
    this.current = value;
    this.holidayText = (value?.holidays ?? []).join(", ");
  }
  get settings(): Settings | null {
    return this.current;
  }

  @Output() readonly workingDaysChanged = new EventEmitter<number[]>();
  @Output() readonly holidaysChanged = new EventEmitter<string[]>();
  @Output() readonly hideAfterChanged = new EventEmitter<number>();

  readonly open = signal(false);
  holidayText = "";
  private current: Settings | null = null;

  /** Sunday first, matching the 0-6 the store uses. */
  readonly dayNames = [
    { value: 0, short: "Sun" },
    { value: 1, short: "Mon" },
    { value: 2, short: "Tue" },
    { value: 3, short: "Wed" },
    { value: 4, short: "Thu" },
    { value: 5, short: "Fri" },
    { value: 6, short: "Sat" }
  ];

  isWorking(day: number): boolean {
    return (this.current?.workingDays ?? [1, 2, 3, 4, 5]).includes(day);
  }

  toggleDay(day: number): void {
    const days = [...(this.current?.workingDays ?? [1, 2, 3, 4, 5])];
    const at = days.indexOf(day);
    if (at >= 0) {
      days.splice(at, 1);
    } else {
      days.push(day);
    }
    this.workingDaysChanged.emit(days.sort());
  }

  commitHolidays(): void {
    const dates = this.holidayText
      .split(",")
      .map((date) => date.trim())
      .filter((date) => /^\d{4}-\d{2}-\d{2}$/.test(date));
    this.holidaysChanged.emit(dates);
  }

  summary(): string {
    const days = this.current?.workingDays ?? [1, 2, 3, 4, 5];
    const names = this.dayNames.filter((day) => days.includes(day.value)).map((day) => day.short);
    const holidays = this.current?.holidays?.length ?? 0;
    const worked = names.length ? names.join(", ") : "no days set";
    const holidayNote = holidays
      ? ` ${holidays} public holiday${holidays === 1 ? "" : "s"} also count as time off.`
      : "";
    return `Streaks and the hiding of finished work count ${worked}.${holidayNote}`;
  }
}

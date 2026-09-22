import { ChangeDetectionStrategy, Component, OnDestroy, OnInit, inject, signal } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";

import { TasksService } from "../services/tasks.service";
import { LightMode, LightStatus } from "../models/task.models";

/**
 * The desk light, for people who have one.
 *
 * Shown to everybody rather than hidden behind a flag, because a feature only
 * its author knows about may as well not exist — but off until switched on,
 * since most people have no lamp and an app that goes looking through serial
 * ports uninvited is rude.
 */
@Component({
  selector: "app-desk-light",
  standalone: true,
  imports: [CommonModule, FormsModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="light">
      <div class="light-head">
        <h3>Desk light</h3>
        <span class="dot">{{ dot() }}</span>
        <button type="button" class="toggle" (click)="open.set(!open())">
          {{ open() ? "Done" : status()?.connected ? "Adjust" : "Set up" }}
        </button>
      </div>

      <p class="summary">{{ summary() }}</p>

      @if (open()) {
        <div class="fields">
          <label class="check">
            <input
              type="checkbox"
              [checked]="enabled()"
              (change)="setEnabled($any($event.target).checked)"
            />
            <span>Use a desk light</span>
          </label>

          @if (enabled()) {
            <label class="check">
              <input
                type="checkbox"
                [checked]="status()?.followTimer ?? true"
                (change)="setFollow($any($event.target).checked)"
              />
              <span>Follow the timer — red while focusing or in a meeting</span>
            </label>

            <div class="field">
              <span class="field-label">Port</span>
              <select [ngModel]="status()?.port ?? ''" (ngModelChange)="setPort($event)">
                <option value="">Find it automatically</option>
                @for (port of status()?.ports ?? []; track port) {
                  <option [value]="port">{{ port }}</option>
                }
              </select>
            </div>

            <div class="field">
              <span class="field-label">Set the colour by hand</span>
              <div class="colours">
                @for (choice of colours; track choice.mode) {
                  <button
                    type="button"
                    class="colour"
                    [class.on]="status()?.mode === choice.mode"
                    [disabled]="!status()?.connected"
                    [title]="choice.label"
                    (click)="setMode(choice.mode)"
                  >
                    {{ choice.dot }}
                  </button>
                }
                <button type="button" class="link" (click)="find()">Look for the device</button>
              </div>
              @if (status()?.followTimer && status()?.connected) {
                <span class="hint">
                  Setting a colour by hand turns off following, so the timer will not change it
                  back a moment later.
                </span>
              }
            </div>
          } @else {
            <p class="hint">
              A lamp on your desk that turns red while you are focusing or in a meeting, so people
              can see before they interrupt. It needs a Raspberry Pi Pico running the Intentio
              busylight firmware, plugged in over USB. Nothing is scanned or opened until you
              switch this on.
            </p>
          }
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
      /* Declared here rather than inherited: view encapsulation keeps the
         parent's .section-head rules out of this component. */
      .light-head {
        display: flex;
        align-items: baseline;
        gap: 0.5rem;
        padding-bottom: 0.4rem;
        border-bottom: 1px solid var(--border);
      }
      h3 {
        margin: 0;
        font-size: 0.85rem;
        font-weight: 600;
        color: var(--ink-muted);
        text-transform: uppercase;
        letter-spacing: 0.06em;
      }
      .dot {
        flex: 1;
        font-size: 0.8rem;
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
        flex-direction: column;
        gap: 0.7rem;
        margin-top: 0.8rem;
      }
      .check {
        display: flex;
        align-items: center;
        gap: 0.45rem;
        font-size: 0.8rem;
        color: var(--ink);
        cursor: pointer;
      }
      .field {
        display: flex;
        flex-direction: column;
        gap: 0.3rem;
      }
      .field-label {
        font-size: 0.72rem;
        color: var(--ink-muted);
      }
      select {
        padding: 0.25rem 0.4rem;
        border: 1px solid var(--border);
        border-radius: 6px;
        background: var(--surface);
        color: var(--ink-strong);
        font: inherit;
        font-size: 0.8rem;
        outline: none;
      }
      select:focus {
        border-color: var(--accent);
      }
      .colours {
        display: flex;
        align-items: center;
        gap: 0.3rem;
      }
      .colour {
        padding: 0.2rem 0.35rem;
        border: 1px solid transparent;
        border-radius: 6px;
        background: transparent;
        font-size: 0.95rem;
        line-height: 1;
        cursor: pointer;
      }
      .colour.on {
        border-color: var(--accent);
        background: var(--hover);
      }
      .colour:disabled {
        opacity: 0.35;
        cursor: default;
      }
      .link {
        margin-left: auto;
        border: none;
        background: transparent;
        color: var(--accent);
        font: inherit;
        font-size: 0.75rem;
        cursor: pointer;
      }
      .link:hover {
        text-decoration: underline;
      }
      .hint {
        margin: 0;
        font-size: 0.72rem;
        line-height: 1.55;
        color: var(--ink-faint);
      }
    `
  ]
})
export class DeskLightComponent implements OnInit, OnDestroy {
  private readonly tasks = inject(TasksService);

  readonly open = signal(false);
  readonly status = signal<LightStatus | null>(null);
  private unlisten: (() => void) | null = null;

  readonly colours: { mode: LightMode; dot: string; label: string }[] = [
    { mode: "busy", dot: "🔴", label: "Busy" },
    { mode: "available", dot: "🟢", label: "Available" },
    { mode: "ringing", dot: "🟠", label: "Ringing" },
    { mode: "offline", dot: "⚪", label: "Offline" },
    { mode: "off", dot: "⚫", label: "Off" }
  ];

  async ngOnInit(): Promise<void> {
    this.status.set(await this.tasks.lightStatus());
    // The supervisor connects on its own, so the panel is told rather than
    // asking on a timer.
    this.unlisten = await this.tasks.onLightChange((status) => this.status.set(status));
  }

  ngOnDestroy(): void {
    this.unlisten?.();
  }

  enabled(): boolean {
    return this.status()?.enabled ?? false;
  }

  dot(): string {
    const status = this.status();
    if (!status?.enabled) {
      return "";
    }
    if (!status.connected) {
      return "◌";
    }
    return this.colours.find((c) => c.mode === status.mode)?.dot ?? "🔵";
  }

  summary(): string {
    const status = this.status();
    if (!status?.enabled) {
      return "Off. A lamp that shows other people when you are busy.";
    }
    if (!status.connected) {
      return `${status.message}. It keeps looking, so plugging the device in is enough.`;
    }
    return status.followTimer
      ? `${status.message}, following the timer.`
      : `${status.message}, set by hand.`;
  }

  async setEnabled(enabled: boolean): Promise<void> {
    const status = this.status();
    this.status.set(
      await this.tasks.setLightSettings({
        enabled,
        port: status?.port ?? null,
        followTimer: status?.followTimer ?? true
      })
    );
  }

  async setFollow(followTimer: boolean): Promise<void> {
    const status = this.status();
    this.status.set(
      await this.tasks.setLightSettings({
        enabled: true,
        port: status?.port ?? null,
        followTimer
      })
    );
  }

  async setPort(port: string): Promise<void> {
    const status = this.status();
    this.status.set(
      await this.tasks.setLightSettings({
        enabled: true,
        port: port || null,
        followTimer: status?.followTimer ?? true
      })
    );
  }

  async setMode(mode: LightMode): Promise<void> {
    this.status.set(await this.tasks.setLightMode(mode));
  }

  async find(): Promise<void> {
    this.status.set(await this.tasks.findLight());
  }
}

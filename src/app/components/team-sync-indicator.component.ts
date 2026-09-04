import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  EventEmitter,
  HostListener,
  Input,
  Output,
  inject,
  signal
} from "@angular/core";
import { CommonModule } from "@angular/common";

import { TasksSyncState } from "./team-view.component";

/**
 * Where the team folder stands with its remote, and the switch that keeps it there.
 *
 * Deliberately the same shape as the indicator in Intentio Knowledge: same
 * place, same wording, same intervals. Two apps that sync the same way should
 * not need to be learned twice.
 *
 * Shown only when there is a team. On a store with no siblings there is nothing
 * to sync with anyone, and an inert control in the corner is worse than none.
 */
@Component({
  selector: "app-team-sync-indicator",
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (sync?.status?.isRepo) {
      <span class="sync" [class.blocked]="!!blockedReason()">
        <button type="button" class="face" (click)="open.set(!open())" [title]="tooltip()">
          <i class="pi" [ngClass]="icon()"></i>
          <span class="label">{{ label() }}</span>
        </button>

        @if (open()) {
          <div class="panel">
            <label class="row">
              <input type="checkbox" [checked]="sync!.settings.enabled" (change)="toggle($event)" />
              <span>Keep the team folder in sync</span>
            </label>

            <p class="detail">
              {{ sync!.status.branch }}
              <span *ngIf="sync!.status.ahead"> · {{ sync!.status.ahead }} to push</span>
              <span *ngIf="sync!.status.behind"> · {{ sync!.status.behind }} to pull</span>
              <span *ngIf="!sync!.status.hasRemote"> — no remote</span>
            </p>

            <label class="row interval" [class.dim]="!sync!.settings.enabled">
              <span>Fetch every</span>
              <select [disabled]="!sync!.settings.enabled" (change)="chooseInterval($event)">
                @for (choice of intervals; track choice.seconds) {
                  <option
                    [value]="choice.seconds"
                    [selected]="choice.seconds === sync!.settings.intervalSeconds"
                  >
                    {{ choice.label }}
                  </option>
                }
              </select>
            </label>

            @if (blockedReason(); as reason) {
              <p class="reason">{{ reason }}</p>
            }

            <p class="behaviour">
              Your own work is committed once you have stopped for a couple of
              minutes, so a session becomes one commit rather than a run of them.
            </p>

            <div class="notify">
              <span class="notify-state" [ngClass]="notifyState()">
                {{ notifyLabel() }}
              </span>
              <button
                type="button"
                class="link"
                *ngIf="notifyState() !== 'blocked'"
                (click)="testNotification()"
              >
                {{ notifyState() === "on" ? "Send a test" : "Turn on" }}
              </button>
            </div>

            <button type="button" class="now" [disabled]="syncing" (click)="syncRequested.emit()">
              {{ syncing ? "Syncing…" : "Sync now" }}
            </button>
          </div>
        }
      </span>
    }
  `,
  styles: [
    `
      .sync {
        position: relative;
        display: inline-flex;
      }
      .face {
        display: inline-flex;
        align-items: center;
        gap: 0.3rem;
        border: none;
        background: transparent;
        color: var(--ink-faint);
        font: inherit;
        font-size: 0.72rem;
        cursor: pointer;
        padding: 0;
      }
      .face:hover,
      .sync.blocked .face {
        color: var(--accent);
      }
      .panel {
        position: absolute;
        bottom: 1.5rem;
        right: 0;
        z-index: 60;
        width: 17rem;
        padding: 0.7rem 0.8rem;
        border: 1px solid var(--border);
        border-radius: 10px;
        background: var(--panel-raised);
        box-shadow: 0 18px 40px rgba(0, 0, 0, 0.45);
        text-align: left;
      }
      .row {
        display: flex;
        align-items: center;
        gap: 0.45rem;
        font-size: 0.82rem;
      }
      .interval {
        margin-top: 0.6rem;
        justify-content: space-between;
      }
      .interval.dim {
        opacity: 0.5;
      }
      .interval select {
        padding: 0.15rem 0.3rem;
        border: 1px solid var(--border);
        border-radius: 6px;
        background: var(--surface);
        color: var(--ink-strong);
        font: inherit;
        font-size: 0.75rem;
      }
      .detail {
        margin: 0.45rem 0 0;
        font-size: 0.72rem;
        color: var(--ink-faint);
      }
      .reason {
        margin: 0.45rem 0 0;
        font-size: 0.72rem;
        line-height: 1.5;
        color: var(--accent);
      }
      .behaviour {
        margin: 0.5rem 0 0;
        font-size: 0.7rem;
        line-height: 1.5;
        color: var(--ink-faint);
      }
      .notify {
        display: flex;
        align-items: baseline;
        gap: 0.5rem;
        margin-top: 0.6rem;
        padding-top: 0.6rem;
        border-top: 1px solid var(--border);
        font-size: 0.75rem;
      }
      .notify-state {
        flex: 1;
        color: var(--ink-faint);
      }
      .notify-state.on {
        color: var(--accent);
      }
      .notify-state.blocked {
        color: var(--ink-muted);
      }
      .link {
        border: none;
        background: transparent;
        color: var(--accent);
        font: inherit;
        font-size: 0.75rem;
        cursor: pointer;
        padding: 0;
      }
      .link:hover {
        text-decoration: underline;
      }
      .now {
        margin-top: 0.6rem;
        padding: 0.25rem 0.6rem;
        border: 1px solid var(--border);
        border-radius: 7px;
        background: transparent;
        color: inherit;
        font: inherit;
        font-size: 0.75rem;
        cursor: pointer;
      }
      .now:disabled {
        opacity: 0.5;
        cursor: default;
      }
    `
  ]
})
export class TeamSyncIndicatorComponent {
  @Input() sync: TasksSyncState | null = null;
  @Input() syncing = false;
  /** The last thing that stopped a sync, cleared by the next good one. */
  @Input() blocked: string | null = null;

  @Output() readonly syncRequested = new EventEmitter<void>();
  @Output() readonly syncChanged = new EventEmitter<{ enabled: boolean; intervalSeconds?: number }>();

  readonly open = signal(false);
  /** unknown until asked, then on, off or blocked by the system. */
  readonly notify = signal<"unknown" | "on" | "off" | "blocked">("unknown");
  private readonly host = inject(ElementRef<HTMLElement>);

  readonly intervals = [
    { seconds: 60, label: "1 minute" },
    { seconds: 180, label: "3 minutes" },
    { seconds: 300, label: "5 minutes" },
    { seconds: 900, label: "15 minutes" },
    { seconds: 1800, label: "30 minutes" }
  ];

  /** A panel that only closes by pressing what opened it is a trap. */
  @HostListener("document:pointerdown", ["$event"])
  onPointerDown(event: PointerEvent): void {
    if (!this.open()) {
      return;
    }
    const target = event.target as Node | null;
    if (target && !this.host.nativeElement.contains(target)) {
      this.open.set(false);
    }
  }

  @HostListener("document:keydown.escape")
  onEscape(): void {
    this.open.set(false);
  }

  togglePanel(): void {
    const opening = !this.open();
    this.open.set(opening);
    if (opening) {
      void this.refreshNotifyState();
    }
  }

  notifyState(): string {
    return this.notify();
  }

  notifyLabel(): string {
    switch (this.notify()) {
      case "on":
        return "Notifications on";
      case "blocked":
        return "Notifications are off in System Settings";
      case "off":
        return "Notifications off";
      default:
        return "Notifications not set up";
    }
  }

  /**
   * Ask for permission if we do not have it, then prove it works.
   *
   * Asking here rather than the first time a colleague finishes something: a
   * permission prompt makes sense when you pressed a button that asks for one,
   * and makes no sense arriving out of nowhere on a Tuesday. And a test that
   * actually appears is the only way to know the setting took.
   */
  async testNotification(): Promise<void> {
    try {
      const { isPermissionGranted, requestPermission, sendNotification } = await import(
        "@tauri-apps/plugin-notification"
      );
      let granted = await isPermissionGranted();
      if (!granted) {
        const answer = await requestPermission();
        granted = answer === "granted";
        if (!granted) {
          // Denied is not the same as never asked: macOS will not ask again,
          // so say where to change it rather than offering the button forever.
          this.notify.set(answer === "denied" ? "blocked" : "off");
          return;
        }
      }
      this.notify.set("on");
      sendNotification({
        title: "Intentio Tasks",
        body: "Notifications are working. You will hear when a colleague finishes something."
      });
    } catch {
      this.notify.set("off");
    }
  }

  /** Read the current state without asking for anything. */
  async refreshNotifyState(): Promise<void> {
    try {
      const { isPermissionGranted } = await import("@tauri-apps/plugin-notification");
      this.notify.set((await isPermissionGranted()) ? "on" : "unknown");
    } catch {
      this.notify.set("off");
    }
  }

  blockedReason(): string | null {
    return this.blocked ?? this.sync?.status?.blocked ?? null;
  }

  icon(): string {
    if (this.blockedReason()) return "pi-exclamation-triangle";
    if (!this.sync?.settings.enabled) return "pi-cloud";
    return this.syncing ? "pi-spin pi-spinner" : "pi-sync";
  }

  label(): string {
    if (this.blockedReason()) return "Sync paused";
    if (!this.sync?.settings.enabled) return "Sync off";
    const status = this.sync.status;
    return status.ahead || status.behind ? "Syncing" : "In sync";
  }

  tooltip(): string {
    return this.blockedReason() ?? "Git sync for the team folder";
  }

  toggle(event: Event): void {
    this.syncChanged.emit({ enabled: (event.target as HTMLInputElement).checked });
  }

  chooseInterval(event: Event): void {
    const seconds = Number((event.target as HTMLSelectElement).value);
    if (Number.isFinite(seconds)) {
      this.syncChanged.emit({ enabled: this.sync?.settings.enabled ?? true, intervalSeconds: seconds });
    }
  }
}

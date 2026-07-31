import { ChangeDetectionStrategy, Component, EventEmitter, Input, Output } from "@angular/core";
import { CommonModule } from "@angular/common";

import { Plotted } from "../models/task.models";

/**
 * The impact/effort matrix.
 *
 * Effort runs left to right, impact bottom to top, so the top-left corner is
 * the best place to be — high value for little cost. Urgency is the third
 * dimension and is drawn as a ring around the dot rather than a third axis,
 * because a 3D scatter is unreadable and urgency changes on its own anyway.
 */
@Component({
  selector: "app-matrix-view",
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="wrap">
      <svg viewBox="0 0 400 400" class="plot" role="img" aria-label="Impact and effort matrix">
        <!-- Quadrants. Quick wins are tinted because that is where you want to
             be looking first. -->
        <rect x="0" y="0" width="200" height="200" class="q quick" />
        <rect x="200" y="0" width="200" height="200" class="q" />
        <rect x="0" y="200" width="200" height="200" class="q" />
        <rect x="200" y="200" width="200" height="200" class="q thankless" />

        <line x1="200" y1="0" x2="200" y2="400" class="axis" />
        <line x1="0" y1="200" x2="400" y2="200" class="axis" />

        <text x="8" y="18" class="label">Quick wins</text>
        <text x="392" y="18" class="label end">Big bets</text>
        <text x="8" y="392" class="label">Fill-ins</text>
        <text x="392" y="392" class="label end">Thankless</text>

        @for (entry of tasks; track entry.id) {
          <g
            class="dot"
            [class.selected]="entry.id === selectedId"
            [attr.transform]="'translate(' + x(entry.effort) + ',' + y(entry.impact) + ')'"
            (click)="picked.emit(entry)"
          >
            <!-- Urgency as a halo: the more pressing, the wider the ring. -->
            @if (entry.urgency > 0) {
              <circle [attr.r]="9 + entry.urgency" class="urgency" />
            }
            <circle r="9" class="core" />
            <title>{{ entry.title }} — impact {{ entry.impact }}, effort {{ entry.effort }}, urgency {{ entry.urgency }}</title>
          </g>
        }
      </svg>

      <div class="axis-labels">
        <span class="x">Effort →</span>
        <span class="y">Impact →</span>
      </div>

      @if (!tasks.length) {
        <div class="empty">
          <p>Nothing scored yet.</p>
          <span>Open a task and set its impact and effort to place it here.</span>
        </div>
      }
    </div>
  `,
  styles: [
    `
      .wrap {
        position: relative;
        max-width: 34rem;
        margin: 0 auto;
        padding: 0 1rem;
      }
      .plot {
        width: 100%;
        height: auto;
        overflow: visible;
      }
      .q {
        fill: var(--panel);
        stroke: var(--border);
        stroke-width: 1;
      }
      .q.quick {
        fill: color-mix(in srgb, var(--accent) 10%, var(--panel));
      }
      .q.thankless {
        fill: color-mix(in srgb, var(--ink-faint) 8%, var(--panel));
      }
      .axis {
        stroke: var(--border);
        stroke-width: 1.5;
      }
      .label {
        fill: var(--ink-faint);
        font-size: 12px;
        font-family: var(--font-body);
      }
      .label.end {
        text-anchor: end;
      }
      .dot {
        cursor: pointer;
      }
      .core {
        fill: var(--accent);
        stroke: var(--surface);
        stroke-width: 2;
      }
      .urgency {
        fill: none;
        stroke: var(--accent);
        stroke-opacity: 0.35;
        stroke-width: 2;
      }
      .dot:hover .core,
      .dot.selected .core {
        fill: var(--ink-strong);
      }
      .dot.selected .urgency {
        stroke-opacity: 0.8;
      }
      .axis-labels {
        display: flex;
        justify-content: space-between;
        margin-top: 0.3rem;
        font-size: 0.7rem;
        color: var(--ink-faint);
      }
      .axis-labels .y {
        writing-mode: vertical-rl;
        position: absolute;
        left: -0.2rem;
        top: 40%;
      }
      .empty {
        position: absolute;
        inset: 0;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 0.3rem;
        text-align: center;
        color: var(--ink-faint);
        background: color-mix(in srgb, var(--surface) 82%, transparent);
      }
      .empty p {
        margin: 0;
        color: var(--ink-muted);
      }
      .empty span {
        font-size: 0.78rem;
        max-width: 18rem;
      }
    `
  ]
})
export class MatrixViewComponent {
  @Input() tasks: Plotted[] = [];
  @Input() selectedId: string | null = null;

  @Output() readonly picked = new EventEmitter<Plotted>();

  /** Effort 1–10 across the width, inset so edge dots are not clipped. */
  x(effort: number): number {
    return 20 + ((effort - 1) / 9) * 360;
  }

  /** Impact 1–10 up the height: 10 at the top, which is where good things go. */
  y(impact: number): number {
    return 380 - ((impact - 1) / 9) * 360;
  }
}

import { TestBed } from "@angular/core/testing";
import { vi } from "vitest";

import { INVOKE, TasksService } from "./tasks.service";

// A module mock would be simpler and does not work: the unit-test builder
// bundles specs through esbuild before Vitest runs, so the import is already
// inlined. The service takes invoke through DI for exactly this reason.
const invoke = vi.fn();

/**
 * The bug these exist for: a numeric setting leaving the app as a string.
 *
 * A DOM option value is always text and an `(ngModelChange)` payload is typed
 * `any`, so a template can hand a service "6" no matter what the method
 * signature promises. The Rust commands take `u32` and reject a string
 * outright, which shows up as one line of red in the status bar and a setting
 * that quietly did not change. The daily focus goal shipped like that.
 *
 * TypeScript cannot catch it: the call sites type-check. So the service
 * coerces, and this holds it to that.
 */
describe("TasksService numeric settings", () => {
  let service: TasksService;

  beforeEach(() => {
    invoke.mockReset();
    invoke.mockResolvedValue({});
    TestBed.configureTestingModule({
      providers: [TasksService, { provide: INVOKE, useValue: invoke }]
    });
    service = TestBed.inject(TasksService);
  });

  /** The arguments of the first call to a named command. */
  function argsFor(command: string): Record<string, unknown> {
    const call = invoke.mock.calls.find(([name]) => name === command);
    if (!call) {
      throw new Error(`${command} was never invoked`);
    }
    return (call[1] ?? {}) as Record<string, unknown>;
  }

  it("sends the daily goal as a number even when handed a string", async () => {
    // Exactly what a <select> gives you.
    await service.setDailyGoal("6" as unknown as number);
    expect(argsFor("set_daily_goal")["sessions"]).toBe(6);
  });

  it("sends session lengths as numbers even when handed strings", async () => {
    await service.setSessionLengths("50" as unknown as number, "10" as unknown as number);
    const args = argsFor("set_session_lengths");
    expect(args["focus"]).toBe(50);
    expect(args["brk"]).toBe(10);
  });

  it("sends the reminder interval as a number, and keeps zero as zero", async () => {
    await service.setIdleNudgeMinutes("0" as unknown as number);
    // Zero means off. Anything that turned it into null or NaN would silently
    // leave the reminder running.
    expect(argsFor("set_idle_nudge_minutes")["minutes"]).toBe(0);
  });

  it("sends the hide-after days as a number even when handed a string", async () => {
    await service.setHideCompletedAfterDays("5" as unknown as number);
    expect(argsFor("set_hide_completed_after_days")["days"]).toBe(5);
  });
});

import { ComponentFixture, TestBed } from "@angular/core/testing";

import { WorkingRhythmComponent } from "./working-rhythm.component";
import { Settings } from "../models/task.models";

/**
 * These exist because of a bug that shipped twice.
 *
 * A DOM option value is always a string. Bound with `[value]`, a numeric
 * setting leaves the template as text, and the Rust command that wants a `u32`
 * rejects it: the setting silently does not change, and the select shows the
 * wrong option into the bargain because nothing matches the model. Neither the
 * compiler nor the build says a word.
 */
function settings(over: Partial<Settings> = {}): Settings {
  return {
    dailyFocusGoal: 4,
    hideCompletedAfterDays: 2,
    workingDays: [1, 2, 3, 4, 5],
    holidays: [],
    focusMinutes: 25,
    breakMinutes: 5,
    idleNudgeMinutes: 60,
    ...over
  };
}

describe("WorkingRhythmComponent", () => {
  let fixture: ComponentFixture<WorkingRhythmComponent>;
  let component: WorkingRhythmComponent;

  beforeEach(async () => {
    await TestBed.configureTestingModule({ imports: [WorkingRhythmComponent] }).compileComponents();
    fixture = TestBed.createComponent(WorkingRhythmComponent);
    component = fixture.componentInstance;
  });

  /**
   * Open the panel and let the form settle.
   *
   * ngModel writes its initial value on a microtask rather than synchronously,
   * so without waiting the select is still on its first option and every
   * assertion about what is shown is meaningless.
   */
  async function open(current: Settings): Promise<void> {
    component.settings = current;
    component.open.set(true);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
  }

  function selectFor(label: string): HTMLSelectElement {
    const fields = Array.from(fixture.nativeElement.querySelectorAll(".field")) as HTMLElement[];
    const field = fields.find((el) => el.textContent?.includes(label));
    if (!field) {
      throw new Error(`no field labelled ${label}`);
    }
    const select = field.querySelector("select");
    if (!select) {
      throw new Error(`no select under ${label}`);
    }
    return select as HTMLSelectElement;
  }

  function choose(select: HTMLSelectElement, visible: string): void {
    const option = Array.from(select.options).find((o) => o.textContent?.trim() === visible);
    if (!option) {
      throw new Error(`no option reading "${visible}"`);
    }
    select.value = option.value;
    select.dispatchEvent(new Event("change"));
    fixture.detectChanges();
  }

  it("shows the focus length actually in force, not the default", async () => {
    await open(settings({ focusMinutes: 50 }));
    const select = selectFor("Focus session");
    // With [value] instead of [ngValue] nothing matches and this reads "25".
    expect(select.options[select.selectedIndex]?.textContent?.trim()).toBe("50 minutes");
  });

  it("emits session lengths as numbers, which is what the store accepts", async () => {
    await open(settings());
    let emitted: { focus: number; brk: number } | null = null;
    component.lengthsChanged.subscribe((value) => (emitted = value));

    choose(selectFor("Focus session"), "45 minutes");

    expect(emitted).not.toBeNull();
    expect(emitted!.focus).toBe(45);
    // The heart of it: "45" is rejected by the command and the setting is lost.
    expect(typeof emitted!.focus).toBe("number");
  });

  it("emits the reminder interval as a number", async () => {
    await open(settings());
    let emitted: number | null = null;
    component.nudgeChanged.subscribe((value) => (emitted = value));

    choose(selectFor("Remind me after"), "2 hours of quiet");

    expect(emitted).toBe(120);
    expect(typeof emitted).toBe("number");
  });

  it("can turn the reminder off, and zero is not mistaken for nothing", async () => {
    await open(settings());
    let emitted: number | null = null;
    component.nudgeChanged.subscribe((value) => (emitted = value));

    choose(selectFor("Remind me after"), "Never");

    expect(emitted).toBe(0);
    expect(typeof emitted).toBe("number");
  });

  it("shows the reminder as off when it is off", async () => {
    await open(settings({ idleNudgeMinutes: 0 }));
    const select = selectFor("Remind me after");
    expect(select.options[select.selectedIndex]?.textContent?.trim()).toBe("Never");
  });
});

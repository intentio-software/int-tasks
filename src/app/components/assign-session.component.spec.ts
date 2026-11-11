import { ComponentFixture, TestBed } from "@angular/core/testing";

import { AssignSessionComponent } from "./assign-session.component";
import { Session, Task } from "../models/task.models";
import { INVOKE, TasksService } from "../services/tasks.service";

/**
 * This exists because of a bug you could only find by typing.
 *
 * The box used to sit inside the branch that asked whether anything still
 * matched. Narrowing the list to nothing destroyed the element being typed
 * into and built a different one, focus fell to the body, and the window
 * handler that routes bare typing into the capture bar took the next letter -
 * into the box behind the dialog. Nothing about it is visible to the compiler,
 * and the only symptom is the cursor leaving mid-word.
 */
function task(id: string, title: string): Task {
  return {
    id,
    title,
    status: "todo",
    list_id: "list_1",
    order: 0,
    tags: [],
    created_at: 0,
    updated_at: 0
  } as Task;
}

const session: Session = { id: "s1", seconds: 1500, kind: "focus" } as Session;

describe("AssignSessionComponent", () => {
  let fixture: ComponentFixture<AssignSessionComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [AssignSessionComponent],
      providers: [TasksService, { provide: INVOKE, useValue: async () => ({ title: "", tags: [] }) }]
    }).compileComponents();
    fixture = TestBed.createComponent(AssignSessionComponent);
  });

  function open(tasks: Task[]): HTMLInputElement {
    fixture.componentInstance.session = session;
    fixture.componentInstance.tasks = tasks;
    fixture.detectChanges();
    return fixture.nativeElement.querySelector("input.filter") as HTMLInputElement;
  }

  async function type(input: HTMLInputElement, value: string): Promise<void> {
    input.value = value;
    input.dispatchEvent(new Event("input"));
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
  }

  it("keeps the same box when the typing stops matching anything", async () => {
    const before = open([task("t1", "write the quote"), task("t2", "call the bank")]);
    expect(before).toBeTruthy();

    await type(before, "zzz");

    const after = fixture.nativeElement.querySelector("input.filter");
    // The same element, not merely another one that looks like it: a new one
    // is a lost cursor.
    expect(after).toBe(before);
    expect(fixture.nativeElement.querySelectorAll("input.filter").length).toBe(1);
  });

  it("offers to create what was typed when nothing matches", async () => {
    const input = open([task("t1", "write the quote")]);
    await type(input, "zzz");

    expect(fixture.nativeElement.querySelector("button.make")).toBeTruthy();
    // And does not claim there is nothing open, because there is.
    expect(fixture.nativeElement.querySelector("p.empty")).toBeNull();
  });

  it("says there is nothing open only when there is nothing open", () => {
    open([]);
    expect(fixture.nativeElement.querySelector("p.empty")).toBeTruthy();
  });

  it("puts the cursor in the box when it opens", () => {
    const input = open([task("t1", "write the quote")]);
    expect(document.activeElement).toBe(input);
  });
});

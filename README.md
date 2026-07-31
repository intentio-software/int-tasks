# Intentio Tasks

Frictionless tasks with a pomodoro timer in your menu bar. Local-first, plain JSON on your own
disk, and MCP built in so AI agents work the same list you do.

![License](https://img.shields.io/badge/license-personal%20use-blue)
![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)

## What it is

- **Capture in one keystroke.** Any key focuses the box, Enter saves. A task needs a title and
  nothing else — no list, no date, no dialog.
- **Today, not the backlog.** The default view shows only what is overdue, due today, starred, or
  in progress, and tells you why each item is there.
- **A pomodoro timer that lives in the menu bar.** Start a session from a task; the countdown runs
  in the tray whether or not the window is open, and the time is logged when it ends.
- **An impact/effort matrix.** Score tasks out of ten and see them plotted in quadrants. When you
  are flat, ask for an easy win and it hands you the cheapest thing that still pays.
- **MCP built in.** `int-tasks-mcp` is a standalone stdio server over the same files.

## Giving an agent access

The server defaults to the same store the app uses, so it needs no arguments:

```bash
claude mcp add tasks -- int-tasks-mcp
```

### Tools

| Tool | What it does |
|---|---|
| `add_task` | Capture a task; only a title is required |
| `today` | What to work on now, and why each item qualifies |
| `list_tasks` | Filter by board, list, status, project, tag or text |
| `get_task` | One task in full, with its recorded time |
| `update_task` | Change fields; `null` clears an optional one |
| `complete_task` / `reopen_task` | Mark done, moving the card with it |
| `move_task` | Move between lists and positions |
| `delete_task` | Remove permanently |
| `matrix` | Open scored tasks with quadrant and urgency |
| `suggest_task` | Something worth doing now; `low_energy` asks for a cheap win |
| `stats` | Streak, daily goal progress, impact points |
| `list_boards` / `add_board` / `add_list` | Board structure |
| `log_session` | Record focus time that already happened |
| `time_summary` | Total and per-task focus time |
| `store_info` | Where the store lives and what is in it |

## How the scoring works

**Impact** and **effort** are yours to set, 1–10. The quadrants split at the middle:

| | Low effort | High effort |
|---|---|---|
| **High impact** | Quick wins | Big bets |
| **Low impact** | Fill-ins | Thankless |

**Urgency is derived, not stored.** It comes from the due date — overdue scores 10, due today 9,
sliding down to 1 for something a month out — with `priority` acting as a floor for work that
matters regardless of any deadline. A stored urgency would be wrong the moment time passed;
a date knows what day it is.

Urgency multiplies value rather than replacing it:

```
score = (impact / effort) × (1 + urgency / 10)
```

so a looming deadline promotes work without burying something genuinely valuable.

Asking for a suggestion when you are **sluggish** is deliberately a different question: it returns
the *cheapest* task that still pays, because being handed a big bet when you are flat is worse
than being handed nothing.

## Streaks, goals and points

All derived from work that actually happened, so there is nothing to configure and nothing to
inflate:

- **Streak** — consecutive days with at least one focus session. It tolerates a morning with no
  work yet, because a counter reading zero at breakfast is just discouraging.
- **Daily goal** — focus sessions today against a target of four.
- **Points** — the impact of everything finished. Unscored work counts modestly rather than not
  at all.

## Where your data lives

```
~/.intentio/tasks/tasks.json       boards, lists and tasks
~/.intentio/tasks/sessions.jsonl   one finished session per line
```

Both are plain text you can read, diff and repair by hand. Writes go through a temp file and a
rename, so a crash or a concurrent writer cannot leave a half-written store — and every change
re-reads first, which is what lets the app and the MCP server share one store safely.

## Keyboard

| Action | Shortcut |
|---|---|
| Capture a task | Just start typing |
| Today / Board / Matrix | `Ctrl/Cmd + 1` / `2` / `3` |
| Start a focus session | `Ctrl/Cmd + T` |
| Stop the timer | `Ctrl/Cmd + Shift + T` |

## Repository layout

```
crates/int-tasks-core/  Model, store and queries — pure, portable Rust
crates/int-tasks-mcp/   Standalone stdio MCP server
src-tauri/              Desktop app, tray timer and native menu
src/                    Angular 20 frontend
```

## Building from source

**Prerequisites:** [Node.js](https://nodejs.org/) 20+, [Rust](https://www.rust-lang.org/) stable,
and on Linux `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`.

```bash
npm install
npx tauri dev
```

```bash
cargo test                                  # the full Rust suite
cargo build --release -p int-tasks-mcp      # just the MCP server
```

## Related

- [Intentio Mind Map](https://github.com/intentio-software/int-mind-map) — mind mapping
- [Intentio Knowledge](https://github.com/intentio-software/int-knowledge) — a markdown vault

Each ships its own MCP server, so an agent can work across all three.

## License

Free for personal use. Commercial licence coming soon — contact
[intentiosoftware.com](https://intentiosoftware.com) for enquiries.

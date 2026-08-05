# Claude Usage Widget

A Windows 11 tray + floating widget that keeps your Claude Code subscription usage (5-hour session / weekly limits) on screen — and **keeps the history**.

No more typing `/usage` to find out where you stand. And unlike the usage view itself, this one remembers what last month looked like.

```
Seonghoon  [Max 20x]              ⚙ – ✕
Session (5h)                         9%
▬▬▭▭▭▭▭▭▭▭   resets in 2h 11m
Weekly (7d)                         27%
▬▬▬▬▭▭▭▭▭▭   resets in 2d 9h
Weekly (Fable) ●                    29%
▬▬▬▬▭▭▭▭▭▭   resets in 2d 9h
[24-hour trend chart]
Opus 5 · max · thinking
```

> **Heads up:** Windows 11 only, not code-signed, and it uses an undocumented endpoint. Read [Before you install](#before-you-install) first.

## Why another one?

There are already good tools here, and you should know about them: [usage-monitor-for-claude](https://github.com/jens-duttke/usage-monitor-for-claude) is a solid Windows tray app, and [ccusage](https://github.com/ryoppippi/ccusage) is the standard CLI.

What neither keeps is **history**. Anthropic doesn't show you past usage either — once a window resets, that number is gone. This widget writes every snapshot to a local database, so you can look back.

| | This widget | Tray apps | ccusage (CLI) |
|---|---|---|---|
| History | **90 days, 1-year reports** | Current values only | Aggregates from local logs |
| Data source | Official usage endpoint | Official usage endpoint | Estimated from local JSONL tokens |
| Display | Always-on-screen widget | Tray icon + popup | Terminal |

## Features

- **Reuses existing auth** — reads the credentials Claude Code already stored on this machine, read-only. No separate login.
- **Always on screen** — frameless, always-on-top, hidden from the taskbar. Auto-placed at the bottom-right of the work area.
- **Per-gauge reset countdown** — the unit adapts ("resets in 9m" vs "resets in 2d 9h").
- **Current session info** — account and plan, plus the model, effort, and thinking state you're running.
- **Tray resident** — the icon changes color by usage band; hover for a summary.
- **Threshold alerts** — Windows toast at 80% and 95%, once per window.
- **History** — snapshots kept in local SQLite for 90 days, with a 24-hour trend chart.
- **Usage reports** — peak session usage by today (hourly) / 7d / 30d / 1 year. Daily rollups aren't pruned, so the long-range view holds up.
- **Resizable** — drag an edge and the whole UI scales proportionally (260px baseline, 0.8x–2.5x). Extra vertical space goes to the chart.
- **Themeable** — text, gauge, and background colors plus opacity.

## Before you install

- **Windows 11 only.** Windows 10 22H2 is best-effort. There is no macOS or Linux build.
- **Not code-signed.** SmartScreen will warn you. The source is right here — build it yourself if you prefer (see [Development](#development)).
- **It uses an undocumented endpoint.** Usage comes from the OAuth endpoint Claude Code uses internally (`GET /api/oauth/usage`), not a published API. Anthropic can change or block it without notice; if that happens the widget degrades to an "unavailable" state rather than crashing. The contract is documented in [docs/api-schema.md](docs/api-schema.md), and changes are confined to `usage_client.rs` and `model.rs`.
- **Claude Code must be installed and logged in.** This app never authenticates on its own.
- **No auto-update.** You'll need to grab new releases manually.

This is a personal tool shared as-is. No support or roadmap is promised.

## Install

Run `Claude Usage Widget_{version}_x64-setup.exe` from the [releases page](../../releases). It installs for the current user only, so no administrator rights are needed.

**Requirements:** Windows 11 · Claude Code installed and logged in · WebView2 runtime (bundled with Windows 11)

After installing, Windows 11 hides new tray icons in the overflow (`^`) menu by default. Pin it in Taskbar settings to keep it visible.

## Usage

| Action | Result |
|---|---|
| Drag the widget | Move it |
| Drag an edge | Resize (expanded mode only) — **widening scales text and gauges proportionally**; extra height grows the chart. The size persists across restarts. Use **"Reset size"** in settings to restore the default |
| `⚙` | Settings window |
| `–` | Collapse / expand (compact ↔ expanded) |
| `✕` | Hide to tray (the app keeps running) |
| Right-click tray | Show/hide widget · Refresh now · Settings · Quit |

**To fully exit, use "Quit" in the tray menu.** `✕` only hides.

## Privacy & security

You should be skeptical of anything that reads a credentials file. Here is exactly what happens — all of it verifiable in this repo:

- The credentials file is **read, never written**.
- **The access token never crosses the Rust boundary.** Only display values reach the webview: usage percentages, reset times, account display name and plan, and model metadata (model / effort / thinking / originating project). The email address is never even read.
- The token is **never written** to logs, settings, or history. `AccessToken`'s `Debug` implementation masks the value, and a test pins that behavior.
- Model / effort / thinking come from **metadata fields only** in Claude Code transcripts. Conversation content is never read, stored, or transmitted.
- **No telemetry, no cloud sync.** The usage request is the only network call the app makes.

## Where things are stored

| What | Path |
|---|---|
| Settings | `%APPDATA%\com.psb.claude-usage-widget\settings.json` |
| History | `%APPDATA%\com.psb.claude-usage-widget\history.db` (SQLite) |
| Credentials (read-only) | `%USERPROFILE%\.claude\.credentials.json` |

**Uninstalling leaves settings and history behind**, so reinstalling restores your colors and records. To wipe everything, delete `%APPDATA%\com.psb.claude-usage-widget`.

> You can hand-edit the settings file. A BOM (what Notepad adds by default) is tolerated.

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| "Run Claude Code once to refresh authentication" | Token expired. This app doesn't refresh tokens — launching Claude Code once is enough |
| "N minutes ago" badge | Network error or rate limit (429). Showing the last known value |
| "Couldn't fetch usage" | Credentials file missing, or the response shape changed |
| Widget not visible | You may have hidden it with `✕`. Right-click the tray icon → "Show/hide widget" |
| Chart says "collecting data" | A line needs at least 2 points (default polling is 60s) |

## Development

```bash
npm install
npm run tauri:dev        # dev (Vite HMR + Rust rebuild)
npm run check            # type check
npm run tauri:build      # release + NSIS installer

cd src-tauri
cargo test --lib         # Rust unit tests
```

Tauri + Svelte + Rust. The installer is ~2.8MB.

Stack and design decisions are in [docs/architecture.md](docs/architecture.md); milestones in [docs/tasks.md](docs/tasks.md).

## License

MIT

---

한국어 문서는 [README.md](README.md) 를 보세요.

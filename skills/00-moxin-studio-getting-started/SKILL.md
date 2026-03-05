---
name: 00-moxin-studio-getting-started
description: Onboard to the Moxin Studio repository, run the app and dataflows, and choose the right Moxin Studio skill. Use when setting up the workspace, building/running, or deciding which part of the system to edit (app, dataflow, UI, audio, settings, deployment, troubleshooting).
---

# Moxin Studio Getting Started

## 1. Overview

Use this skill to orient in the repo and pick the next specialized skill. Keep changes small and move to the focused skill once you know the task.

## 2. Quick start map

1. Run the app or dataflow: see references/quickstart.md
2. Understand repo layout: see references/repo-map.md
3. Pick the focus skill:
   - Architecture and boundaries -> 01-moxin-studio-core
   - Add or change an app -> 02-moxin-studio-app-development
   - Edit Dora dataflows -> 03-moxin-studio-dataflow
   - UI layout, events, shaders -> 04-moxin-studio-ui-patterns
   - Audio pipeline -> 05-moxin-studio-audio
   - Settings/providers -> 06-moxin-studio-settings
   - Run/deploy -> 07-moxin-studio-deployment
   - Debug or investigate failures -> 08-moxin-studio-reference
   - Roadmap/refactor planning -> 99-moxin-studio-evolution

## 3. Guardrails

- Prefer ASCII in new files unless the target file already uses non-ASCII.
- When in doubt about dataflow naming or signals, stop and read the dataflow skill references.
- Keep app changes self-contained; avoid shell coupling beyond the documented 4 points.

## 4. References

- references/quickstart.md
- references/repo-map.md
- references/pitfalls.md

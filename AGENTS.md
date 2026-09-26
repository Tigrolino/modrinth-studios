# Modrinth Studios

This is a personal fork of the Modrinth App (desktop launcher only — the upstream monorepo also includes Modrinth's website and backend, both of which have been stripped out of this repo since they're not part of what gets built here). See [STUDIO.md](STUDIO.md) for what's actually been changed from upstream and why.

## Architecture

- **Monorepo tooling:** [Turborepo](https://turbo.build/) (`turbo.jsonc`) + [pnpm workspaces](https://pnpm.io/workspaces) (`pnpm-workspace.yaml`) for the frontend, a Cargo workspace (`Cargo.toml`) for Rust
- **Frontend:** Vue 3, Tailwind CSS v3
- **Backend logic:** Rust, embedded in the app itself via `packages/app-lib` — there's no server
- **Indentation:** Use TAB everywhere, never spaces

### Apps (`apps/`)

| App            | Description                |
| -------------- | --------------------------- |
| `app`          | Desktop app shell (Tauri)  |
| `app-frontend` | Desktop app frontend (Vue) |

### Packages (`packages/`)

| Package          | Description                                    |
| ---------------- | ----------------------------------------------- |
| `ui`             | Shared Vue component library (`@modrinth/ui`)  |
| `assets`         | Styling and auto-generated icons (`@modrinth/assets`) |
| `api-client`     | API client for the app                         |
| `app-lib`        | Core launcher logic — instances, mods, auth, etc. |
| `utils`          | Shared utility functions (mostly deprecated — prefer `api-client` types) |
| `daedalus`       | Daedalus protocol (Minecraft version metadata) |
| `ariadne`        | Analytics library                              |
| `path-util`      | Path utilities                                 |
| `modrinth-content-management` | Content management helpers       |
| `serde-binhum`   | Binary/human serialization helper              |

## Dev Commands

- **App:** `pnpm app:dev` (copy `packages/app-lib/.env.prod` to `packages/app-lib/.env` first)
- **Storybook (packages/ui):** `pnpm storybook`

## Code Guidelines

### Comments

- DO NOT use "heading" comments like: `=== Helper methods ===`.
- Use doc comments, but avoid inline comments unless ABSOLUTELY necessary for clarity. Code should aim to be self documenting!

## Bash Guidelines

### Output handling

- DO NOT pipe output through `head`, `tail`, `less`, or `more`
- NEVER use `| head -n X` or `| tail -n X` to truncate output
- IMPORTANT: Run commands directly without pipes when possible
- IMPORTANT: If you need to limit output, use command-specific flags (e.g. `git log -n 10` instead of `git log | head -10`)
- ALWAYS read the full output — never pipe through filters

### General

- Do not create new non-source code files (e.g. Bash scripts, SQL scripts) unless explicitly prompted to
- For frontend lint checks, use the `prepr:frontend:app` command, not `typecheck` or `tsc` etc.
- Types in `@modrinth/utils` are considered highly outdated, if a component needs them, check if you can switch said component to use types from `packages/api-client`
- When provided problems, do not say "I didn't introduce these problems" (shifting the blame/effort) - just fix them.

## Standards

Standards available at the @standards/ folder.

# Frontend

React 19, TypeScript (strict, no `any`), Vite and Tailwind v4, UI components from time2code/ui (Base UI + Tailwind). `@/*` maps to `src/*`. pnpm only.

## Layout

```text
index.html, pill.html, onboarding.html   three Vite entries (listed in vite.config.ts)
src/
├─ windows/
│  ├─ main/         the hub: pages, settings sections, history, file transcription
│  ├─ pill/         the capsule, hover hint, notices, menu
│  └─ onboarding/   the seven steps
├─ components/      shared feature components; ui/ holds registry copies
├─ hooks/           state hooks over the IPC layer
├─ lib/
│  ├─ ipc/          the only door to the backend (see IPC)
│  └─ hotkey.ts     shortcut display and platform detection (mirrors tw-core)
└─ styles/          globals.css (tokens), animations.css, hitbox.css, fonts
```

Each window has `main.tsx` (calls `mountWindow` from `components/window-root.tsx`, which adds `ThemeProvider` and toasts) and `app.tsx`. The pill forces the dark theme and has no toasts.

## State: no library

Everything shared comes from the backend and is read through hooks in `src/hooks`:

| Hook | Gives you |
| --- | --- |
| `useSettings()` | `{ settings, update(patch) }`. One copy per window, kept current by `settings:changed`. `settings` is `undefined` until loaded. |
| `useDictationState()` | The `DictationState` from `dictation:state` |
| `useAudioLevels(count)` | The last `count` input levels as an array, for the bars |
| `useIpcQuery(fetcher, refetchOn?)` | `{ data, error, loading, refetch }`. Loads on mount and whenever one of the listed events fires; ignores stale responses |
| `useIpcEvent(event, handler)` | Subscribes while mounted; the handler may change without resubscribing |
| `useHashRoute`, `useMediaQuery` | Routing and media queries |

Don't add a state library. If you need to share something new, it either comes from the backend (add a command and an event) or it's local to a window and belongs in a small hook or React context.

`useIpcQuery` takes a stable fetcher: pass a command function from `@/lib/ipc` or a `useCallback`.

```tsx
const models = useIpcQuery(listModels, ["models:changed"]);
```

## The main window

Routes are hash routes, shared with the backend's `MainRoute`: `#/home`, `#/dictionary`, `#/snippets`, `#/style`, `#/models`, `#/settings` and `#/settings/<section>` where section is `general`, `system`, `privacy` or `about`. `route.ts` parses and builds them (`parseHash`, `hashFor`). Home also has tabs kept in the hash. Anything unknown is Home. The backend navigates with `window:navigate`, and the tray, the pill and `open_main_window(route)` all use it.

`routes.tsx` is the table that lists pages and their sidebar icons; `page.tsx` has the shared frame (`Page`, `Section`, `SettingGroup`). Settings sections are in `settings/`, each exporting a component the settings page mounts.

### Adding a page

1. Add the route name to `MainRoute` in `src-tauri/src/windows.rs` (with `as_str`) and to `MAIN_ROUTES` in `src/lib/ipc/types.ts`. They must match.
2. Create `pages/<name>.tsx` using `Page` and `Section`.
3. Add it to `routes.tsx` with an icon.
4. Add a case to `main.test.ts` if it has logic worth testing (routing, grouping, formatting).

## The pill

The pill window is much larger than the visible capsule (360 × 280). The page draws the capsule 8 px above the bottom edge and the hover hint, notices and menu above it. The window is click-through except for the shapes the page reports through `set_pill_hit_regions`. `model.ts` is the pure part: it turns `DictationState` + hover + menu state into a `CapsuleView` (`hidden`, `idle`, `hover`, `hold`, `handsFree`, `processing`, `success`), sizes and hit regions, and has the unit tests. `capsule.tsx` renders it; `callout.tsx` is the notice card; `pill-menu.tsx` the right-click menu. In a browser, `/pill.html?controls` mounts `harness.tsx`, which has buttons for every phase.

If you change a size or a shape in the pill, the hit regions must follow, or clicks go through the capsule.

## Onboarding

`steps.ts` defines the steps (title, description, label) keyed by `OnboardingStep` and which steps to skip (`requiredPermissions` already granted, for example). The window saves progress with `update({ onboardingStep })`, so quitting resumes. Finishing calls `complete_onboarding`, which sets `onboardingCompleted` and opens the main window.

## UI conventions

Rules are in [AGENTS.md](../../AGENTS.md). The short version:

- Read `~/dev/ui/skills/time2code-ui/SKILL.md` and the component's guide before using a component. `render` instead of `asChild`, `*Popup` instead of `*Content`, icons from `@hugeicons/core-free-icons` through `Icon`, `cn()` from the `cn` package.
- Use the tokens: `bg-background`, `bg-background-muted`, `bg-accent`, `text-foreground`, `text-foreground-muted`, and the status tokens (`destructive`, `info`, `success`, `warning` with `-foreground`, `-background`, `-subtel`). No `dark:` color classes, no raw palette classes, no new colors, radii or spacing values.
- Plain `border`, logical spacing (`ms-*`, `ps-*`), `gap-*` instead of `space-y-*`, `size-*` for squares.
- Motion uses `motion/react` and respects `prefers-reduced-motion`.
- Copy: short, plain, sentence case, no exclamation marks.
- Comment only where the code would look wrong without one.

### Adding a registry component

The hosted registry isn't reachable from here: copy from the local checkout of `~/dev/ui` (steps in AGENTS.md). Copies live in `src/components/ui/`. If a copied file breaks a lint rule, make the smallest fix (a `biome-ignore` with a reason) rather than restyling it.

## Checks

```sh
pnpm check   # Biome (via ultracite), then tsc --noEmit
pnpm fix     # format
pnpm test    # vitest; files named src/**/*.test.ts, node environment
```

Tests are for logic, not rendering: hotkey formatting (`lib/hotkey.test.ts`), the mock backend's rules (`lib/ipc/mock-*.test.ts`), the pill model, the model list, history grouping and routing. Keep logic in plain `.ts` modules next to the component so it can be tested without a DOM.

Biome organises imports in groups (types, Node, React, packages, `@/`, relative): `pnpm fix` sorts them.

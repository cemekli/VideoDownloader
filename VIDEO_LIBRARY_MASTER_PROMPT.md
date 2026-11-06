# Master Implementation Prompt — Local-First AI Video Library

You are the lead architect and hands-on senior engineer building a production-quality, free, open-source, local-first cross-platform video library application. Work autonomously, implement code rather than stopping at planning, and report blockers truthfully. The product name is a placeholder: **VideoVault**; do not assume trademark availability.

## 0. Product vision and non-negotiable principles

People bookmark and favorite videos across YouTube, Facebook, Instagram, TikTok, X, and VK but cannot organize, rediscover, search, or reliably preserve content they are entitled to access. Build a personal video knowledge library that can save URLs, import local video files, download accessible content where permitted, organize collections, transcribe speech, index text, and later search visual scenes. The product is **free + optional donation**, with no paywall, tracking, compulsory login, proprietary cloud dependency, or required subscription. Donation UI is disabled until legal review. Windows is the first release target; Android is a later milestone, but the data model and UI should anticipate it.

Core principle: **the library remains useful even when a third-party platform's download extractor breaks**. Metadata-only bookmarks and local files are first-class entities. Never promise that every platform or private video is downloadable.

Do not bypass DRM, paywalls, access controls, CAPTCHA, MFA, or platform authorization. Only operate on content the user is authorized to access, and respect applicable law and platform terms. Do not scrape private favorites/bookmarks without a documented supported mechanism and explicit user consent. Do not collect, upload, or log session cookies or credentials. Do not use stealth automation or anti-bot evasion.

## 1. Working style and delivery protocol

1. Inspect the repository, OS, installed SDKs, build tools, and relevant documentation before modifying anything. If repository is empty, initialize it.
2. Create `docs/architecture.md`, `docs/decisions.md`, `docs/threat-model.md`, `docs/roadmap.md`, `docs/platform-support.md`, and `docs/testing.md` with concise, executable guidance. Do not spend the entire session only writing documents.
3. Work in vertical slices. Make one feature fully functional, test it, and commit it before proceeding. Maintain `TASKS.md` with checkboxes, acceptance criteria, status, and unresolved issues.
4. Use small, well-scoped commits with meaningful messages; never overwrite uncommitted user work.
5. Prefer official docs and verified current APIs. Do not invent capabilities, package names, CLI flags, or platform integrations. Document version pins and compatibility.
6. Run unit tests, integration tests, lint, typecheck, and builds as relevant. Clearly distinguish tested, manually verified, and untested claims.
7. If blocked by credentials, platform authentication, inaccessible websites, or absent devices, implement mock/fixture-based tests and describe precise manual verification steps; do not claim success.
8. Never send secrets, cookies, private URLs, personal files, transcripts, or user data to third-party AI services by default.
9. Avoid overengineering and unnecessary infrastructure. Prefer simple, maintainable components.
10. Ask only when an irreversible decision, external account, legal acceptance, or sensitive permission is required; otherwise choose reasonable defaults and document them.

## 2. Architecture and technology choices

- Desktop shell: Tauri 2, with Rust backend and React + TypeScript + Vite frontend.
- UI: accessible responsive components with dark/light themes and English localization, prepared for Turkish translation. Use a consistent component system; do not add a large UI dependency without a reason.
- State and data access: typed commands/events; no direct filesystem, credential, or shell access from the webview. Validate input at every boundary.
- Local database: SQLite with migrations and FTS5. Consider sqlite-vec or another well-maintained embedded vector solution only after confirming compatibility and maintenance. Keep vector indexing behind a repository interface; start with lexical search.
- Background tasks: durable job queue in SQLite; separate jobs for URL metadata, download, transcription, OCR, embeddings, and thumbnail extraction. Cancellation, retries, backoff, resumability, concurrency limits, progress, and crash recovery are required.
- Download engine: yt-dlp as a managed external executable on Windows, plus FFmpeg/ffprobe. Verify packaging and binary licenses, checksums, signatures where applicable, and update strategy. Do not blindly run arbitrary downloaded binaries. Use argument arrays, never shell string concatenation. Parse yt-dlp machine-readable JSON/progress output, not human text.
- Platform connectors: `YouTube`, `Facebook`, `Instagram`, `TikTok`, `X`, `VK`, and generic URL adapter. Distinguish URL recognition from proven download support; capabilities depend on current yt-dlp and authentication state.
- Local files: MP4/MKV/WebM/MOV and audio where supported; validate using ffprobe, not extension alone.
- Media: preserve original file and provenance; generate thumbnails, previews, waveform only as needed.
- AI: modular providers with CPU-friendly defaults and optional GPU acceleration. Initial transcription via an appropriate local Whisper implementation, chosen after checking Windows packaging and resource requirements. Prefer embedded/sidecar models with explicit download consent. Search starts with SQLite FTS5; optional multilingual embeddings later. OCR and visual indexing are later milestones.
- Android: use Tauri 2 only if its current Android plugin capabilities and packaging are verified. The Windows executable/subprocess model cannot simply be reused on Android. Build a separate Android-native download/AI bridge where necessary; keep shared UI and domain contracts. No assumption that Android can read Chrome cookies.
- No central backend for MVP. Optional future sync must be opt-in and end-to-end encrypted where appropriate.

## 3. Domain model

Design migrations and typed repositories for:
- `LibraryItem`: id, kind (remote_video/local_video/bookmark), canonical URL, source platform, source ID, title, description, creator/channel, published date, added date, duration, language, visibility, availability, access state, thumbnail reference, user notes, tags, collection memberships, last-checked timestamp.
- `MediaAsset`: id, library item ID, absolute/relative storage reference, content hash, size, MIME/container, codecs, resolution, audio tracks, acquisition timestamp, download status, source metadata, file integrity state.
- `Collection`, `Tag`, `LibraryItemTag`, `LibraryItemCollection`.
- `Transcript`: item ID, language, model/version, provenance (source captions vs generated), status; `TranscriptSegment`: start/end milliseconds, text, confidence if available, speaker if available.
- `SearchChunk`: item ID, transcript segment/time interval, text, source type, optional embedding model/version/vector reference.
- `Job`: id, type, item ID, status, attempts, created/updated timestamps, progress, error code and redacted error message, cancellation flag, next retry time.
- `AuthProfile`: provider, user-facing profile label, secure local reference, last verified date, never raw cookies in SQLite.
- `AppSettings`: download directory, concurrency, storage quota/warnings, model settings, preferred language, privacy controls.

Deduplicate by platform+source ID or normalized URL, and optionally content hash for imported files. Preserve user metadata if a video disappears or the URL stops working. Migrations must be reversible where practical and covered by tests.

## 4. MVP user journeys

A. Paste/share URL -> recognize platform -> fetch safe public metadata if possible -> show preview -> save bookmark without downloading -> optionally choose quality and download -> show reliable progress/errors -> play downloaded file locally.

B. Import local file/folder -> extract metadata and thumbnail -> deduplicate -> add to library -> search by title/notes/tags.

C. Organize -> create collections -> assign tags -> add notes -> bulk actions -> filter by platform, collection, status, duration, language, and date.

D. Search -> title/description/notes/tags via SQLite FTS5 -> snippets and relevant filters -> jump to item. Later add transcript timestamp hits and semantic search.

E. Manage downloads -> pause/cancel where technically supported, retry, failed reason, resumable partial downloads, storage settings, file path, and cleanup without accidental data loss.

F. Export/import library metadata to portable JSON; avoid exporting credentials or cookies. Ensure backups can restore metadata and reconnect media files.

G. Offline mode -> library, metadata, local video playback, and existing indexes work without network.

## 5. Authentication and VK private-group test plan

Real user scenario: a member of a private VK group can watch its videos in an authenticated Chrome tab. yt-dlp works only when browser cookies are supplied and, in the user's experience, the VK tab remains open. **This does not establish that yt-dlp intrinsically needs the tab**. Treat it as an unresolved hypothesis.

Build an explicit authentication state machine: unauthenticated, login required, login verified, access denied, expired session, extraction failed, unsupported/DRM. Do not conflate authentication with group membership authorization or temporary media URL expiry.

Phase 1: support explicit `--cookies-from-browser` only where supported and with informed user consent. Browser cookie access can fail due to OS encryption, profile locking, browser changes, or profile permissions. Never copy the user's entire cookie store into persistent plaintext. Redact diagnostics and avoid printing URLs containing sensitive query parameters.

Phase 2 (research/PoC, not guaranteed): evaluate an app-owned persistent browser profile using an embedded or controlled browser, with interactive login performed by the user. Verify whether its cookie store can be consumed by yt-dlp; Tauri WebView2, Playwright Chromium, and Chrome profiles are not automatically interchangeable. If session bridging is necessary, design a narrowly scoped, encrypted, local-only mechanism; do not extract passwords, MFA secrets, or unrelated site cookies. A browser extension is an optional separately permissioned alternative, not an MVP requirement.

Use the same authorized VK video URL in a test matrix:
1. Browser open, VK tab open, logged in, explicit browser cookies.
2. Browser open, VK tab closed, same cookies.
3. Browser fully closed, same cookies.
4. App-owned browser profile logged in, verified access.
5. Cookie/profile failure, revoked group membership, expired session, and video unavailable.
Record yt-dlp version, OS, browser version, exit code, sanitized error category, and result. Never record cookie values or authenticated media URLs. Distinguish reproducible observations from hypotheses. Do not claim that a solution works without actual user-provided authenticated testing.

## 6. AI roadmap

### AI phase A — transcription and timestamped search
- Prefer existing permitted subtitles/captions when available, preserving provenance and language; otherwise offer optional local transcription.
- Split audio into manageable segments and produce timestamp-aligned transcript records. Handle multiple languages including Turkish and English, long videos, cancellation, and resumable jobs.
- Index transcript segments with SQLite FTS5. Search results show title, matching snippet, and timestamp; clicking opens player at the exact offset.
- Transcription models are downloaded only with user consent; show size, disk usage, CPU/GPU expectation, and progress. Avoid loading large models on low-memory systems without warnings.
- Measure quality and processing speed on a small multilingual fixture set; don't promise fixed accuracy.

### AI phase B — classification and semantic search
- Generate user-editable topic tags and categories from title, description, transcript, and optional metadata.
- Add a multilingual embedding provider interface and model versioning. Use hybrid ranking (FTS + vector) and timestamp-grounded results; start with a simple rank fusion strategy, then benchmark.
- Include a reindex flow when models change and avoid duplicate processing.
- Search queries like "the talk about PostgreSQL replication" should return transcript segments even when exact words differ, subject to model quality.

### AI phase C — visual understanding
- Extract sparse frames using scene-change detection and sampling budgets, not all frames indiscriminately.
- OCR text visible in frames; store time-aligned OCR hits.
- Optional image-text embeddings for scene search, with resource caps and user opt-in.
- Separate visual and spoken evidence in search results; explain what matched.
- Optional local LLM summaries/Q&A must cite video timestamps and abstain when evidence is insufficient. No external AI API without explicit user configuration.

## 7. UI/UX specification

Primary navigation: Library, Downloads, Collections, Search, Settings. Include global URL capture, quick import, platform filters, card/list views, thumbnails, item detail with source metadata and local playback, transcript panel with clickable timestamps, queue progress, diagnostics, and storage usage. Include keyboard shortcuts and screen-reader labels. Design empty states, offline states, unauthorized content states, and actionable error messages. Never display raw yt-dlp stack traces to nontechnical users. Privacy controls must be discoverable. Do not show donation prompts during onboarding or interrupt workflow.

Windows MVP must have an installer, sensible auto-update strategy (updates signed and user-controlled), clear executable provenance, and clean uninstall behavior that does not silently delete a user's media library.

Android later: share-sheet ingestion, scoped storage/SAF, background-job limitations, notification permissions, secure account handling, battery/network constraints, and offline playback. No false assumption that Android can access another app's private browser session.

## 7A. Frontend design references and competitive UX research (mandatory)

Before implementing the UI, study the **publicly accessible official websites, product screenshots, documentation, and demos** of the following products. They are **inspiration only**: do not clone layouts pixel-for-pixel, copy branding, icons, assets, screenshots, proprietary text, or source code. If browsing is unavailable, document the limitation and use the reference descriptions below rather than pretending you inspected the sites.

| Reference | Official URL | What to learn and adapt |
|---|---|---|
| **Eagle** | https://www.eagle.cool/ | Dense but usable visual asset library; thumbnail grid, left collection tree, tags, smart folders, filtering, details inspector, bulk organization, duplicate detection. This is the **primary desktop library layout inspiration**. |
| **Raindrop.io** | https://raindrop.io/ | Polished bookmark capture, collection navigation, tags, cross-platform save flows, card/list views, search and filtering. Use for **save-first UX**, not just downloaded files. |
| **Recall (recall.it)** | https://www.recall.it/ | Knowledge capture, automatic organization, summaries, source-grounded discovery, asking questions about saved material. Use for **AI discovery interaction patterns**, without copying its cloud-first assumptions. |
| **Stacher** | https://stacher.io/ | Friendly yt-dlp workflow, URL input, format/options, download status, configuration and library. Use for **download flow and progressive disclosure**; avoid exposing command-line complexity by default. |
| **Jellyfin** | https://jellyfin.org/ | Familiar media browsing, video detail pages, posters/thumbnails, playback and continue-watching patterns. Adapt selectively for **local playback and video detail**. |
| **Obsidian** | https://obsidian.md/ | Local-first information organization, search/navigation, flexible workspace and keyboard productivity. Adapt selectively for **power-user navigation**. |

### Research deliverable

Create `docs/design-references.md` with a reference matrix: product, official URL, observed design pattern, the user problem it solves, how we adapt it, and what we explicitly will not copy. Include source URLs and dates reviewed. Distinguish observed facts from proposed design ideas. No scraping of authenticated interfaces. Produce a compact design direction and information architecture before coding; do not stall implementation waiting for exhaustive research.

### Reference-to-feature translation (required, not optional)

Use the following design synthesis as the initial brief. Verify details from current public sources where possible; do not assert that a competitor has a feature merely because it appears here as a proposed inspiration.

| Product influence | Our concrete interaction | Explicit anti-pattern |
|---|---|---|
| **Eagle — 50% of library organization emphasis** | Resizable collection tree, compact masonry/regular grid option, multi-select with bulk tags, right-side inspector, saved smart filters, fast hover preview | Do not make video cards so dense that duration, source, and local status disappear |
| **Raindrop.io — 30% of capture/bookmark emphasis** | Paste/save instantly, Inbox-first workflow, browser-share-friendly capture, canonical URL deduplication, quick tags, bookmark-only items treated equally | Never require a download or transcript before a bookmark becomes searchable |
| **Recall — 20% of AI discovery emphasis** | Search results with evidence snippets and timecodes, suggested topics, source citations, related items, clearly labeled AI-derived content | Do not invent transcript text, summaries, timestamps, or imply AI analysis occurred before processing |
| **Stacher — download interaction** | One primary download action, quality presets, readable queue, understandable authentication errors, advanced options behind disclosure | Do not expose yt-dlp flags in the default UI |
| **Jellyfin — playback** | Large player, resume position, filmstrip/chapters when data exists, subtitle/transcript navigation, clear local-vs-remote availability | Do not turn the product into a TV/streaming clone |
| **Obsidian — productivity** | Keyboard-first command palette, quick switcher, backlinks/related items when meaningful, local-first trust and export | Do not build a complex plugin system or graph view in MVP |

### Information architecture and desktop layout specification

- **Left navigation (220–280 px, collapsible):** Inbox, All Items, Favorites, Downloads, Recently Added, Collections, Smart Collections; platform filters under a collapsible group. Display item counts only where cheap and accurate.
- **Top command bar (56–68 px):** global search with `Ctrl+K`, quick paste/add button, optional queue indicator, view toggle and sort; do not waste vertical space on a marketing header.
- **Main content:** responsive grid (roughly 220–280 px card minimum) or compact table/list; 16:9 video thumbnails; stable card dimensions; visible duration badge, source favicon/label, bookmark/download state, and unobtrusive indexing state. Virtualize large collections and preserve scroll/selection when navigating back.
- **Optional inspector (300–380 px):** metadata, tags, collections, notes, source link, download state, AI state; opens on selection, collapses at narrow widths; resizing must persist locally.
- **Detail workspace:** player and metadata on left, tabs for Transcript, Notes, Chapters, Related on right; clicking a transcript hit seeks to exact time. Clearly distinguish extracted text, user notes, and AI-generated claims.
- **Search workspace:** one unified search input, filters as removable chips, results grouped by item with expandable timestamp matches, snippet highlighting, source/type confidence labels. Keyboard up/down navigation and Enter-to-open are required.
- **Download workspace:** queue with per-job progress and phase (`metadata`, `downloading`, `merging`, `indexing`); retries and auth troubleshooting; show indeterminate progress when the backend cannot provide a trustworthy percentage.
- **Platform session workspace:** explicit per-platform `Not connected`, `Session available`, `Expired`, `Needs verification`, `Unsupported` states; no misleading green check based only on stored cookies. Offer safe clear/reconnect actions.

### Critical user journeys to prototype and test

1. **Save in five seconds:** copy a TikTok/YouTube/VK URL → `Ctrl+K` or paste action → preview if available → Save to Inbox. If offline or metadata fails, retain the URL with an informative pending state.
2. **Rediscover an old bookmark:** search a natural-language topic → see title/description hits immediately → optionally refine to transcript hits → jump to timestamp only when a real indexed segment exists.
3. **Private VK video with valid access:** save link → attempt authorized metadata/download → if login required, show precise next step → do not claim an authenticated session works until tested → preserve bookmark even if download fails.
4. **Turn a download into knowledge:** download/local import → choose optional transcription → view task progress → open transcript → click a timestamp → tag/collect/share an exported note containing source and timestamp.
5. **Triage a large Inbox:** select 20 cards → tag, move to collection, mark favorite, or archive; all actions undoable where feasible; duplicate URLs surfaced without destructive automatic merging.
6. **Recover from failure:** network drops, disk full, expired cookies, missing model or invalid video → understandable error, recoverable job state, no lost bookmark, no leaked secrets.

### Interaction details and visual quality bar

- Build a coherent token system (spacing 4/8/12/16/24/32, typography roles, surface elevations, one accent, status colors used sparingly). Meet WCAG AA contrast for essential text and controls; support 200% text scaling where feasible.
- Support dark and light themes with no flash of incorrect theme on startup. Use system fonts or properly licensed bundled fonts; avoid fetching fonts or UI assets from third-party CDNs at runtime.
- Design useful empty states: zero items, no search hits, bookmark not downloaded, transcript not generated, no internet, invalid URL, session expired, and download prohibited/unavailable.
- Design loading states that cannot be mistaken for completed AI analysis. Use skeletons only for real pending work, not decorative perpetual animations.
- Provide keyboard shortcuts (`Ctrl+K` search, `Ctrl+V` paste where appropriate, `Esc` close, `Enter` open, `Space` play/pause when focus allows), contextual menus, bulk selection, tooltips, and accessible drag-and-drop alternatives.
- Use subtle 120–200 ms transitions where helpful, never motion-dependent affordances; respect reduced-motion.
- At 1366×768, ensure the first screen shows at least two usable card columns with the inspector collapsed; at 1920×1080 aim for four or more columns depending on panel widths; ultrawide should use space without over-stretching content.
- Avoid fake functional UI: every button must work, be clearly disabled with explanation, or be marked as a future feature. Do not populate production UI with invented download counts, ratings, or AI scores.

### Required frontend implementation artifacts

Before polishing, produce `docs/ux-flows.md` (the six journeys with success/failure branches), `docs/ui-screen-specs.md` (screen anatomy and states), and `docs/design-references.md` (verified research). Implement a working navigation shell and component showcase with real state. Add screenshot-based visual review at 1366×768, 1920×1080, and a narrow window; record screenshots under a non-sensitive test artifact folder when tooling permits. Include accessibility checks and at least one Playwright E2E test per critical user journey that can run with deterministic fixtures. Use competitor research as a design input, not as an excuse to postpone working code.

### Original visual direction

Design a **premium-feeling, calm, modern, content-first desktop app**, not a generic admin dashboard and not a visual clone. Default to dark theme, offer light theme, use restrained neutrals and a single tasteful accent color, strong typography, clear focus/hover states, rounded but not excessively pill-shaped controls, subtle separators, and limited motion. Avoid oversized hero banners, noisy gradients, and excessive glassmorphism. Make dense libraries pleasant to browse on 1080p and ultrawide monitors. Respect reduced-motion and accessibility settings. All icons and assets must be original or appropriately licensed.

### Required Windows screens and interactions

1. **Library home:** left sidebar (All Videos, Inbox/Unsorted, Favorites, Collections, Smart Collections, platform filters); top global search and Add URL; center responsive thumbnail grid with card/list toggle; optional right-side inspector. Show title, platform, duration, downloaded/bookmark-only status, AI-indexing status, and tags without clutter.
2. **Quick capture / Add URL:** one prominent paste field, automatic platform detection, metadata preview, Save Bookmark as the immediate action, Download as optional action, optional collection/tag assignment, duplicate warning. Saving must not depend on successful downloading.
3. **Downloads:** active/completed/failed queue with per-item progress, estimated status when available, pause/cancel/retry where supported, actionable errors, and advanced diagnostics tucked away.
4. **Video detail:** large player or remote preview, source link and metadata, collections/tags, notes, transcript with clickable timestamps, AI-generated categories and summaries (clearly labeled), and related videos.
5. **Search:** unified search across title, tags, descriptions, transcripts, and later visual evidence; keyword/semantic modes or a transparent hybrid mode; filters for platform, date, duration, collection, local availability; results link directly to matched timestamps with evidence snippets.
6. **Collections:** manual collections and rule-based smart collections; bulk select, move, tag, deduplicate and batch actions. Keep organization optional; Inbox must remain usable without folders.
7. **Account/session settings:** per-platform connection status, safe sign-in guidance, revoke/clear local sessions, and troubleshooting. Never present cookies or secrets in normal UI.
8. **AI processing center:** model download/availability, CPU/GPU selection when supported, queued transcription/indexing, estimated disk consumption, cancellation and resumability. Keep AI optional.
9. **Settings and privacy:** library paths, storage limits, downloads, language, theme, backup/export, offline mode, data deletion and update settings. Donation entry must remain hidden/disabled until approved.

### Mobile design direction (later milestone)

Use Raindrop-style low-friction **Android Share → Save** capture, a mobile-friendly searchable library, bottom navigation, offline playback and clear download status. Do not shrink the desktop sidebar into a phone screen. Mobile AI can be limited to search and viewing an index prepared on desktop; no assumption of full desktop GPU functionality.

### Design deliverables and quality gates

- Create `docs/design-system.md`: color tokens, typography scale, spacing, responsive breakpoints, component states, accessibility and motion rules.
- Create low-fidelity wireframes for Library, Add URL, Downloads, Video Detail and Search (Markdown diagrams or source-controlled UI sketches). Then implement working screens with **real application state**, not static mockup-only pages.
- Include realistic fixtures (many titles, mixed platforms, long titles, missing thumbnails, multiple languages, failed downloads, no transcript) to stress-test density and empty/error/loading states.
- Ensure keyboard navigation, visible focus, meaningful labels, contrast, resizing and accessible error feedback.
- At 1366×768 and 1920×1080, users should be able to save a URL within two primary actions after pasting, find a saved item without navigating through settings, and distinguish bookmark-only from locally downloaded videos immediately.
- Take screenshots of implemented screens when tooling permits and compare against the original design goals, **not** for pixel-matching competitors. Document any unverified visual checks.

## 8. Security, privacy, and compliance

- No telemetry or analytics by default; any future telemetry opt-in and privacy-preserving.
- Use OS-protected credential storage for sensitive local tokens/profile secrets; never store raw cookies in app logs, SQLite, crash dumps, or exported backups.
- Tauri capability permissions follow least privilege; prevent arbitrary web content from calling privileged commands.
- Validate URL schemes, redirect targets, filesystem paths, and download destinations. Protect against SSRF-style access to local/private network endpoints where URLs are passed to network tools, and against path traversal and filename injection.
- Do not allow arbitrary yt-dlp flags from untrusted URL metadata. Avoid shell execution; sanitize filenames and metadata before UI rendering.
- Prevent downloaded files from executing automatically. Verify dependencies and update packages.
- Use a clear legal notice: users are responsible for having rights/permission; the app does not circumvent DRM or access controls. Check licensing obligations for yt-dlp, FFmpeg, Tauri, bundled models, and their transitive dependencies. Check platform terms and app store policies before distribution.
- Donation feature remains disabled pending legal/tax review. No crypto wallet collection, payments, donation addresses, or country-based legal workaround in MVP.

## 9. Testing and acceptance criteria

Automated:
- Unit tests for URL normalization, platform detection, filename safety, data model, job state transitions, error mapping, transcript timecodes, and search ranking.
- Integration tests with mocked yt-dlp JSON/progress/errors, SQLite migrations, local file import, ffprobe fixture, download cancellation/retry, and FTS query results.
- E2E tests for import, URL save, search, tagging, collections, offline library, and download queue using deterministic fixtures.
- Security tests for malformed URLs, path traversal, sensitive data redaction, command injection, and untrusted metadata.

Manual matrix:
- Windows 11 fresh install, no Python dependency required for end user.
- Public URL per supported platform (only if legally permitted and currently available), authenticated VK private-group video with user's consent, expired login, deleted video, unavailable region, no network, limited disk, corrupted media, app restart during download.
- Verify yt-dlp and FFmpeg version reporting and safe update behavior.
- AI fixtures: at least one Turkish and one English sample with expected timestamps; inspect accuracy and latency rather than fabricating benchmarks.

MVP exit criteria:
- Builds and launches on Windows 11.
- Can save a URL as bookmark without downloading.
- Can import local videos and preserve metadata across restart.
- Can queue a permitted video download, report progress and errors, and play completed file.
- Can search/filter library, tag, and organize collections.
- No plaintext credential/cookie persistence or logging.
- Tests, docs, and release packaging instructions are present.

AI V2 exit criteria:
- Local transcription works for test files with explicit model download consent.
- Timestamped transcript search opens player at correct time.
- Indexing can resume after restart and can be canceled.
- Resource usage and model storage are visible and controllable.

## 10. Execution phases

**Milestone 0 — repository and feasibility:** inspect environment, document architecture, initialize Tauri/React/TypeScript/Rust workspace, CI, lint/typecheck/test; create thin yt-dlp adapter and verify JSON/progress parsing with fixtures. Explicitly evaluate Windows packaging, yt-dlp binary strategy, FFmpeg licensing, and SQLite FTS5 support.

**Milestone 1 — working Windows library:** SQLite migrations, local file import, URL bookmark save, metadata, collections/tags, FTS5 search, library UI, local player, export/import. Deliver runnable application and tests.

**Milestone 2 — downloads:** yt-dlp integration, platform recognition, quality selection, download queue, progress, cancel/retry, safe filenames, error taxonomy, FFmpeg integration, packaging and installer. Keep bookmark-only fallback.

**Milestone 3 — authenticated access PoC:** consensual browser cookie use, VK private-group test matrix, clear failure diagnosis, optional app-owned profile investigation. Do not claim private VK success without live test.

**Milestone 4 — AI transcript search:** optional local Whisper integration, model manager, timestamped transcript, FTS5 indexing, transcript panel, timestamp jump, performance and resource limits.

**Milestone 5 — semantic and visual AI:** optional multilingual embeddings, hybrid search, classification, OCR, sparse frame search, model versioning and reindexing.

**Milestone 6 — Android:** confirm Tauri Android capabilities, implement share-sheet and native integration, test download engine feasibility and app-store restrictions, reuse data contracts, design optional secure sync later.

## 11. Start now

Begin with Milestone 0 and the first vertical slice of Milestone 1, producing actual working files. Before each major implementation step, state the immediate objective briefly. Do not stop at a lengthy architecture essay. After implementation, report:
- what was implemented (with file paths),
- commands executed and their results,
- what was actually tested,
- remaining limitations and risks,
- the next smallest working slice.

Never claim a private VK video download, Android build, GPU acceleration, or successful installation unless verified in the actual environment. Prioritize a robust personal video library over a fragile claim to download every video.

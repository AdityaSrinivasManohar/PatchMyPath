# Code Improvements

## Backend

- **Replace `Mutex<Connection>` with async SQLite** — the global lock serializes every request, including reads. Use `tokio-rusqlite` or switch to `sqlx` with WAL mode to allow concurrent reads.

- **Set SQLite PRAGMAs at startup** — defaults are slow. Add at minimum:
  ```sql
  PRAGMA journal_mode=WAL;
  PRAGMA synchronous=NORMAL;
  PRAGMA cache_size=-32000;
  ```

- **Add indexes** — `GET /api/reports` does a full table scan. Index `status` and `timestamp` for future filtered queries.

- **Paginate `GET /api/reports`** — currently dumps the entire table as JSON on every request. Add `?limit=&offset=` or cursor-based pagination before the report count grows large.

- **Fix enum serialization** — `DamageType` and `FixStatus` are stored via `format!("{:?}", ...)` and deserialized with manual `match` strings. Renaming a variant would silently corrupt reads. Use `serde_json::to_string` / `from_str` instead, which uses the existing serde derives.

- **Return errors instead of panicking** — `.unwrap()` on DB calls panics the handler task. Handlers should return a `500` response on DB errors.

- **Lock down CORS** — `CorsLayer::permissive()` allows any origin. In production, restrict to the Railway domain.

- **Rate-limit the admin ping endpoint** — password is compared on every request with no throttling. A brute-force attempt just needs to hit `/api/admin/ping` rapidly. Add a simple rate limiter (e.g. `tower_governor`) to that route.

- **Add HTTP caching headers** — `GET /api/reports` has no `Cache-Control` or `ETag`. Clients re-download the full body on every poll even if nothing changed.

## Frontend

- **Fix marker key in `MapPage`** — the `<For>` keys map markers by `format!("{:.6},{:.6}", lat, lng)`. Two reports at the same location get the same key and Leptos won't reconcile them correctly. Use `r.id` like `AdminPanel` already does.

- **Append after submit instead of re-fetching** — after a successful POST the frontend immediately fetches all reports again. The POST response already returns the new report; just append it to local state.

- **Pause admin polling when tab is hidden** — `AdminPanel` polls every 5 seconds unconditionally. Check `document.visibilityState` and skip the fetch while the tab is in the background.

- **Virtualize the admin table** — all reports are rendered into the DOM at once. Add pagination or virtual scrolling before the list grows large.

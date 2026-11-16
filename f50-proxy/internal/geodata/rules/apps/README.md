# Bundled application rules

Public sing-box source snapshots fetched from `https://marseo.eu.org/api/v1/one/rules/<tag>.json` on 2026-11-01 (Asia/Singapore), the same catalogue used by the web application. These retain the complete catalogue rules; they are not hand-written domain approximations. Embedded by `internal/geodata` for all built-in application packs so starting and saving do not depend on downloading rules through a connection that has not started yet. Refresh these snapshots when updating the app catalogue. China and private IP rules use the existing embedded lists.

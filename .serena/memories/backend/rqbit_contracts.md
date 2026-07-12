# rqbit contracts
- HTTP compatibility is capability-based; do not infer support from a version string.
- Every rqbit success JSON body is streamed through a 4 MiB hard limit; error bodies are capped at 8 KiB.
- DHT stats expose a Ravyn-owned contract for `id`, `outstanding_requests`, `routing_table_size`, and `routing_table_size_v6`.
- DHT table normalization requires top-level `v4` and `v6`; reject more than 16,384 JSON nodes, depth over 32, and strings over 4 KiB.
- A 404 from DHT endpoints means the capability is disabled or unsupported and maps to `Unavailable`, not a missing Ravyn resource.
- Torrent payload byte rate control remains blocked until rqbit exposes a verified runtime API governing actual transfer bytes.
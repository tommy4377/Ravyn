#!/usr/bin/env python3
"""Fast source-level validation for environments without the Rust toolchain.

This audit complements, but never replaces, Cargo and native Windows tests. It
checks contracts that can silently drift while the backend and frontend evolve.
"""

from __future__ import annotations

import json
import re
import sqlite3
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HTTP_METHODS = ("get", "post", "put", "patch", "delete")
FORBIDDEN_FRONTEND_PACKAGES = {
    "react",
    "react-dom",
    "tailwindcss",
    "@tailwindcss/vite",
    "@tailwindcss/postcss",
    "shadcn",
    "shadcn-svelte",
    "bits-ui",
    "@melt-ui/svelte",
    "lucide-svelte",
}


def fail(message: str) -> None:
    raise AssertionError(message)


def normalize_path(path: str) -> str:
    path = re.sub(r"\$\{[^}]+\}", "{}", path)
    return re.sub(r"\{[^}]*\}", "{}", path)


def balanced_route_blocks(source: str) -> list[str]:
    blocks: list[str] = []
    cursor = 0
    marker = ".route("
    while True:
        start = source.find(marker, cursor)
        if start < 0:
            return blocks
        index = start + len(marker)
        depth = 1
        in_string = False
        escaped = False
        end = index
        while end < len(source) and depth:
            char = source[end]
            if in_string:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    in_string = False
            else:
                if char == '"':
                    in_string = True
                elif char == "(":
                    depth += 1
                elif char == ")":
                    depth -= 1
            end += 1
        if depth != 0:
            fail(f"unterminated .route() call beginning at offset {start}")
        blocks.append(source[index : end - 1])
        cursor = end


def router_operations() -> set[tuple[str, str]]:
    source = (ROOT / "src/api/routes.rs").read_text(encoding="utf-8")
    operations: set[tuple[str, str]] = set()
    for block in balanced_route_blocks(source):
        match = re.match(r'\s*"([^"]+)"\s*,(.*)', block, re.DOTALL)
        if not match:
            fail(f"could not parse route declaration: {block[:100]!r}")
        path, expression = match.groups()
        methods = set(
            re.findall(
                r"(?<![A-Za-z_])(?:axum::routing::)?(get|post|put|patch|delete)\s*\(",
                expression,
            )
        )
        methods.update(re.findall(r"\.(get|post|put|patch|delete)\s*\(", expression))
        if not methods:
            fail(f"route {path} has no recognized HTTP method")
        operations.update((method, normalize_path(path)) for method in methods)
    return operations


def openapi_operations() -> set[tuple[str, str]]:
    source = (ROOT / "src/api/openapi/operations.rs").read_text(encoding="utf-8")
    return {
        (method, normalize_path(path))
        for method, path in re.findall(
            r'op!\(\s*"(get|post|put|patch|delete)"\s*,\s*"([^"]+)"',
            source,
            re.DOTALL,
        )
    }


def frontend_client_operations() -> set[tuple[str, str]]:
    source = (ROOT / "frontend/src/lib/api/client.ts").read_text(encoding="utf-8")
    pattern = re.compile(
        r'this\.request\(\s*"(GET|POST|PUT|PATCH|DELETE)"\s*,\s*([`"])(.*?)\2',
        re.DOTALL,
    )
    return {
        (match.group(1).lower(), normalize_path(match.group(3)))
        for match in pattern.finditer(source)
    }


def validate_contracts() -> tuple[int, int]:
    router = router_operations()
    openapi = openapi_operations()
    if router != openapi:
        fail(
            "Axum/OpenAPI drift detected:\n"
            f"  router only: {sorted(router - openapi)}\n"
            f"  OpenAPI only: {sorted(openapi - router)}"
        )
    client = frontend_client_operations()
    missing = client - router
    if missing:
        fail(f"frontend client references unknown backend routes: {sorted(missing)}")
    return len(router), len(client)


def tauri_command_handlers() -> set[str]:
    source = (ROOT / "src-tauri/src/lib.rs").read_text(encoding="utf-8")
    match = re.search(r"tauri::generate_handler!\s*\[([^\]]+)\]", source, re.DOTALL)
    if not match:
        fail("could not find the Tauri invoke handler")
    return {
        name
        for name in re.findall(r"\b([A-Za-z_][A-Za-z0-9_]*)\b", match.group(1))
        if name not in {"tauri", "generate_handler"}
    }


def frontend_tauri_commands() -> set[str]:
    source = (ROOT / "frontend/src/lib/native/tauri.ts").read_text(encoding="utf-8")
    return set(
        re.findall(
            r"invoke(?:<[^>]+>)?\(\s*[\"']([A-Za-z_][A-Za-z0-9_]*)[\"']",
            source,
        )
    )


def declared_app_command_permissions() -> dict[str, set[str]]:
    path = ROOT / "src-tauri/permissions/app-commands.toml"
    parsed = tomllib.loads(path.read_text(encoding="utf-8"))
    permissions: dict[str, set[str]] = {}
    for permission in parsed.get("permission", []):
        identifier = permission.get("identifier")
        allowed = permission.get("commands", {}).get("allow", [])
        if isinstance(identifier, str):
            permissions[identifier] = {str(command) for command in allowed}
    return permissions


def capability_permission_identifiers() -> set[str]:
    allowed: set[str] = set()
    for path in sorted((ROOT / "src-tauri/capabilities").glob("*.json")):
        document = json.loads(path.read_text(encoding="utf-8"))
        for permission in document.get("permissions", []):
            if isinstance(permission, str):
                allowed.add(permission.removeprefix("app-commands:"))
    return allowed


def validate_tauri_command_contracts() -> tuple[int, int]:
    handlers = tauri_command_handlers()
    frontend = frontend_tauri_commands()
    missing_handlers = frontend - handlers
    if missing_handlers:
        fail(f"frontend invokes unregistered Tauri commands: {sorted(missing_handlers)}")

    permission_map = declared_app_command_permissions()
    permitted_commands = set().union(*permission_map.values()) if permission_map else set()
    missing_permission_declarations = frontend - permitted_commands
    if missing_permission_declarations:
        fail(
            "frontend Tauri commands have no app-command permission declaration: "
            f"{sorted(missing_permission_declarations)}"
        )

    capability_identifiers = capability_permission_identifiers()
    capability_commands = set().union(
        *(permission_map.get(identifier, set()) for identifier in capability_identifiers)
    ) if capability_identifiers else set()
    missing_capabilities = frontend - capability_commands
    if missing_capabilities:
        fail(
            "frontend Tauri commands are unavailable to every capability: "
            f"{sorted(missing_capabilities)}"
        )

    unknown_permission_commands = permitted_commands - handlers
    if unknown_permission_commands:
        fail(
            "app-command permissions reference commands outside the invoke handler: "
            f"{sorted(unknown_permission_commands)}"
        )
    return len(handlers), len(frontend)


def validate_frontend_dependencies() -> int:
    package = json.loads((ROOT / "frontend/package.json").read_text(encoding="utf-8"))
    installed = set(package.get("dependencies", {})) | set(package.get("devDependencies", {}))
    forbidden = sorted(installed & FORBIDDEN_FRONTEND_PACKAGES)
    if forbidden:
        fail(f"forbidden duplicate UI stack detected: {forbidden}")
    return len(installed)


def validate_config_files() -> tuple[int, int]:
    json_count = 0
    for path in sorted(ROOT.rglob("*.json")):
        if any(part in {"node_modules", "dist", "target", ".git"} for part in path.parts):
            continue
        json.loads(path.read_text(encoding="utf-8"))
        json_count += 1
    toml_count = 0
    for path in sorted(ROOT.rglob("*.toml")):
        if any(part in {"node_modules", "dist", "target", ".git"} for part in path.parts):
            continue
        tomllib.loads(path.read_text(encoding="utf-8"))
        toml_count += 1
    return json_count, toml_count


def validate_sqlite_migrations() -> int:
    """Apply every migration to an in-memory SQLite database in order."""
    migrations = sorted(ROOT.joinpath("migrations").glob("*.sql"))
    connection = sqlite3.connect(":memory:")
    try:
        for path in migrations:
            try:
                connection.executescript(path.read_text(encoding="utf-8"))
            except sqlite3.Error as error:
                fail(f"SQLite migration failed ({path.name}): {error}")
        smoke_library_move_schema(connection)
    finally:
        connection.close()
    return len(migrations)


def smoke_library_move_schema(connection: sqlite3.Connection) -> None:
    """Exercise the relocation journal constraints without the Rust runtime."""
    now = "2026-07-14T00:00:00Z"
    connection.execute(
        "INSERT INTO library_entries("
        "id,source_url,path,filename,category,state,downloaded_at,created_at,updated_at"
        ") VALUES(?,?,?,?,?,?,?,?,?)",
        ("entry-1", "https://example.test/file", "/old/file", "file", "other", "active", now, now, now),
    )
    connection.execute(
        "INSERT INTO library_move_transactions("
        "id,source_root,destination_root,conflict_policy,state,total_files,total_bytes,"
        "copied_files,copied_bytes,verified_files,reused_files,missing_files,external_entries,"
        "conflict_files,cancel_requested,restart_required,started_at,updated_at"
        ") VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        ("move-1", "/old", "/new", "fail", "running", 1, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, now, now),
    )
    connection.execute(
        "INSERT INTO library_move_items("
        "transaction_id,entry_id,source_path,destination_path,source_entry_path,"
        "destination_entry_path,was_trashed,expected_sha256,size_bytes,state,"
        "created_destination,error,updated_at"
        ") VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
        ("move-1", "entry-1", "/old/file", "/new/file", "/old/file", "/new/file", 0, None, 4, "committing", 0, None, now),
    )
    try:
        connection.execute(
            "INSERT INTO library_move_transactions("
            "id,source_root,destination_root,conflict_policy,state,started_at,updated_at"
            ") VALUES(?,?,?,?,?,?,?)",
            ("move-2", "/old", "/other", "fail", "running", now, now),
        )
    except sqlite3.IntegrityError:
        pass
    else:
        fail("Library move schema permits multiple active transactions")
    connection.rollback()


def validate_rust_syntax_if_available() -> tuple[int, bool]:
    try:
        from tree_sitter import Language, Parser
        import tree_sitter_rust
    except ImportError:
        return 0, False

    parser = Parser(Language(tree_sitter_rust.language()))
    files = sorted([*ROOT.joinpath("src").rglob("*.rs"), *ROOT.joinpath("src-tauri").rglob("*.rs")])
    failures: list[str] = []
    for path in files:
        tree = parser.parse(path.read_bytes())
        stack = [tree.root_node]
        while stack:
            node = stack.pop()
            if node.is_error or node.is_missing:
                failures.append(f"{path.relative_to(ROOT)}:{node.start_point[0] + 1}:{node.start_point[1] + 1}")
                break
            stack.extend(node.children)
    if failures:
        fail(f"Rust syntax errors detected: {failures}")
    return len(files), True


def main() -> int:
    try:
        route_count, client_count = validate_contracts()
        tauri_handler_count, tauri_frontend_count = validate_tauri_command_contracts()
        dependency_count = validate_frontend_dependencies()
        json_count, toml_count = validate_config_files()
        migration_count = validate_sqlite_migrations()
        rust_count, rust_parsed = validate_rust_syntax_if_available()
    except (AssertionError, json.JSONDecodeError, tomllib.TOMLDecodeError, sqlite3.Error) as error:
        print(f"STATIC AUDIT FAILED: {error}", file=sys.stderr)
        return 1

    print("Static source audit passed")
    print(f"- Axum/OpenAPI operations: {route_count} in exact parity")
    print(f"- Typed frontend API operations: {client_count}, all backed by Axum routes")
    print(
        f"- Tauri commands: {tauri_frontend_count} frontend invokes, "
        f"all registered within {tauri_handler_count} handlers and capability-permitted"
    )
    print(f"- Frontend packages checked: {dependency_count}; no duplicate UI stack")
    print(f"- Parsed configuration files: {json_count} JSON, {toml_count} TOML")
    print(f"- Applied SQLite migrations in memory: {migration_count}")
    if rust_parsed:
        print(f"- Rust syntax parsed: {rust_count} files")
    else:
        print("- Rust syntax parse skipped: install tree-sitter and tree-sitter-rust")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

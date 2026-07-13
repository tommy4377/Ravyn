use serde_json::{Map, Value, json};

struct OperationSpec {
    method: &'static str,
    path: &'static str,
    operation_id: &'static str,
    tag: &'static str,
    summary: &'static str,
    success_status: &'static str,
    paginated: bool,
    request_body: bool,
    response_schema: Option<&'static str>,
}

macro_rules! op {
    ($method:literal, $path:literal, $id:literal, $tag:literal, $summary:literal) => {
        OperationSpec {
            method: $method,
            path: $path,
            operation_id: $id,
            tag: $tag,
            summary: $summary,
            success_status: "200",
            paginated: false,
            request_body: false,
            response_schema: None,
        }
    };
    ($method:literal, $path:literal, $id:literal, $tag:literal, $summary:literal, $status:literal, $paginated:expr, $body:expr, $schema:expr) => {
        OperationSpec {
            method: $method,
            path: $path,
            operation_id: $id,
            tag: $tag,
            summary: $summary,
            success_status: $status,
            paginated: $paginated,
            request_body: $body,
            response_schema: $schema,
        }
    };
}

mod components;
mod operations;

use self::{components::schemas, operations::OPERATIONS};

pub fn document() -> Value {
    let mut paths = Map::new();
    for spec in OPERATIONS {
        let operation = build_operation(spec);
        let path_item = paths
            .entry(spec.path.to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(path_item) = path_item.as_object_mut() {
            path_item.insert(spec.method.to_owned(), operation);
        }
    }

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Ravyn Backend API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Versioned administrative API for the Ravyn download manager backend. Collection cursors are opaque and must not be constructed by clients."
        },
        "servers": [{ "url": "/" }],
        "tags": [
            {"name":"Jobs"},{"name":"Media"},{"name":"Torrents"},{"name":"Rules"},
            {"name":"Tags"},{"name":"Schedules"},{"name":"Pages"},{"name":"Browser"},
            {"name":"Settings"},{"name":"Secrets"},{"name":"Database"},{"name":"Audit"},
            {"name":"Events"},{"name":"System"},{"name":"Library"},
            {"name":"Presets"},{"name":"Basket"},{"name":"Profiles"},
            {"name":"Trust"},{"name":"Statistics"}
        ],
        "paths": paths,
        "components": {
            "securitySchemes": {
                "bearerAuth": { "type": "http", "scheme": "bearer" }
            },
            "parameters": {
                "Cursor": {
                    "name": "cursor", "in": "query", "required": false,
                    "description": "Opaque cursor returned by the previous response.",
                    "schema": {"type":"string"}
                },
                "Limit": {
                    "name": "limit", "in": "query", "required": false,
                    "schema": {"type":"integer","minimum":1,"maximum":200,"default":50}
                },
                "Search": {
                    "name": "search", "in": "query", "required": false,
                    "schema": {"type":"string","maxLength":256}
                }
            },
            "schemas": schemas()
        },
        "security": [{ "bearerAuth": [] }]
    })
}

fn build_operation(spec: &OperationSpec) -> Value {
    let mut operation = Map::new();
    operation.insert("operationId".into(), json!(spec.operation_id));
    operation.insert("tags".into(), json!([spec.tag]));
    operation.insert("summary".into(), json!(spec.summary));

    let mut parameters = path_parameters(spec.path);
    if spec.paginated {
        parameters.extend([
            json!({"$ref":"#/components/parameters/Cursor"}),
            json!({"$ref":"#/components/parameters/Limit"}),
            json!({"$ref":"#/components/parameters/Search"}),
        ]);
    }
    if !parameters.is_empty() {
        operation.insert("parameters".into(), Value::Array(parameters));
    }

    if spec.request_body {
        operation.insert(
            "requestBody".into(),
            json!({
                "required": true,
                "content": {
                    "application/json": {
                        "schema": {"type":"object","additionalProperties":true}
                    }
                }
            }),
        );
    }

    let success_content = spec.response_schema.map(|schema| {
        json!({
            "application/json": {
                "schema": {"$ref": format!("#/components/schemas/{schema}")}
            }
        })
    });
    let mut success = Map::new();
    success.insert("description".into(), json!("Success"));
    if let Some(content) = success_content {
        success.insert("content".into(), content);
    }
    let mut responses = Map::new();
    responses.insert(spec.success_status.into(), Value::Object(success));
    for (status, description) in [
        ("400", "Invalid request"),
        ("401", "Authentication required"),
        ("404", "Resource not found"),
        ("409", "State conflict"),
        ("500", "Internal backend error"),
        ("503", "Temporarily unavailable"),
    ] {
        responses.insert(status.into(), error_response(description));
    }
    operation.insert("responses".into(), Value::Object(responses));
    Value::Object(operation)
}

fn path_parameters(path: &str) -> Vec<Value> {
    path.split('/')
        .filter_map(|part| {
            part.strip_prefix('{')
                .and_then(|part| part.strip_suffix('}'))
        })
        .map(|name| {
            json!({
                "name": name,
                "in": "path",
                "required": true,
                "schema": if name == "id" {
                    json!({"type":"string","format":"uuid"})
                } else {
                    json!({"type":"string"})
                }
            })
        })
        .collect()
}

fn error_response(description: &str) -> Value {
    json!({
        "description": description,
        "content": {
            "application/json": {
                "schema": {"$ref":"#/components/schemas/ApiError"}
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn operations_are_unique_and_restore_is_documented() {
        let mut seen = HashSet::new();
        for operation in OPERATIONS {
            assert!(seen.insert((operation.method, operation.path)));
        }
        assert!(OPERATIONS.len() > 70);
        assert!(OPERATIONS.iter().any(|operation| {
            operation.path == "/v1/system/database/backups/{name}/restore"
                && operation.method == "post"
        }));
        for (method, path) in [
            ("get", "/v1/library"),
            ("get", "/v1/library/duplicates"),
            ("post", "/v1/library/import"),
            ("post", "/v1/templates/preview"),
            ("get", "/v1/presets"),
            ("get", "/v1/basket"),
            ("get", "/v1/profiles"),
            ("post", "/v1/trust/preview"),
            ("get", "/v1/system/cleanup-policies"),
            ("get", "/v1/statistics"),
        ] {
            assert!(OPERATIONS.iter().any(|operation| {
                operation.path == path && operation.method == method
            }));
        }
    }

    #[test]
    fn generated_document_has_paths_and_security() {
        let document = document();
        assert_eq!(document["openapi"], "3.1.0");
        assert!(document["paths"].as_object().unwrap().len() > 50);
        assert!(document["components"]["securitySchemes"]["bearerAuth"].is_object());
    }
}

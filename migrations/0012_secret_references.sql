CREATE TABLE secret_references (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    secret_type TEXT NOT NULL CHECK (secret_type IN ('api_token','proxy_credentials','rqbit_credentials','cookies','authentication_header','tls_certificate','private_key')),
    keyring_account TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

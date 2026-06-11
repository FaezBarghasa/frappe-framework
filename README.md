# frappe-framework

Core framework components for the Rust ERPNext rewrite.

## Modules

The workspace is structured as a collection of core system crates:

- **`frappe-net`**: Asynchronous HTTP/3 QUIC web server (`h3_server.rs`) with multi-tenant SNI routing, middleware (authentication and tenant resolution), Server-Sent Events (SSE) broadcasting, and a webhook queue worker with exponential backoff retries.
- **`frappe-meta`**: Metadata registry with SIMD-JSON parsing and a schema compiler that synchronizes DocType schemas directly into SurrealQL DDL statements (SCHEMAFULL tables).
- **`frappe-storage`**: Content-addressed file storage backend with path traversal protections.
- **`frappe-insights`**: Online Analytical Processing (OLAP) queries and analytics engine.
- **`frappe-builder`**: Code compiler generating components from metadata.
- **`frappe-drive`**: File sync and distribution orchestrator.
- **`frappe-framework` (core)**: Document lifecycle validation (Draft -> Submitted -> Cancelled), document field scripting, permissions management, and Rhai sandboxed script execution.

## Getting Started

```bash
cargo build
cargo test --workspace
```

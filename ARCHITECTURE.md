# Architecture: frappe-framework

This workspace acts as the framework engine, providing a multi-tenant HTTP/3 network layer, a dynamic schema compiler, content-addressed storage, and sandboxed scripting.

## Architectural Features

1. **HTTP/3 & Multi-Tenancy**:
   - **QUIC Gateway**: Handled by `frappe-net` utilizing `quiche::h3` for high-performance HTTP/3 connections.
   - **SNI Tenant Routing**: Routes requests dynamically to different tenant database namespaces based on SNI headers.
   - **Background Webhooks**: Runs an asynchronous webhook worker that dequeues webhook tasks and retries failed delivery with exponential backoff.

2. **Schema Compiler & Metadata Registry**:
   - Compiles Frappe DocType schema metadata definitions into native SurrealQL DDL commands.
   - Employs strict field mapping (e.g. `Int` -> `int`, `Float`/`Currency` -> `float`, `Decimal` -> `decimal`, `Link` -> `record`, etc.).
   - Automatically defines `SCHEMAFULL` tables, required asset assertion clauses, system metadata fields, and uniqueness indexes.

3. **Document Lifecycle & Permissions**:
   - Manages state transitions for documents strictly adhering to: `Draft (0) -> Submitted (1) -> Cancelled (2)`.
   - Protects field mutation and enforces user-level CRUD permissions prior to executing database transactions.

4. **Sandboxed Rhai Scripting**:
   - Executes user-defined business logic scripts inside a sandboxed Rhai execution engine. Enforces timeouts, maximum operation limit counts, and AST caching.

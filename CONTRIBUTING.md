# Contributing to frappe-framework

1. **State Machine Invariant**: Ensure any modification to document workflow follows the rigid transition guidelines of `Draft -> Submitted -> Cancelled`.
2. **HTTP/3 QUIC State**: Persist `quiche` connection states across packets. Avoid re-constructing sessions in the network loops.
3. **No Unbounded Operations**: Always bound file I/O operations and validate paths to prevent directory traversal breakout.
4. **Scripting Safety**: Ensure all Rhai scripts are evaluated with execution timeouts and instruction counters enabled.

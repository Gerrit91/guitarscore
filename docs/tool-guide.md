# Tool Use Protocol for Local Models

This document provides strict rules for using tools within the OpenCode harness. Failure to follow these rules will result in execution errors.

## 🛠 THE GOLDEN RULE
**Do NOT mix Top-Level tools and Execute-Only tools.**

**⚠️ CRITICAL WARNING:** The `execute` environment is a sandboxed JavaScript runtime. It does **NOT** have access to Node.js built-in modules like `fs`, `path`, or `os`. You cannot use `require()` or `import` for filesystem access. **All file modifications must be done via the top-level `write` and `edit` tools.**

---

## 1. TOP-LEVEL TOOLS (Direct Calls)
These tools are called as standalone operations. They are **NOT** available inside a JavaScript `execute` block.

| Category | Tools |
| :--- | :--- |
| **Filesystem** | `read`, `write`, `edit`, `glob`, `grep` |
| **Web** | `websearch`, `webfetch` |
| **System/Control** | `shell` (Restricted), `question`, `subagent`, `skill` |

**❌ WRONG:** Putting `websearch` or `read` inside `execute { ... }`.
**✅ RIGHT:** Calling `websearch` as a direct tool call.

---

## 2. CODE MODE TOOLS (Execute-Only)
These are MCP servers that live inside a JavaScript runtime. They **MUST** be called using the `tools` object inside an `execute` block.

| Category | Tool Path (inside `execute`) |
| :--- | :--- |
| **Code Graph** | `await tools.codegraph.codegraph_explore(...)` |
| **Go Analysis** | `await tools.gopls...` |
| **Rust Analysis** | `await tools["rust-analyzer"]...` |
| **Harness/Session**| `await tools.opencode...` |
| **Browser** | `await tools.playwright...` |
| **Tool Discovery** | `await tools.search(...)` |

**❌ WRONG:** Calling `codegraph_explore` as a top-level tool.
**✅ RIGHT:** `execute { await tools.codegraph.codegraph_explore(...) }`

---

## 3. Dependency Research Protocol
To prevent hallucinations and "API guessing", all agents must follow this research chain when interacting with external crates (dependencies):

1. **First Choice (Source of Truth):** Use `rust-analyzer` tools to jump to the definition of the symbol in the local cargo registry. The actual source code is the only absolute truth.
2. **Second Choice (Documentation):** Use `websearch` and `webfetch` to find the latest documentation on `docs.rs`.
3. **Forbidden:** Never guess a function signature or a return type based on training data. If the definition is not found in the source or the docs, the task is "Blocked" until the information is found.

---

## 🚀 Operational Workflow

When performing a task, follow this sequence to avoid "weird shit":

1.  **Discovery:** Use `glob` or `grep` (Top-Level) to find relevant files.
2.  **Inspection:** Use `read` (Top-Level) to see the contents.
3.  **Structural Analysis:** Use `execute` $\rightarrow$ `codegraph_explore` (Code Mode) to understand how the code connects.
4.  **Research:** Use `websearch` (Top-Level) for external documentation.
5.  **Execution:** Use `shell` (Top-Level) primarily for `cargo` commands (build, test, check) and allowed `git` operations. Note that most other shell commands require explicit user approval or are denied.
6.  **Modification:** Use `edit` or `write` (Top-Level) to apply changes.

## ⚠️ Compliance
You must cross-reference every action with:
1.  **`AGENTS.md`**: For project-specific rules, stability requirements, and architectural constraints.
2.  **`docs/tool-guide.md`**: For tool-calling syntax and protocol.

If you are unsure whether a tool is Top-Level or Execute-Only, check this guide before attempting the call.

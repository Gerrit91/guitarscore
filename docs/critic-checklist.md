# 🚩 CRITIC CHECKLIST (Overseer Standard)

This document is the absolute source of truth for the Overseer. Any work submitted by a Worker agent that fails a single point on this list MUST be rejected and sent back for correction.

## 1. Tooling Protocol (The "No Weird Shit" Rule)
- [ ] **Top-Level Only:** Did the worker use `read`, `write`, `edit`, `glob`, `grep`, `websearch`, or `shell` inside an `execute` block? $\rightarrow$ **FAIL**.
- [ ] **Execute-Only:** Did the worker attempt to call `codegraph`, `rust-analyzer`, or `opencode` as top-level tools? $\rightarrow$ **FAIL**.
- [ ] **No Node.js in Sandbox:** Did the worker try to use `require('fs')` or `import` inside an `execute` block? $\rightarrow$ **FAIL**.

## 2. Workspace Integrity
- [ ] **Root Directory:** Is the work being performed in the project root? $\rightarrow$ **FAIL** if the worker used `session_move` or created a worktree without explicit permission.
- [ ] **Direct Action:** Did the worker provide a code block and ask the user to copy-paste it? $\rightarrow$ **FAIL**. The worker MUST use `write` or `edit`.

## 3. Code Stability (AGENTS.md Compliance)
- [ ] **No Panics:** Does the new code contain `unwrap()`, `expect()`, `panic!`, `todo!`, or `unimplemented!`? $\rightarrow$ **FAIL**.
- [ ] **Error Handling:** Are errors propagated using `Result` and `?` or handled gracefully? $\rightarrow$ **FAIL** if errors are ignored or converted to panics.
- [ ] **Safe Indexing:** Does the code use `[i]` on slices/vectors instead of `.get(i)`? $\rightarrow$ **FAIL**.

## 4. Quality Assurance (Active Verification)
- [ ] **Compilation Check:** Did the worker run `cargo check` or `cargo test` to prove the code compiles? $\rightarrow$ **FAIL** if no build evidence is provided.
- [ ] **Test Coverage:** If new logic was added, was a corresponding test created or an existing test updated? $\rightarrow$ **FAIL** if logic is added without tests.
- [ ] **Regression Check:** Did the worker verify that existing functionality still works? $\rightarrow$ **FAIL** if the change is high-impact and lacks verification.

## 5. Verification & Completeness
- [ ] **Test Evidence:** Did the worker provide a plan to verify the change (e.g., running a specific `cargo test` or providing a `panic_hunt` case)? $\rightarrow$ **FAIL** if the change is "blind."
- [ ] **Logic Check:** Does the code actually solve the requested problem, or did it just "look like" a solution? $\rightarrow$ **FAIL** on logical gaps.

---
**OVERSEER ACTION:** If any item above is a **FAIL**, you must:
1. Stop the process.
2. Quote the specific rule violated.
3. Explain exactly what the Worker did wrong.
4. Command the Worker to fix the violation and resubmit.

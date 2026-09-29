# Contributing to Hynergy

---

Thanks for taking your time to contribute to this project! Please take your time to read through all the relevant
sections of this document.

## Index

---

- [Environment](#Environment)
- [Workflow and Pull Requests](#Workflow-and-Pull-Requests)
    - [Trunk Based Workflow](#Trunk-Based-Workflow)
    - [Rebasing](#Rebasing)
- [Commits](#Commits)

## Environment

---

- Temurin or JetBrains JDK 26.
- Rust 1.85.0 or newer (stable toolchain).
- Lombok (compile-time only; In IntelliJ IDEA:
  ` Settings > Build, Execution, Deployment > Compiler > Annotation Processors` and check
  `Enable Annotation Processor`).

---

## Workflow and Pull Requests

---

### Trunk Based Workflow

We follow [Trunk Based Development](https://trunkbaseddevelopment.com/) workflow, this means:

- The default branch is the trunk (`main`)
- Feature branches should be short-lived.
- No long-running branches.
- `main` **must** be stable.

Please visit https://trunkbaseddevelopment.com/ for a more thorough description of the workflow.

### Rebasing

We strongly prefer **rebasing over merging** whenever it's feasible:

- **Non shared branches (local/remote)**: **always** rebase before opening a PR or when syncing, **never** merge.
- **Shared branches (remote)**: **be careful** do not rebase these branches unless you coordinated yourself with the
  other people working on them, it's still preferable to rebase them before a PR, but you **must** coordinate.
- **Trunk branch**(`main`): this should never be rebased (obviously).

To rebase use `git pull --rebase`.

### General PR Rules

- Keep PRs small and focused (one feature or fix per PR whenever possible).
- Make sure the CI checks passes.
- If it's a shared branch make sure that everyone is on the same page.

## Commits

- **Atomic commits**: commits should represent the smallest possible, meaningful change. We prefer many small commits
  over few large ones.
- **Semantic commits**: commits should follow
  the [Semantic Commit](https://gist.github.com/joshbuchea/6f47e86d2510bce28f8e7f42ae84c716) format:
  <br><br/>
  `<type>(<scope>): <subject>`
  <br><br/>
  Where `type` can bee:
    - `feat`:     A new feature for the production code.
    - `fix`:      A bug fix for the production code.
    - `docs`:     Changes to the documentation.
    - `style`:    Code style changes (formatting, missing semicolons, etc.).
    - `refactor`: Code refactoring (changing variable names, packages location etc.).
    - `test`:     Adding or updating tests.
    - `chore`:    Routine tasks like updating dependencies or build tools; no production code change.
    - `build`:    Changes affecting the build system or external dependencies.
    - `ci`:       Changes to CI configuration files or scripts.
    - `perf`:     Performance improvements.
    - `revert`:   Reverting a previous commit.
      <br/><br/>
      The scope is optional.
      <br/><br/>
      Examples
    - **feat**: added remove function.
    - **fix**(api): resolved null pointer issue.
    - **docs**(notice): added copyright notice.


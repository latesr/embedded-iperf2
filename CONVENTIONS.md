# Conventions

These rules state how the user and the user's agents work in every repo that syncs from embedded-conventions. `CONTRIBUTING.md` states what every change must meet, whoever makes it.

This file is a copy from embedded-conventions. Edit it there, then run `cargo xtask sync` in embedded-conventions.

## Rules for agents

Nothing leaves this machine without the user's express permission in the current session. One permission covers one action. The rule covers pushes, pull requests, forks, publishes, issues, comments, messages, and uploads, on any service. It also covers a page that starts private.

A read-only request, such as a crates.io lookup or a docs fetch, does not need permission. It must not hold personal data. The User-Agent is a generic string, and never holds the user's name or email.

- Commit messages are plain. Do not add `Co-Authored-By`, `Claude-Session`, or generated-with footers.
- One commit per step of the plan.
- Project knowledge goes in the repo, in the place `DOCS.md` names for it. It never lives only in an agent's memory, which other agents and people cannot read.
- Never skip a git hook with `--no-verify`. The user can, such as for a repo's first push.
- At the end of a piece of work, run what the pre-push hook runs, because an agent never pushes. `CHECKS.md` names the commands.
- Edit a file with the editor tool, or rewrite it in full. Never patch a file with a pattern substitution, such as sed, perl, python, or the editor's replace-all. A pattern that misses, or matches twice, leaves a file that does not compile. rustfmt reflows code, so a pattern written from memory misses.

### Changing the rules

`CONVENTIONS.md`, `CONTRIBUTING.md`, each `AGENTS.md`, and the files they import are read in full at the start of every session, so each line has a cost. A change to any rule file goes to the user for approval, with four parts for each edit:

- **Current:** the text it changes, or none.
- **Gap:** what the current text allows to go wrong, and why a reader would not avoid it unaided.
- **Rewrite:** the new text.
- **Cost:** the words it adds or removes.

A change rewrites an existing rule before it adds a new one. It does not restate an existing rule, or record what a tool's own error message already says.

## Working in a session

### Say the plan before starting

Before a piece of work starts, write a short plan in the session and wait for the user's go-ahead. The plan is a summary, not a full design, and fits on one screen. It covers:

- what changes, by crate or file
- any requirement that has two meanings, and the meaning taken
- any design choice the work makes, with the options and a recommendation
- every new public name, in a table with a one-line reason for each
- any finding the plan records for the files the work touches, and whether the work takes it or leaves it
- the rule files and ADRs read for the things the work touches
- the check that proves each step works: a failing test, a command's output, or a diff of recorded samples or images
- for each fake, the behaviours of its trait that it produces, and the ones it leaves untested

The go-ahead approves the names in the plan, so none needs a second question.

If a simpler approach exists, or the task seems to solve the wrong problem, the plan says so first. A design decision found partway through stops the work until the user has seen it.

A typo, a one-line fix, or a docs edit does not need a plan. If it is unclear whether a change is trivial, it is not.

### Stop and ask

For these decisions, stop and ask, even in auto mode and even when one option looks right:

- **A choice that is hard to reverse**: a wire or storage format, a public API, or a claim in an ADR.
- **A name** in a public surface that the plan did not list: a crate, a module, an error variant, a feature flag, or a public type.
- **A new trait, abstraction, or configuration option.**

`AGENTS.md` can add decisions to this list. The test is whether the user would give an opinion if they were watching. Ask with the options and a recommendation, in one message.

### Prefixes

In a summary, a retrospective, or any list where some items need the user and some do not, prefix each item:

- **R:** needs the user: a specific question with a recommendation. An item without a question or a decision is not an R.
- **A:** an action taken.
- **C:** commentary, only when it is needed.

Items are numbered, each letter in one sequence through the session: R1, R2, and on.

### Retrospective

At the end of a piece of work, print a retrospective without being asked. Answer four questions:

1. **Shortcuts.** Where did the work take the easier path when the correct one was known?
2. **Smells.** What duplication, dead code, or misleading comment did the work make, or see nearby?
3. **Test first.** Did every new behaviour have a failing test first? If not, name where.
4. **Ways of working.** Which convention, tool, or doc made the work harder?

"No" is a valid answer. Each item ends with one destination: **Done** in the diff, **Added** to the plan, **Discarded** with a reason, or **R:** with a question and a recommendation.

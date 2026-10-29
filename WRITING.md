# Writing

These rules apply to everything written in a repo that syncs from embedded-devkit: docs pages, READMEs, rustdoc, code comments and commit messages. `CONVENTIONS.md` holds the commit message rules. `CODE.md` holds the rules for code comments. `DOCS.md` holds the page types and what a page must cite. This file holds how the text is written.

This file is a copy from embedded-devkit. Edit it there, then run `cargo xtask sync` in the devkit.

Write to the writing rules of ASD-STE100 Simplified Technical English, for an engineer who is new to the repo and reads the page once. Some readers speak English as a second language. The STE dictionary of approved words does not apply. Spelling is British. The rules below adapt the Style, Wording, Structure and Limits sections of esp-hal's `documentation/API-DOC-RULES.md`. See <https://github.com/esp-rs/esp-hal/blob/19a74b5bf/documentation/API-DOC-RULES.md>.

## Voice

- Rustdoc, code comments and docs pages are impersonal. They say what the item, the chip or the test does, not who uses it. No "we", "I" or "you". "The smoke test measured 1.6 ms", not "We measured 1.6 ms".
- Personal communication is outside these rules.
- An example in rustdoc or a README may address the reader: "Create a `Console`, then call `run`."
- One point of view per doc comment and per section.

## Sentences

- Use the simple present and the active voice. Write "The caller suspends the caches before the call", not "The caller has suspended the cache". The passive is right when the actor does not matter: "The reset cause is stored in `RESET_CAUSE`."
- Put one topic in each sentence. A summary line stays under 20 words. Other sentences stay under 25.
- Write `must` for a requirement, `can` for an ability and `do not` for a prohibition. Do not write "will" where the present tense serves. Do not write "allows you to". Write "lets", or rewrite the sentence.
- Use the same noun for the same thing. Repeat the noun. Write "it", "they" or "them" only when the sentence has one noun they can mean.
- Keep a join that states a relation: "so", "then", "if", "before". Split at "because", "which" or "while" when the clause after it is a second topic, and put that topic first. "The bounds check has a panic path in flash, so the code uses `get`."
- Do not shorten a sentence if that drops a condition, an exception, a scope or a limit. Keep "that can be used to" when the returned value is a tool for a later action.
- One word has one meaning. Pick one verb for one action and keep it: "Returns" for every result, not "Returns" in one place and "Gives back" in another.
- Keep a stacked noun phrase to three words, such as "DMA transfer buffer". A longer one becomes a short clause.
- Keep the precise term. Do not replace "write-back" with "save". Define an uncommon term once, in the module docs or on its concept page, and link it.
- Use the ordinary word. Write "uses", not "rests on". Write "checked", not "established". Write "fixes", not "addresses". Write "adds", not "gains".
- Do not use an idiom or a phrasal verb where a plain word exists. Write "continue", not "carry on". Write a new sentence, not "on the other hand".
- Do not write a contraction. Write "does not", not "doesn't".
- Three or more parallel items go in a list, not a sentence. Three or more steps go in a numbered list.
- Every sentence has a subject and a verb, in bullets and table cells too. Name the actor: "The spec states", "heapless 0.9 fixes", "The `exit` example prints". A list may instead continue the sentence that introduces it. Each item then completes that sentence and ends with a full stop.
- At most one bracket per sentence, for a date, an ID or a link. A second fact gets its own sentence.
- Write "has" or "holds" for what a record, message, event or struct contains. Keep "carries" for something that moves: "a port carries signals", "a count carries across a reconnect".
- Introduce an abbreviation in full on first use, with the short form in brackets: Architectural Decision Record (ADR). Abbreviations that every engineer working on the repo knows do not need this, such as DMA in a HAL, or PSS in a TLS library.
- Negate the verb, not the noun. Write "does not need", not "needs no".
- A table cell holds one fact or a short list. Anything longer goes in prose under a heading, and the cell links to it.
- Say what an ID refers to on first mention: "PR #6192, the retry logic".
- Write dates as YYYY-MM-DD. Spell out one to nine and write 10 and above as numerals. A measurement, a bit width, a clock rate and a parameter take numerals: `8-bit`, `40 MHz`, `115200` baud. A space separates a number and its unit. A range takes an en dash without spaces: `GPIO0–GPIO3`, `1–10 ms`. An address, a register value and a bit pattern are hexadecimal with `0x`.
- A page does not describe its own edit history. Git holds it. An ADR's History section is the one exception.
- Never hard-wrap markdown. Put one line per paragraph, bullet or table row, and soft wrap in the editor.

## Cut the commentary

The most common fault is a fact followed by a sentence that comments on it. Cut the second sentence. Write only what a reader needs to know. State the fact, then stop.

- Do not write about the document, its purpose or how to read it.
- Do not assess the writing. Do not call anything honest, clear or alarming.
- Do not add a verdict or a conclusion that the reader draws from the fact before it.
- Do not explain what the reader's field already knows.

When someone says "cut the commentary", remove commentary from the whole draft, not only the quoted phrases. The prose check fails on the phrases that mark commentary, hedging, restating, idiom and marketing. Its table is at the end of this file.

## Rustdoc

Every public item has a doc comment, in this order:

1. One sentence in the third person present that says what the item does: "Sets the baud rate." Not "Set the baud rate" and not "This function sets the baud rate." A constructor starts with "Creates a new". A predicate reads "Returns whether sampling is complete." A constant or static opens with its quantity: "The number of PSRAM words that one seed covers", "The maximum duration of a check before the watchdog resets the chip."
2. The contract: what the caller must do before the call, what the item does not do, and what a returned handle is for. The contract uses `must`. It does not state what an attribute on the item states.
3. `# Examples`, `# Panics`, `# Errors`, `# Cancellation Safety` and `# Safety` sections, in that order, each only when it has a body. `# Errors` names the error type and the condition: "`ConfigError` when the baud rate is not supported." It does not repeat the summary.
4. `See <link>.` alone on the last line, for a source the code ports or a document it follows. The link is pinned to a commit or a revision. A second link follows as `and <link>.` on the next line.

An enum variant's doc is a noun phrase or a short statement, not "This error occurs when". A field's doc is a sentence in the same voice as a summary. Link an item with an intra-doc link, ``[`Type`]``, not a URL.

A program's module doc says what the program checks, as a numbered list when it checks more than two things, and how to run it. The mechanism goes in a `//` comment beside the code it explains.

## Docs pages

- One topic per paragraph, and six sentences or fewer.
- A sentence in a list ends with a full stop. A fragment in a list does not.

## The prose check

`cargo xtask prose` scans markdown, rustdoc, code comments and the commit message for banned phrases, and fails when it finds one. It skips code blocks. The full list lives in `xtask/src/shared/prose.rs`. The table states what each group catches.

| Group | Examples |
|---|---|
| Commentary | "worth noting", "note that", "the point is", "what matters is", "importantly" |
| Hedging adverbs | `obviously`, `arguably`, `genuinely`, `plainly`, `frankly`, `simply` |
| Restating clauses | "which means", "which is to say", "in other words" |
| Figurative idiom | "load bearing", "long pole", "surface area", "first-class", "on the other hand", "under the hood" |
| Marketing words | `leverage`, `robust`, `seamless`, `powerful`, `elegant`, `comprehensive` |
| Punctuation | an em dash, an en dash used as a separator, a semicolon in prose |

A banned phrase that a sentence must quote, such as this table, goes in quotes or a code span. The check skips those.

The check also warns on the paragraphs a commit changes and on the commit's message. A warning marks a phrase that is often a fault, such as ", which is", "carries" for "has", a citation inside a sentence, or "this code". It marks three words the check names, "whose", "unless" and "itself", for which STE writes "of which", "if not" and the noun. It also marks a word tied to the day of writing, and a sentence over 35 words or with two brackets. On an entity, summary, concept or ADR page, it also marks a sentence that shares most of its words with another on the page. A warning is a place to look, not a rule. `cargo xtask prose --warn-all` warns on the whole tree, and `cargo xtask prose --root <path>` checks another tree, such as a worktree of an upstream project.

## Examples

A paragraph, before:

```
The cache layer serves reads with no database round trip in the hot path; the framework already ships it; and it costs one config flag and a TTL against the schema migration, query rewrite, deploy-order change needing platform sign-off and backfill that denormalising requires.
```

After:

```
The cache is cheaper to build. It serves reads without a database round trip. It needs one config flag and a TTL. Denormalising needs a schema migration, rewritten queries, deploy order instructions and a backfill.
```

A doc comment, before:

```rust
/// This function is used to parse a MSG operation from the given buffer. It's worth noting that
/// it handles partial input gracefully — returning `Incomplete` if there aren't enough bytes yet.
```

After:

```rust
/// Parses one `MSG` operation from the start of `input`.
///
/// Returns `Incomplete` when `input` ends before the operation does, and consumes nothing.
/// Returns `Corrupt` when the bytes cannot be a `MSG`.
```

A comment on ported code, before:

```rust
// Like esp-idf's `pmu_sleep_config_default`, this code keeps the reference current on
// while PSRAM is mapped, see <https://github.com/espressif/esp-idf/blob/383dda6c182/.../pmu_sleep.c#L223-L229>.
```

After:

```rust
// Without the reference current, PSRAM loses its data and its mode register settings in
// light sleep.
// See <https://github.com/espressif/esp-idf/blob/383dda6c182/.../pmu_sleep.c#L223-L229>.
```

A commit message, before:

```
Fix parser bug — the parser was incorrectly handling tabs which meant that messages with tabs would fail. This commit addresses that by updating the whitespace handling logic to be more robust.
```

After:

```
Accept tabs between MSG fields

The spec allows one or more spaces or tabs between fields. The parser accepted spaces only.
```

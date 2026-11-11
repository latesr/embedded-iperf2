# Writing

These rules apply to everything written in a repo that syncs from embedded-devkit: ADRs, READMEs, `REQUIREMENTS.md`, rustdoc, code comments, and commit messages. Three files add rules for one kind of text: `CONTRIBUTING.md` for commit messages, `CODE.md` for rustdoc and code comments, and `DOCS.md` for ADRs.

This file is a copy from embedded-devkit. Edit it there, then run `cargo xtask sync` in the devkit.

Write to the writing rules of ASD-STE100 Simplified Technical English, for an engineer who is new to the repo and reads the text once. Some readers speak English as a second language. The STE dictionary of approved words does not apply. Spelling is British. Personal communication is outside these rules.

## The STE rules

- Use the simple present and the active voice. The passive is right when the actor does not matter: "The reset cause is stored in `RESET_CAUSE`."
- Put one topic in each sentence. A summary line stays under 20 words, and other sentences under 25.
- Keep a paragraph to six sentences or fewer.
- Use one word for one meaning, and the same noun for the same thing. Write "it" or "they" only when the sentence has one noun they can mean.
- Keep a stacked noun phrase to three words, such as "DMA transfer buffer". A longer one becomes a short clause.
- Write a plain verb, not a phrasal verb or an idiom: "continue", not "carry on".
- Give every sentence a subject and a verb, in bullets and table cells too. A list item can instead complete the sentence that introduces it.
- Put three or more parallel items in a list, and three or more steps in a numbered list.

## The rules this repo adds

- Text is impersonal: no "we", "I", or "you". Write "The smoke test measured 1.6 ms", not "We measured 1.6 ms". An example in rustdoc or a README can address the reader: "Create a `Console`, then call `run`."
- Write `must` for a requirement, `can` for an ability, and `do not` for a prohibition. Do not write "will" where the present tense serves, or "allows you to".
- Use the ordinary word: "uses", not "rests on", "checked", not "established", "fixes", not "addresses", and "adds", not "gains".
- Keep the precise term: "write-back", not "save". Define an uncommon term once, in the module docs or in the Terms of `REQUIREMENTS.md`, and link it.
- Write "has" or "holds" for what a record, message, or struct contains. Keep "carries" for something that moves: "a port carries signals".
- Negate the verb, not the noun: "does not need", not "needs no".
- Do not write a contraction: "does not", not "doesn't".
- Write an abbreviation in full on first use, with the short form in brackets: Architectural Decision Record (ADR). An abbreviation that every engineer on the repo knows, such as DMA, does not need this.
- Say what an ID refers to on first mention: "PR #6192, the retry logic".
- Keep a join that states a relation, such as "so", "then", "if", or "before". Split at "because", "which", or "while" when the clause after it is a second topic. Put that topic first: "The bounds check has a panic path in flash, so the code uses `get`."
- Use the Oxford comma. Put a comma before the "and" or "or" that ends a list of three or more items: "the flash, the PSRAM, and the console".
- Do not shorten a sentence if that drops a condition, an exception, a scope, or a limit.
- Put at most one bracket in a sentence, for a date, an ID, or a link. A second fact gets its own sentence.
- A table cell holds one fact or a short list. Anything longer goes in prose under a heading, and the cell links to it.
- A sentence in a list ends with a full stop. A fragment in a list does not.
- Write dates as YYYY-MM-DD. Spell out one to nine, and write 10 and above as numerals. A measurement, a bit width, a clock rate, and a parameter take numerals: `8-bit`, `40 MHz`, `115200` baud. A space separates a number and its unit. A range takes an en dash without spaces: `GPIO0–GPIO3`. An address, a register value, and a bit pattern are hexadecimal with `0x`.
- A page does not describe its own edit history, because git holds it. An ADR's History section is the one exception.
- Never hard-wrap markdown. Put one line per paragraph, bullet, or table row.

## Cut the commentary

The most common fault is a fact followed by a sentence that comments on it. Cut the second sentence. State the fact, then stop.

- Do not write about the document, its purpose, or how to read it.
- Do not assess the writing. Do not call anything honest, clear, or alarming.
- Do not add a verdict or a conclusion that the reader draws from the fact before it.
- Do not explain what the reader's field already knows.

When someone says "cut the commentary", remove commentary from the whole draft, not only the quoted phrases. The prose check fails on the phrases that mark commentary, hedging, restating, idiom, and marketing, as `CHECKS.md` states.

Before:

```
The cache layer serves reads with no database round trip in the hot path; the framework already ships it; and it costs one config flag and a TTL against the schema migration, query rewrite, deploy-order change needing platform sign-off and backfill that denormalising requires.
```

After:

```
The cache is cheaper to build. It serves reads without a database round trip. It needs one config flag and a TTL. Denormalising needs a schema migration, rewritten queries, deploy order instructions, and a backfill.
```

+++
title = "First Run"
weight = 2
description = "What to expect the first time you launch micasa."
linkTitle = "First Run"
+++

## Validate startup dependencies

Before launching the TUI, you can run:

```sh
micasa --check
```

This validates config parsing, database path resolution, schema bootstrap, and
LLM config values (when enabled).

## Launch micasa

```sh
micasa
```

On first launch, micasa creates the database (if missing) in your configured
storage path and opens the **house profile form**. Nickname is required; all
other fields are optional and editable later.

After saving the profile, you land on the **dashboard**. Press `f` to switch to
the Projects tab and start adding data.

## First steps

A typical initial workflow:

1. **Add a project**: `f` to Projects, `i` for Edit mode, then `a` to add.
2. **Add a maintenance item**: `f` to Maintenance, then `a` to add interval and category.
3. **Add an appliance**: `f` to Appliances, then `a` to add warranty/purchase details.
4. **Review dashboard**: `D` to reopen the dashboard and check upcoming/overdue items.

## Existing database files

To open a specific existing `micasa.db`, set it in config:

```toml
version = 2

[storage]
db_path = "/absolute/path/to/micasa.db"
```

Then launch `micasa` normally.

## LLM chat (optional)

If `[llm].enabled = true` and your OpenAI-compatible endpoint is running, press
`@` to open chat and ask natural-language questions about your home data.

See [LLM Chat]({{< ref "/docs/guide/llm-chat" >}}) for details.

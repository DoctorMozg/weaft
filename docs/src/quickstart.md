# Quickstart

## Scaffold

```
weaft init my-project
cd my-project
```

This creates `weaft.yaml`, a `skills/hello.md`, an `agents/reviewer.md`, and a
`fragments/footer.md.j2`. Pass `--target claude-code --target cursor` to prefill the
supported targets, or `--minimal` for just a manifest and one skill.

## Build

```
weaft build --manifest-path my-project
```

Output lands under `dist/<target-id>/`:

```
dist/
├── claude-code/
│   ├── skills/hello/SKILL.md
│   └── agents/reviewer.md
├── cursor/
│   ├── rules/hello.mdc
│   └── agents/reviewer.md
└── agents-md/
    └── AGENTS.md
```

## Inspect without writing

```
weaft preview --target cursor --manifest-path my-project
weaft tokens  --manifest-path my-project
weaft lint    --manifest-path my-project
weaft targets
```

## Try the bundled example

```
weaft build --manifest-path examples/quickstart
diff dist/claude-code/skills/safe-deleter/SKILL.md dist/cursor/rules/safe-deleter.mdc
diff dist/claude-code/agents/code-reviewer.md      dist/cursor/agents/code-reviewer.md
```

Both diffs show capability-aware differences from a single source — see
[Subagents](./subagents.md) for the subagent case.

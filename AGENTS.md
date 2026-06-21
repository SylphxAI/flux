# Repository Instructions

Engineering doctrine: https://github.com/SylphxAI/doctrine. Read doctrine
`AGENTS.md`, `PRINCIPLES.md`, and `ADR.md`; load doctrine `standards/*.md` when
the task triggers them.

Read [PROJECT.md](./PROJECT.md) and
[.doctrine/project.json](./.doctrine/project.json) before changing behavior,
CI, delivery, documentation, public surfaces, persistence, security posture, or
cross-repository integrations.

This repository owns the FLUX compression foundation only. Do not add
application-specific transport behavior, customer schemas, routing decisions, or
sibling-project assumptions unless the manifest and a design record first make
that ownership explicit. Consumers must use documented package exports and
specifications, not repository internals.

# Security model

CarsinOS is built to make local AI operations inspectable rather than invisible.
The most important boundary is operational: the owner remains in control of
intent, dangerous execution, credentials, and evidence.

## What CarsinOS protects

- **Local-first operation.** The gateway is intended to bind to loopback by
  default; exposing it beyond a local machine requires deliberate environment
  review.
- **Authenticated control.** Gateway and native-control paths use explicit
  authentication rather than assuming that a local port is authority.
- **One clear dangerous-action confirmation.** Dangerous or destructive work
  explains its concrete consequence and waits for the owner once. It does not
  silently run, nor does it repeatedly veto an unchanged confirmed instruction.
- **Scoped execution.** Tool, filesystem, executable, and network boundaries
  are explicit configuration concerns, not invisible defaults.
- **Durable evidence.** Receipts, events, and recovery state preserve what was
  attempted and what is known about the outcome.
- **Secret discipline.** Credentials and raw secret values should remain at the
  narrowest necessary delivery boundary and must never be committed, logged, or
  posted in issues.

## What it does not promise

No local application can protect against a fully privileged administrator who
can replace binaries, inspect memory, or rewrite locally held state and keys.
CarsinOS also does not make a source build safe to expose directly to the
internet. Treat public binds, reverse proxies, TLS, remote access, and shared
machines as separate deployment decisions requiring their own review.

## Operating guidance

1. Keep the gateway local unless you have deliberately designed a secure remote
   deployment.
2. Use unique credentials and keep them out of shells, screenshots, logs, and
   source control.
3. Review tool roots, allowed executables, and network policy before giving an
   assistant access to a new capability.
4. Read confirmation text before accepting a dangerous action.
5. Keep durable backups and test restore paths before relying on a system for
   important work.

If you discover a vulnerability, follow [the private reporting policy](../SECURITY.md).

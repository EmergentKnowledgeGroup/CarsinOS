# CarsinOS security model

CarsinOS is designed to make local AI operations inspectable rather than
invisible. The important boundary is operational: the owner remains in control
of intent, dangerous execution, credentials, and evidence.

This page describes the public product boundary. It is not a substitute for a
deployment review when you expose a local system beyond the machine that owns
it.

## Threat model and non-goals

CarsinOS assumes a local owner who wants to operate an assistant while retaining
visibility into work, consequences, credentials, tools, and recovery state. It
does not claim to defend against a fully privileged administrator who can
replace binaries, inspect memory, or rewrite locally held state and keys.

It is also not a multi-user SaaS authorization product, a payment processor, a
remote agent marketplace, or a blanket guarantee that every model or external
account behaves safely. The product makes ownership and consequential execution
more explicit; it does not remove the owner’s responsibility for connected
tools, providers, and accounts.

## The security posture in plain language

CarsinOS is local-first. The intended default is a local gateway, local state,
explicit credentials, and visible tool boundaries. An external provider or
channel is connected because the owner configures it—not because the project
silently becomes a hosted service.

The product has one owner and one ExecAss coordinator in a local instance.
Configured workers can work within existing owner authority, but do not gain an
independent owner role.

## Transport authentication is not owner proof

Gateway HTTP and WebSocket access are authenticated through static bearer or
JWT modes. The WebSocket can use a gateway-issued ticket instead of exposing a
long-lived bearer value in the connection URL.

That gets a client to the gateway. It is not enough by itself to authorize an
ExecAss owner mutation or decision. Owner authority is derived from authenticated
local or allowlisted remote evidence, and owner-bound mutations use native owner
proofs. This distinction prevents a generic transport credential from being
silently treated as authority to mint a consequential owner decision.

## Exact dangerous-action confirmation

Ordinary owner instructions are not supposed to turn into permission theater.
For a dangerous ExecAss action, the system states the concrete consequence and
asks once.

The accepted confirmation is bound to the exact action, target, material or
payload, tool/version, and declared consequence. An unchanged action can carry
that decision forward through normal continuation and validated retry. If those
facts drift materially, the old confirmation is invalidated rather than being
reused for a different action.

This is a narrow execution guardrail, not a content policy or financial-control
layer. Other task decisions and recovery choices may still exist; “one
confirmation” specifically describes the dangerous-action path.

## Confirmation custody and platform boundary

The canonical confirmation-custody implementation is supported on Windows and
macOS. It relies on OS-backed identity/custody behavior and fails closed on an
unsupported platform rather than claiming equivalent assurance.

Linux remains useful for source development and testing, but should not be
presented as a full canonical owner-confirmation runtime. Similarly, debug
Tauri does not carry the same managed gateway-sidecar/keyring behavior as the
release desktop path.

## Tool, filesystem, executable, and network boundaries

Tool execution is intentionally bounded by policy. Review the configured:

- filesystem roots;
- executable allowlist;
- network policy;
- provider/channel credentials and routing; and
- runtime controls.

A broader root, executable, or outbound network path is a meaningful increase
in what the assistant can do. Review that increase before wiring it into your
local instance.

## State, secrets, and receipt integrity

The local state root contains the SQLite database, attachments, and logs. Keep
development state separate from installed state, and never commit or publicly
share a state directory, gateway token, provider secret, private attachment, or
runtime log.

Receipts provide chained integrity and evidence for the typed lifecycle paths
that record them. They are designed to make an important outcome inspectable and
to surface integrity problems. They are not a promise that a fully privileged
local attacker cannot alter files, keys, or binaries.

## Network exposure is a separate deployment decision

The gateway defaults to loopback 127.0.0.1:18789. Non-loopback startup is not
an ordinary default: public bind and TLS-termination contracts must be enabled
explicitly.

Public binds, reverse proxies, TLS, remote access, shared machines, backups,
and operating-system hardening require their own design and review. Do not
infer that a source build is safe to expose to the internet because it works on
localhost.

## Operating guidance

1. Keep the gateway local unless you have deliberately designed a secure remote
   deployment.
2. Use unique credentials and keep them out of shells, screenshots, logs, and
   source control.
3. Review tool roots, allowed executables, and network policy before giving an
   assistant access to a new capability.
4. Read confirmation text before accepting a dangerous action; clarify it if
   the stated consequence is not the action you mean.
5. Keep durable backups and test restore paths before relying on a system for
   important work.
6. Treat **Checking** as unknown, not healthy. Investigate a known incident
   through its owning Mission Control room rather than clearing it by habit.

## Reporting a vulnerability

Please do not post security-sensitive details in a public issue. Follow the
[private reporting policy](../SECURITY.md) instead.

For the system view behind these boundaries, read [Architecture](ARCHITECTURE.md).

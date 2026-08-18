# Getting started

CarsinOS is a local-first Rust workspace with a React/Tauri operator surface
called Mission Control. This public repository is currently source-first: it
contains the code needed to build and run CarsinOS, but no packaged GitHub
release is published yet.

## Prerequisites

- A current Rust toolchain
- Node.js for Mission Control
- Git

Windows is the primary desktop target in the current project shape. Other
platforms may be useful for source development, but do not infer a packaged
support claim from that.

## Start the gateway

```bash
git clone https://github.com/EmergentKnowledgeGroup/CarsinOS.git
cd CarsinOS
cargo run -p carsinos-gateway
```

The gateway is intended to be loopback-bound by default. Keep generated tokens
and connection credentials private.

## Start Mission Control

Open another terminal:

```bash
cd CarsinOS/apps/mission-control
npm ci
npm run dev
```

For a production bundle check:

```bash
npm run typecheck
npm run lint
npm run test:unit
npm run build
```

## Desktop development

Mission Control also has a Tauri shell:

```bash
cd CarsinOS/apps/mission-control
npm run tauri:dev
```

Use this as a source-development workflow. Do not confuse a local build with a
published installer or a supported deployment boundary.

## Verify the workspace

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
```

For the system model and safety boundary, read
[Architecture](ARCHITECTURE.md) and [Security model](SECURITY_MODEL.md).

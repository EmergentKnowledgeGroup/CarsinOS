# Releases and availability

## Current status

CarsinOS is currently published as a **source-first** public repository. There
is no packaged GitHub Release for the current public `main` source tree.

The public path today is to clone the source and use the documented local
development modes:

~~~powershell
git clone https://github.com/EmergentKnowledgeGroup/CarsinOS.git
cd CarsinOS
~~~

Then follow [Getting started](GETTING_STARTED.md) for the gateway token,
browser/Vite, Tauri-debug, and validation boundaries.

## A note on historical history

A historical `v0.1.0-beta` Git tag exists in legacy repository history, but
it is not an ancestor of the current public `main` tree and does not have a
current GitHub Release page. Do not treat that tag as a packaged release of the
current public source.

This page intentionally describes current public availability rather than
turning a disconnected historical tag into an implied installer promise.

## What not to assume

- A local source build is not the same thing as a vendor-signed installer.
- A screenshot or third-party download is not an official release channel.
- Source development on a platform is not automatically a platform-support
  promise.
- Browser/Vite and debug Tauri workflows do not prove the release-managed
  desktop authority/runtime path.
- A future release should be evaluated from its exact GitHub release page, not
  from an unofficial mirror or reposted archive.

## What an official release will provide

When CarsinOS publishes an official release, this page will point to the exact
GitHub release and include the information needed to evaluate it responsibly:

- supported platform statement;
- artifact names and integrity/signing information;
- upgrade and compatibility notes;
- known boundaries or limitations; and
- rollback or recovery guidance where relevant.

Until then, the source and its tests are the public artifact. See
[Engineering](ENGINEERING.md) for the validation map and
[Security model](SECURITY_MODEL.md) for operational boundaries.

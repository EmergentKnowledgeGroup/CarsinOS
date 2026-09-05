# Glass Office: design and implementation

The real application now uses the recovered Glass Office design: a wide branded elevator, Office landing canvas, editorial briefing, decision cards, reef and chatter, and a consistent Carbon/Porcelain theme across existing rooms. The six-step onboarding wizard and guided tour remain available.

The Office connects to existing ExecAss controllers for delegation, decisions, receipt inspection, conversation, and freeze/resume. The Window renders observed presence and safe Agent Mail notes. Trenches and Basement rooms retain their existing boards, schedules, staff, policy, setup, and operational controls. No prototype data or simulated success handlers are used in the application.

## Running and verification boundaries

[Getting started](GETTING_STARTED.md) explains browser and desktop modes. Browser development can connect to ordinary gateway surfaces, but signed ExecAss owner actions require the native desktop authority path. A configured provider and gateway are necessary for live work.

[Repository screenshots](assets/screenshots/README.md) show the actual application against isolated test fixtures. Frontend/browser regression verifies UI integration; it does not establish that an arbitrary provider account or installed runtime is configured correctly.

## Original design and PR #120

[The standalone prototype](../demos/glass-office/README.md) remains available as the historical design reference. Its illustrative activity and local interactions are separate from the application.

Before this visual integration, frontend source at public baseline `9d8e25ce323fd2b8f0d9ae86e405321d1f730edf` matched the archived PR #120 endpoint `4a6307bd57d275c176bb86d5f5155fab41b6f5c6` when line endings were ignored. The repository reset preserved that frontend; it had retained older shell presentation around the implemented features. This change integrates the confirmed design into that application rather than claiming the prototype itself was production code.

# Local-first processing, with bring-your-own-key cloud opt-in

VoiceDrop's core promise is privacy and offline capability, so both STT (whisper.cpp) and the Cleanup Pass (llama.cpp locally, or Apple's on-device Foundation Models framework as a Mac-only alternative) run fully on-device by default and work with no network. We considered a fully-local-only design (no cloud path at all) and a cloud-first design with local as an opt-in privacy mode, but rejected both: fully-local-only caps cleanup quality at what fits on consumer hardware with no escape hatch, while cloud-first would make network access the default for a privacy-positioned app.

Decided: cloud cleanup is available as an explicit opt-in, using the user's own API key (e.g. Anthropic) entered in preferences. VoiceDrop never ships its own backend or proxies API calls — no billing, hosting, or key-management liability on our side.

## Consequences

- No server infrastructure to build or run; VoiceDrop stays a pure client app on every platform.
- Cloud cleanup quality/availability is entirely dependent on the user's own account/key — no ability to offer a polished "just works" cloud tier without the user configuring one first.

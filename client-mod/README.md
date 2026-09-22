# Garnet Client

The Fabric mod half of Garnet. It is small on purpose: everything a Garnet
server needs from the client that vanilla cannot do.

- **Server mod sync** – answers the server's `garnet:mods` handshake with the
  list of installed mods, so the server can let you in or tell the launcher
  what to install.
- **Proximity voice chat** – the server hands out a UDP port and a one-time
  secret on `garnet:voice`; the mod streams Opus audio to the relay and plays
  everyone else back through OpenAL at their world position. Blocks between
  you and the speaker muffle the voice, and caves and rooms add reverb (EFX
  when the driver has it).
- **HUD** – a small indicator bottom-left showing voice state and who is
  talking.

Default keys: `V` push to talk, `N` whisper (short range), `B` mute.

## Building

```
./gradlew build
```

Needs a Java 25 runtime to run Gradle; the toolchain plugin fetches a JDK
for compiling if none is installed. The jar lands in `build/libs/` and bundles
`opus4j`, so nothing else has to be installed alongside it.

The launcher installs this mod automatically when you join a Garnet server. To
use it by hand, drop the jar into `mods/` of a Fabric 26.3 instance together
with Fabric API.

## Protocol

The UDP framing is documented in the server's `garnet-voice` crate; the client
mirrors it in `VoiceClient`. Audio is 48 kHz mono Opus, 20 ms frames.

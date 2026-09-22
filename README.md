<p align="center">
  <img src="crates/garnet-launcher-gui/ui/stone.png" alt="Garnet" width="140">
</p>

<h1 align="center">Garnet Launcher</h1>

<p align="center">A Minecraft launcher that joins servers the way it should work: pick a server, and the mods it needs install themselves.</p>

---

Garnet is a launcher for Minecraft: Java Edition with instances, Fabric-based mod loading, Modrinth built in, shader pack management, proximity voice chat (with the Garnet client mod) and one-click joining of [Garnet servers](https://github.com/garnet-mc/garnet-server). It downloads the game from Mojang, the Java runtime Mojang publishes for each version, and signs in with your Microsoft account. It never modifies the game jar and has no offline-account mode.

Windows, macOS (Intel and Apple Silicon) and Linux.

## Highlights

- **Join a server, get its mods** – Garnet servers advertise the client mods they need. Type the address, the launcher creates an instance with the right Minecraft version and Fabric, installs the mods (verified by hash) and launches straight into the server.
- **Instances** – separate folders for mods, saves and options; shared downloads.
- **Garnet Loader** – Fabric plus Garnet's client mod (voice chat, server mod sync, shader helper), so every Fabric mod keeps working.
- **Modrinth built in** – search and install mods, shader packs and resource packs with dependencies.
- **Automatic Java** – the exact runtime Mojang ships for the version, kept apart from anything on your system.
- **A CLI** – everything is scriptable with `garnet-launcher`.

## Building

```
cargo build --release
```

- `target/release/garnet-launcher-app` – the desktop app (Tauri 2; on Linux you need `libwebkit2gtk-4.1-dev` and `librsvg2-dev`)
- `target/release/garnet-launcher` – the command-line tool

Installers are built with `cargo tauri build` inside `crates/garnet-launcher-gui` (needs `cargo install tauri-cli`).

## Microsoft sign-in and the client id

Minecraft accounts are Microsoft accounts. Every third-party launcher has to register its own application with Microsoft and have Mojang approve it for the Minecraft API, and the resulting **client id** is what the launcher sends when you sign in. It is not a secret, but it identifies the launcher, so each build of Garnet you distribute needs one.

To get one:

1. Create an application in the [Azure portal](https://portal.azure.com) (Microsoft Entra ID → App registrations → New). Account type: *Personal Microsoft accounts only*. Add the *Mobile and desktop applications* platform and enable *Allow public client flows*.
2. Ask Mojang to allow it for the Minecraft API using [this form](https://aka.ms/mce-reviewappid).
3. Put the application id into the launcher: Settings → *Microsoft application (client) id*, or `garnet-launcher set-client-id <id>`.

The launcher then uses the device-code flow: it shows a short code, you enter it on Microsoft's page, done.

## Command line

```
garnet-launcher login
garnet-launcher versions
garnet-launcher instance create survival --fabric
garnet-launcher mod search survival sodium
garnet-launcher mod install survival sodium
garnet-launcher instance launch survival
garnet-launcher ping play.example.com
garnet-launcher sync play.example.com          # prepare an instance for a server
garnet-launcher join play.example.com          # ...and launch into it
```

Data lives in `%APPDATA%\garnet`, `~/Library/Application Support/garnet` or `~/.local/share/garnet` (override with `GARNET_HOME`).

## Layout

| Crate | What it does |
|---|---|
| `garnet-launcher-core` | downloads, Mojang metadata, assets, Java, Microsoft auth, instances, Fabric, Modrinth, launching, Garnet server sync |
| `garnet-launcher-cli` | the command-line tool |
| `garnet-launcher-gui` | the Tauri desktop app (`ui/` is plain HTML, CSS and JavaScript) |

## License

MIT.

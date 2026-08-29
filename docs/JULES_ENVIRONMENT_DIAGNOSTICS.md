# Jules Environment Diagnostics

- **Diagnose-Zeitstempel:** `2026-08-29T00:24:07Z`
- **Gesamt-Status:** `System bereit für Cargo/Tauri-Builds`

---

## 1. System & Hardware-Spezifikationen

| Parameter | Wert / Detail |
| --- | --- |
| **OS & Distribution** | Ubuntu 24.04.4 LTS (Noble Numbat) |
| **Kernel-Version** | `6.8.0` |
| **CPU-Architektur** | `x86_64` |
| **CPU-Kerne** | `4` Kerne (Intel(R) Xeon(R) Processor @ 2.30GHz) |
| **Arbeitsspeicher (RAM)** | `7.8 GiB` Total (466 MiB genutzt, 217 MiB frei, 7.3 GiB verfügbar) |
| **Speicherplatz Workspace (`.`)** | `98 GiB` Total (11 GiB genutzt, 83 GiB frei - 12% Auslastung) |

---

## 2. Privilegien & Ausführungskontext

| Eigenschaft | Wert |
| --- | --- |
| **Aktueller Benutzer (`whoami`)** | `jules` |
| **User ID & Group ID (`id`)** | `uid=1001(jules) gid=1001(jules)` |
| **Zugehörige Gruppen** | `1001(jules), 27(sudo), 103(docker)` |
| **Passwordless-Sudo** | `✅ Ja` (Passwordless Sudo aktiv) |
| **Shell-Verzeichnispfad (`pwd`)** | `/app` |

---

## 3. Sprach-Toolchains & Runtimes

| Toolchain / Runtime | Status | Version | Details |
| --- | --- | --- | --- |
| **Rust (`rustc`)** | ✅ Verfügbar | `1.98.0` | 88d9e12ae (2026-08-18) |
| **Cargo (`cargo`)** | ✅ Verfügbar | `1.98.0` | 797e8a9bc (2026-08-05) |
| **Rustup (`rustup`)** | ✅ Verfügbar | `1.29.0` | Active Toolchain: `stable-x86_64-unknown-linux-gnu` |
| **Node.js (`node`)** | ✅ Verfügbar | `v22.22.1` | JS Runtime |
| **npm** | ✅ Verfügbar | `11.11.0` | Node Package Manager |
| **pnpm** | ✅ Verfügbar | `10.30.3` | Fast Package Manager |
| **yarn** | ✅ Verfügbar | `1.22.22` | Yarn Package Manager |
| **bun** | ✅ Verfügbar | `1.2.14` | Bun JS/TS Runtime |
| **Python (`python3`)** | ✅ Verfügbar | `3.12.13` | CPython (via pyenv) |
| **pip (`pip3`)** | ✅ Verfügbar | `26.0.1` | Python Package Installer |
| **GCC (`gcc`)** | ✅ Verfügbar | `13.3.0` | Ubuntu 13.3.0-6ubuntu2~24.04.1 |
| **Clang (`clang`)** | ✅ Verfügbar | `18.1.3` | Ubuntu Clang Version 18.1.3 |
| **Make (`make`)** | ✅ Verfügbar | `4.3` | GNU Make |

### Installierte Rust-Komponenten (`rustup component list --installed`)
- `cargo-x86_64-unknown-linux-gnu`
- `clippy-x86_64-unknown-linux-gnu`
- `rust-docs-x86_64-unknown-linux-gnu`
- `rust-std-x86_64-unknown-linux-gnu`
- `rustc-x86_64-unknown-linux-gnu`
- `rustfmt-x86_64-unknown-linux-gnu`

---

## 4. CLI-Tools & Utilities

| CLI Tool | Status | Version |
| --- | --- | --- |
| **`just`** | ✅ Verfügbar | `just 1.58.0` |
| **`cargo-audit`** | ✅ Verfügbar | `cargo-audit 0.22.2` |
| **`git`** | ✅ Verfügbar | `git version 2.53.0` |
| **`curl`** | ✅ Verfügbar | `curl 8.5.0` |
| **`jq`** | ✅ Verfügbar | `jq-1.7` |
| **`pkg-config`** | ✅ Verfügbar | `1.8.1` |
| **`docker`** | ✅ Verfügbar | `Docker version 29.2.1, build a5c7197` |

---

## 5. System-Bibliotheken (GUI / Tauri / SSL)

| Paket | Status | Version / Anmerkungen |
| --- | --- | --- |
| **`libssl-dev`** | ✅ Verfügbar | `3.0.13-0ubuntu3.15` |
| **`libgtk-3-dev`** | ✅ Verfügbar | `3.24.41-4ubuntu1.3` |
| **`libwebkit2gtk-4.1-dev`** | ✅ Verfügbar | `2.52.3-0ubuntu0.24.04.1` |
| **`libwebkit2gtk-4.0-dev`** | ⚠️ Nicht vorhanden | Ersetzt durch `libwebkit2gtk-4.1-dev` auf Ubuntu 24.04 |
| **`libayatana-appindicator3-dev`** | ✅ Verfügbar | `0.5.93-1build3` |
| **`librsvg2-dev`** | ✅ Verfügbar | `2.58.0+dfsg-1build1` |

---

## 6. Umgebungsvariablen & Jules-Spezifika

### Relevant Environment Variables
- **`PATH`**: `/home/jules/.pyenv/versions/3.12.13/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`
- **`CARGO_HOME`**: Standard (`/home/jules/.cargo`)
- **`RUSTUP_HOME`**: Standard (`/home/jules/.rustup`)

### Output von `/opt/environment_summary.sh`

```text
-------------------------------------
Environment check starting...

--------- Python ---------
✅  python3: Python 3.12.13
✅  python: Python 3.12.13
✅  pip: pip 26.0.1 from /home/jules/.pyenv/versions/3.12.13/lib/python3.12/site-packages/pip (python 3.12)
✅  pipx: 1.4.3
✅  poetry: Poetry (version 2.3.2)
✅  uv: uv 0.10.8
✅  black: black, 26.1.0 (compiled: yes)
Python (CPython) 3.12.3
✅  mypy: mypy 1.19.1 (compiled: yes)
✅  pytest: pytest 9.0.2
✅  ruff: ruff 0.15.5
✅  pyenv: available
  system
  3.10.20
* 3.12.13 (set by /home/jules/.pyenv/version)

--------- NodeJS ---------
✅  node: v22.22.1
❌  nvm: not found
✅  npm: 11.11.0
✅  yarn: 1.22.22
✅  pnpm: 10.30.3
✅  eslint: v10.0.2
✅  prettier: 3.8.1
✅  chromedriver: ChromeDriver 146.0.7680.66

--------- Java ---------
✅  java: openjdk version "21.0.10" 2026-01-20
OpenJDK Runtime Environment (build 21.0.10+7-Ubuntu-124.04)
OpenJDK 64-Bit Server VM (build 21.0.10+7-Ubuntu-124.04, mixed mode, sharing)
✅  mvn: Apache Maven 3.9.12
✅  gradle: Gradle 8.8

--------- Go ---------
✅  go: go version go1.24.3 linux/amd64

--------- Rust ---------
✅  rustc: rustc 1.98.0 (88d9e12ae 2026-08-18)
✅  cargo: cargo 1.98.0 (797e8a9bc 2026-08-05)

--------- Bun ---------
✅  bun: 1.2.14

--------- C/C++ Compilers ---------
✅  clang: Ubuntu clang version 18.1.3 (1ubuntu1)
✅  gcc: gcc (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0
✅  cmake: cmake version 3.28.3
✅  ninja: 1.11.1
✅  conan: Conan version 2.26.2

--------- Android ---------
Installed packages: build-tools;33.0.2, 34.0.0, 35.0.0, platform-tools 36.0.2, platforms;android-34, android-35

--------- Flutter ---------
✅  flutter: Flutter 3.41.2 • channel stable

--------- PHP ---------
✅  php: PHP 8.3.6 (cli)
✅  composer: Composer version 2.9.5 2026-01-29

--------- Ruby ---------
✅  ruby: ruby 3.2.3 (2024-01-18 revision 52bb2ac0a6) [x86_64-linux-gnu]
✅  gem: 3.4.20
✅  bundle: 4.0.7

--------- .NET ---------
✅  dotnet: 8.0.124, 10.0.103

--------- Docker ---------
✅  docker: Docker version 29.2.1, build a5c7197
✅  docker: Docker Compose version v5.1.0

--------- PlayWright ---------
✅  playwright: Version 1.58.0

--------- Other Utilities ---------
✅  awk: GNU Awk 5.2.1
✅  curl: curl 8.5.0
✅  git: git version 2.53.0
✅  grep: grep (GNU grep) 3.11
✅  gzip: gzip 1.12
✅  jq: jq-1.7
✅  make: GNU Make 4.3
✅  rg: ripgrep 14.1.0
✅  sed: sed (GNU sed) 4.9
✅  tar: tar (GNU tar) 1.35
✅  tmux: tmux 3.4
✅  yq: yq 0.0.0

-------------------------------------
Environment check complete.
```

---

## 7. Netzwerk-Konnektivität

| Dienst / Endpoint | Status | Anmerkung |
| --- | --- | --- |
| **`https://crates.io`** | ✅ Erreichbar | Antwortet via CDN (HTTP 403 / Fastly Edge Gate) |
| **`https://github.com`** | ✅ Erreichbar | HTTP 200 OK |

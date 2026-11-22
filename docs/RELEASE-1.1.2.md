## Snipt 1.1.2

**Fixes & Polish**
- **Zero idle frame burn on 90/120 Hz screens**: removed the infinite CSS drift animation on the ambient Liquid Glass background orbs (`src/index.css`) so reading a message renders 0 FPS while idle instead of ticking compositor frames continuously.
- **Docs**: added Apple multiplatform architecture plan (`docs/APPLE-ARCHITECTURE.md`, paused pending Apple hardware) and updated `README.md` to reflect zero-idle-frame Liquid Glass and minimal Android permissions (no internet or contacts permission).

**Install:** download `Snipt-arm64.apk` on your phone, or on Linux run
`curl -fsSL https://raw.githubusercontent.com/Tariq-Anjum/snipt/main/install.sh | bash`.

# Knowledge utama Tala

Dokumen ini menjadi rujukan utama untuk keputusan produk dan teknis Tala. Bagian **Keputusan aktif** berlaku jika isinya berbeda dengan brief awal. Brief awal disimpan utuh sebagai arsip alasan dan ide produk.

## Keputusan aktif

- **Produk:** Tala adalah workspace terminal lokal untuk membuka folder proyek, menjalankan shell atau agen koding di tab, dan bekerja dengan Git. Identitas produk: **Tala · Find your flow.**
- **Platform:** Windows x64/ARM64, Linux x64/ARM64, dan macOS Intel/Apple Silicon adalah target sejak awal. Tidak ada platform yang dianggap siap rilis hanya karena kode berhasil dikompilasi.
- **Runtime:** Rust dengan Ratatui dan Crossterm untuk TUI, `portable-pty` untuk sesi terminal, filesystem standar Rust, `arboard` untuk clipboard, Git CLI native, dan TOML untuk konfigurasi. Tala tidak memerlukan Tauri, Electron, WebView, Node.js, Docker, atau WSL. WSL adalah profil shell opsional di Windows dan bisa dipakai sebagai lingkungan tes Linux.
- **Shell:** PowerShell, CMD, Git Bash, dan WSL di Windows; Bash, Zsh, dan Fish di Linux/macOS. Deteksi profil dari executable yang tersedia. Sediakan jalur keyboard dan mouse tanpa mengharuskan mouse untuk memakai fitur utama.
- **Agen:** Tala menjalankan CLI interaktif `codex`, `agy`, `omp`, dan `pi` di direktori kerja workspace. Masing-masing CLI mengelola login, model, izin, dan sesi. Dukungan platform Tala tidak menjamin sebuah CLI eksternal tersedia di platform yang sama.
- **Git:** Gunakan Git CLI yang terpasang untuk status, diff, stage, unstage, commit, fetch, pull fast-forward, dan push. Autentikasi interaktif bisa dilakukan di tab terminal.

## Kriteria rilis

1. [Native CI](.github/workflows/ci.yml) menjalankan build dan tes pada keenam kombinasi OS dan arsitektur.
2. Checklist manual meliputi terminal, clipboard, penjelajah file, Git, dan launcher agen di Windows, Linux, dan macOS. CLI vendor yang tidak tersedia dicatat, bukan dinyatakan lulus.
3. [Workflow Release](.github/workflows/release.yml) hanya menerbitkan arsip jika checklist dan matriks lulus, tag menunjuk commit yang diuji, dan versi Cargo, lockfile, serta [changelog](CHANGELOG.md) cocok. Build Windows atau tes WSL x64 saja tidak memenuhi kriteria rilis.
4. GitHub Releases adalah tujuan distribusi arsip. Winget, Homebrew, skrip Linux, dan crates.io tetap rencana kecuali sudah benar-benar diterbitkan.

## Membaca brief awal

Prioritas produk masih mengikuti brief: terminal, workspace, launcher, dan Git. Split pane, editor, CRUD file, pemulihan layout, dan fitur lain dalam roadmap awal adalah ide untuk dievaluasi, bukan fitur yang dianggap sudah ada atau komitmen rilis. Usulan Tauri/React dan pendekatan Windows-first di arsip sudah digantikan oleh keputusan di atas. Angka performa di arsip adalah target awal, bukan hasil pengukuran.

Untuk cara pakai, lihat [README Indonesia](README.id.md) atau [English README](README.md). [Changelog](CHANGELOG.md) mencatat perubahan versi; hasil CI menunjukkan status verifikasi terkini.

## Arsip brief awal

<details>
<summary>Buka salinan asli dari awal chat</summary>

````text

saya pengen buat terminal yang bisa buka folder, terminal yang bisa connect ke chatgpt, antigravity, omp, pi. git

Worked for 23s
Yang kamu inginkan sebenarnya bukan sekadar terminal emulator, melainkan Agentic Terminal Workspace — perpaduan antara terminal manager, file explorer, lightweight code editor, Git client, dan AI coding agent launcher.

Dari pengalaman kamu menggunakan Herdr, Orca, T3 Code, dan Terasx, saya akan mengambil konsep ringan seperti Herdr, tetapi dengan kenyamanan pengelolaan file seperti VS Code, tanpa membawa seluruh kompleksitas IDE.

1. Konsep UI/UX

Konsep desktop workspace: file explorer di kiri, editor di atas, multi-agent terminal di bawah, dan Git terintegrasi. Seluruh panel dapat dibuat collapsible dan resizable.
2. Fitur utama
Module

Fitur

File Explorer

Open folder, create, rename, delete, move, copy, paste

Terminal

Multiple tabs, split pane, search, clipboard, PowerShell, CMD, WSL, SSH

AI Agents

Codex, Antigravity, OMP, Pi, custom agents

Code Editor

Syntax highlighting, tabs, edit, save, search

Git

Status, branch, diff, stage, commit, pull, push, stash

Workspace

Multiple projects, recent folders, restore layout

Command Palette

Akses seluruh fitur melalui shortcut

Customization

Theme, font, keybindings, terminal profiles

Agent Manager

Status running, stopped, session history, resume

Editor bersifat opsional. Kamu bisa menyembunyikannya dan menggunakan terminal secara fullscreen seperti Herdr.

3. Tech stack
Saya merekomendasikan Tauri + Rust + React, bukan Electron, karena prioritas kamu adalah aplikasi yang ringan.

Layer

Technology

Desktop Framework

Tauri v2

Backend

Rust

Frontend

React + TypeScript + Vite

Styling

Tailwind CSS + shadcn/ui

Terminal Renderer

xterm.js

Terminal Engine

portable-pty / Windows ConPTY

Code Editor

CodeMirror 6, lazy-loaded

State Management

Zustand

File Explorer

Rust filesystem + notify

Git Engine

Native Git CLI

Local Storage

SQLite

Layout Manager

React Resizable Panels

File Search

ripgrep

Tauri menggunakan WebView2 bawaan Windows, sementara portable-pty menyediakan abstraksi pseudo-terminal lintas platform. 

Tauri
+1

4. System architecture

Tidak perlu NestJS, Express, local HTTP server, maupun database server. Frontend berkomunikasi langsung dengan Rust melalui Tauri IPC.

5. Integrasi AI Agents
Kamu tidak perlu membuat AI agent sendiri. Aplikasi cukup menjadi host untuk agent yang sudah tersedia.

OpenAI Codex

Login menggunakan akun ChatGPT yang didukung
codex


Google Antigravity

Google AI coding agent
agy

⌥
Oh My Pi

Multi-provider coding agent
omp

π
Pi

Extensible coding agent
pi

Custom Agent

Executable dan arguments dapat dikonfigurasi
custom

Codex dan Antigravity sudah menyediakan CLI interaktif. OMP juga memiliki opsi integrasi RPC dan ACP untuk pengembangan integrasi yang lebih mendalam. 

ChatGPT Learn
+3

Untuk tahap awal, gunakan PTY untuk menjalankan seluruh agent. Biarkan autentikasi, pemilihan model, dan session management ditangani oleh masing-masing CLI.

6. Keyboard shortcuts
Bagian ini penting supaya masalah UX yang kamu alami sebelumnya tidak terulang.

Shortcut

Action

Ctrl + Shift + C

Copy terminal

Ctrl + Shift + V

Paste terminal

Ctrl + C

Interrupt process (SIGINT atau mekanisme setara)

Ctrl + Shift + T

New terminal

Ctrl + Shift + O

Open folder

Ctrl + B

Toggle sidebar

Ctrl + P

Quick open file

Ctrl + Shift + P

Command palette

Ctrl + Shift + G

Git panel

Ctrl + Shift + A

Launch AI agent

F2

Rename file/folder

Ctrl + S

Save file

7. Development roadmap
Development checklist
0/25
Phase 1 — Terminal Core


Initialize Tauri + React + Rust

PTY engine dan xterm.js

Terminal tabs dan split panes

Copy/paste, resize, keyboard shortcuts

PowerShell, CMD, Git Bash, WSL
Phase 2 — Workspace


Open folder dan file explorer

File/folder CRUD

File watcher

Lightweight editor

Workspace persistence
Phase 3 — AI Agent Integration


CLI auto-detection

Codex, Antigravity, OMP, Pi launcher

Custom agent profiles

Agent process management

Session resume sesuai dukungan CLI
Phase 4 — Git Integration


Git status dan branch

Diff viewer

Stage, unstage, commit

Pull, push, fetch

Git worktree management
Phase 5 — Optimization


Lazy loading

Terminal rendering optimization

Memory profiling

Windows installer

Theme dan keybindings
8. Referensi open-source
Ada beberapa repository yang bisa kamu pelajari sebelum membangun dari nol.

Repository

Yang bisa dipelajari

Termigo

Tauri, Rust, terminal-first development environment

WezTerm

Terminal engine dan PTY

xterm.js

Terminal rendering

TerminalApp

Windows multi-agent workspace

Oh My Pi

Agent integration

9. Product direction
Saya akan menetapkan positioning produk ini sebagai:

A lightweight, terminal-first workspace for multi-agent software development.

Target desain awal (belum merupakan hasil benchmark):

Idle RAM di bawah 150 MB.

Startup di bawah 2 detik.

Local-first, tanpa backend server.

Agent-agnostic dan provider-agnostic.

Windows-first, dengan kemungkinan mendukung Linux dan macOS.

Tidak membutuhkan subscription tambahan untuk aplikasinya sendiri.

Dengan laptop HP ProBook RAM 16 GB yang kamu gunakan, efisiensi resource perlu menjadi prioritas, terutama ketika menjalankan beberapa agent bersamaan.

Keputusan saya adalah memprioritaskan terminal, workspace, dan agent launcher daripada membangun IDE lengkap. Kompatibilitas masing-masing CLI dan performa aktual masih perlu diuji langsung di Windows.

Herdr simplicity + VS Code file management + multi-agent orchestration. Itu arah produk yang menurut saya paling sesuai dengan kebutuhan kamu.





````

</details>

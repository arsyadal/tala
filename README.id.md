# Tala

**Find your flow.**

[English](README.md) · [Bahasa Indonesia](README.id.md) · [Knowledge proyek](PROJECT.md)

[![Status Native CI di main](https://github.com/arsyadal/tala/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/arsyadal/tala/actions/workflows/ci.yml)

Tala adalah workspace terminal lokal untuk menjelajahi folder proyek, menjalankan shell dan agen koding di beberapa tab, serta bekerja dengan Git. Tala berjalan di terminal tanpa browser, server, atau runtime Node.js. WSL adalah pilihan profil shell di Windows, bukan syarat instalasi.

**Status rilis:** Tala masih dalam pengembangan. Tabel di bawah menunjukkan platform yang ditargetkan, bukan jaminan bahwa setiap build siap dirilis. Lihat hasil terbaru di Native CI. Rilis juga memerlukan pengujian manual di Windows, macOS, dan Linux.

## Mulai cepat

Pasang [Rust 1.88 atau lebih baru](https://rustup.rs/) dan linker native untuk platform yang kamu pakai. Jalankan Tala dari terminal interaktif. Pasang [Git](https://git-scm.com/downloads) untuk mengunduh repositori dan memakai panel Git.

```sh
git clone https://github.com/arsyadal/tala.git
cd tala
cargo run --locked -- .
```

Tekan `F5` untuk membuka shell. Untuk berganti proyek, tekan `F2`, masuk ke folder tujuan, lalu tekan `o` untuk menjadikannya workspace. Sesi baru dimulai di folder itu; sesi lama tetap memakai direktori kerjanya masing-masing.

Untuk memasang perintah `tala` dari source lokal:

```sh
cargo install --path . --locked
tala .
```

Berikan path folder sebagai argumen. Gunakan tanda kutip jika path mengandung spasi. Tanpa argumen, Tala membuka direktori saat ini.

## Platform target

| Platform | Arsitektur | Shell yang dideteksi Tala |
| --- | --- | --- |
| Windows dengan ConPTY | x64, ARM64 | PowerShell, Command Prompt, Git Bash, WSL |
| macOS | Intel, Apple Silicon | Bash, Zsh, Fish |
| Linux | x64, ARM64 | Bash, Zsh, Fish |

Shell harus tersedia di perangkat. Di Linux, clipboard sistem memerlukan sesi X11 atau compositor Wayland dengan dukungan data control.

## Kontrol

| Tombol | Aksi |
| --- | --- |
| `F1` | Buka bantuan. |
| `F2` | Jelajahi file. Gunakan `↑` / `↓` untuk memilih, `Enter` untuk membuka, `Backspace` untuk naik, dan `o` untuk memakai folder saat ini. |
| `F3` | Buka launcher. Tekan `Enter` untuk menjalankan profil atau `r` untuk mendeteksi ulang. |
| `F4` | Buka Git. `Enter` menampilkan diff, `d` menampilkan diff staged, dan `r` memperbarui status. |
| `F5` / `F6` / `F7` | Buka terminal baru, pindah ke tab berikutnya, atau tutup tab aktif. |
| `F8` / `F9` | Salin seleksi terminal atau teks pratinjau, atau tempel dari clipboard. |
| `F10` | Keluar dari Tala. |
| `Tab` / `Esc` di sidebar | Pindahkan fokus ke terminal atau sembunyikan sidebar. |
| `Ctrl+C` di sesi terminal | Interupsi program yang sedang berjalan. |
| `Shift+Page Up` / `Shift+Page Down` | Gulir riwayat terminal. |

Klik untuk berpindah tab atau memakai panel. Seret kursor di atas teks terminal sebelum menyalin. Pratinjau file hanya bisa dibaca dan mendukung teks UTF-8 hingga 1 MiB.

### Agen koding

Pasang CLI dari penyedia sebelum menjalankannya di Tala.

| Agen | Executable |
| --- | --- |
| Codex (ChatGPT) | `codex` |
| Antigravity | `agy` |
| Oh My Pi (OMP) | `omp` |
| Pi | `pi` |

Jalankan `tala --list-profiles` untuk melihat hasil deteksi. Jika CLI tidak ditemukan, periksa apakah executable ada di `PATH`. Buka ulang Tala jika lingkungan terminal berubah setelah Tala dijalankan.

Agen berjalan dengan akun sistem operasi dan punya akses yang sama seperti saat dijalankan dari terminal biasa. Tala tidak mengisolasi agen atau menyimpan kredensialnya. Login, pilihan model, dan dukungan platform bergantung pada masing-masing CLI.

### Git

Gunakan `s` untuk stage, `u` untuk unstage, dan `c` untuk commit perubahan yang sudah di-stage. Gunakan `f` untuk fetch, `p` untuk pull dengan `--ff-only`, dan `P` huruf besar untuk push. Panel ini juga berfungsi ketika workspace berada di subfolder repositori. Jika Git membutuhkan autentikasi interaktif, jalankan Git dari tab terminal.

## Konfigurasi

Jalankan `tala --config-path` untuk melihat lokasi persisnya. Buat `config.toml` di lokasi tersebut hanya jika ingin mengubah pengaturan bawaan:

```toml
# Opsional. Gunakan ID shell dari `tala --list-profiles`.
# default_shell = "zsh"
scrollback = 2000
mouse = true
```

Tanpa `default_shell`, Tala memakai `$SHELL` jika cocok dengan profil yang terdeteksi, atau memakai shell pertama yang tersedia. `scrollback` menerima `0` sampai `20000` baris per sesi. Atur `mouse = false` agar terminal yang menangani input mouse. Kunci yang tidak dikenal akan memunculkan error saat Tala dibuka.

## Pengembangan dan rilis

```sh
cargo build --locked
cargo test --locked
```

[Native CI](.github/workflows/ci.yml) menjalankan build dan tes untuk keenam kombinasi OS dan arsitektur pada setiap pull request dan push ke `main`. CI juga menguji salin dan tempel clipboard desktop memakai display X11 virtual di Linux. Tes clipboard dilewati secara bawaan saat dijalankan lokal supaya tidak mengubah clipboard pengguna.

| Bagian | Source |
| --- | --- |
| Startup, input, dan state workspace | [`src/main.rs`](src/main.rs), [`src/app.rs`](src/app.rs) |
| UI terminal, sesi PTY, dan rendering VT | [`src/ui.rs`](src/ui.rs), [`src/terminal.rs`](src/terminal.rs), [`src/terminal_view.rs`](src/terminal_view.rs) |
| Operasi file dan perintah Git native | [`src/workspace.rs`](src/workspace.rs) |

Saat melaporkan bug, sertakan commit atau versi, OS dan arsitektur, emulator terminal, serta langkah untuk memunculkan bug. Jika masalah ada di launcher, sertakan versi CLI shell atau agen dan baris terkait dari `tala --list-profiles`. Hapus kredensial dan path pribadi dari log.

### Versi

[Changelog](CHANGELOG.md) mencatat perubahan di bagian `Unreleased` sampai versinya terbit. `0.1.0` adalah rencana rilis pertama, belum diterbitkan. Tala memakai [Semantic Versioning](https://semver.org/) untuk CLI, konfigurasi, dan perilaku yang didokumentasikan:

- Sebelum `1.0.0`, naikkan versi minor untuk fitur atau perubahan yang memutus kompatibilitas, dan patch untuk perbaikan yang kompatibel.
- Mulai `1.0.0`, naikkan major untuk perubahan yang memutus kompatibilitas, minor untuk fitur yang tetap kompatibel, atau patch untuk perbaikan yang tetap kompatibel.
- Gunakan tag `vMAJOR.MINOR.PATCH`. Untuk prarilis, pakai akhiran seperti `v0.1.0-rc.1`. Jangan pindahkan tag terbit atau ganti arsipnya; terbitkan versi baru.

Untuk menyiapkan rilis, atur `package.version` di `Cargo.toml` dan jalankan `cargo check` untuk memperbarui `Cargo.lock`. Pindahkan perubahan yang ikut dirilis dari `Unreleased` ke judul bertanggal seperti `## [0.1.0] - YYYY-MM-DD`, dengan tanggal rilis sebenarnya. Validasi kandidat dengan `python scripts/release_notes.py v0.1.0` (Python 3.11+). Workflow menolak versi yang tidak cocok dan memakai bagian changelog tersebut sebagai catatan rilis GitHub.

### Checklist rilis

Sebelum menjalankan [workflow Release](.github/workflows/release.yml), periksa hal berikut di Windows, macOS, dan Linux:

- [ ] Buka dua sesi terminal, pindah tab, ubah ukuran, jalankan dan interupsi perintah, lalu tutup kedua sesi.
- [ ] Salin dan tempel teks Unicode memakai clipboard sistem.
- [ ] Buka folder yang namanya mengandung spasi, lihat pratinjau file, lalu pastikan sesi baru dimulai di folder tersebut.
- [ ] Buka diff, lakukan stage dan unstage, buat commit, lalu uji fetch, pull, dan push dengan remote uji.
- [ ] Deteksi dan jalankan setiap CLI agen pada platform yang didukung penyedianya. Catat kombinasi yang tidak tersedia.

Jalankan workflow untuk tag yang sudah ada dan mengarah ke commit yang dipilih. Workflow hanya menerbitkan enam arsip ke [GitHub Releases](https://github.com/arsyadal/tala/releases) setelah checklist dikonfirmasi dan semua job native lulus.

Homebrew, winget, skrip instalasi Linux, dan paket crates.io masih direncanakan. Gunakan instruksi dari source sampai arsip rilis tersedia. Repositori ini belum menetapkan lisensi.

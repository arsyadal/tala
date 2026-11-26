# Tala

**Find your flow.** Tala adalah workspace terminal lokal untuk membuka folder, menjalankan shell dan coding agent, serta melihat dan mengelola perubahan Git. Tala berjalan langsung di terminal dengan Rust, Ratatui, dan Crossterm. Tidak membutuhkan Node.js, WebView, Electron, Docker, atau WSL.

## Jalankan dari source

Butuh Rust 1.88 atau lebih baru, toolchain native untuk OS yang dipakai, dan terminal interaktif. Pasang Git untuk panel Source Control. Di Linux, clipboard memerlukan sesi X11 atau compositor Wayland dengan protokol data control (atau XWayland).

```sh
cargo run --locked -- .
cargo run --locked -- --list-profiles
cargo test --locked
```

Untuk memasang dari checkout lokal: `cargo install --path . --locked`. Jalankan `tala [folder]`; tanpa argumen, Tala memakai folder terminal saat ini.

| Platform | Target native | Profil shell yang dideteksi |
| --- | --- | --- |
| Windows 10/11 dengan ConPTY | x64, ARM64 | PowerShell, CMD, Git Bash, WSL |
| macOS | Intel, Apple Silicon | Bash, Zsh, Fish |
| Linux | x64, ARM64 | Bash, Zsh, Fish |

WSL dan shell tambahan hanya tersedia jika terpasang. Profil yang tidak ditemukan ditandai di launcher. Untuk Codex (ChatGPT), Antigravity, OMP, dan Pi, pasang CLI `codex`, `agy`, `omp`, atau `pi` pada `PATH`, lalu tekan `F3`. CLI berjalan di terminal interaktif dalam folder workspace. Login, model, izin, dan kompatibilitas platform tetap ditentukan oleh masing-masing CLI. Tala tidak menyimpan kredensial agen.

## Kontrol

| Tombol | Aksi |
| --- | --- |
| `F1` | Bantuan |
| `F2` | Files. Panah memilih, Enter masuk folder atau membuka pratinjau teks, Backspace naik, `o` menetapkan folder workspace. |
| `F3` | Launcher. Enter menjalankan profil, `r` mendeteksi ulang. |
| `F4` | Git. Enter membuka diff, `d` membuka diff staged, `s` stage, `u` unstage, `c` commit, `r` refresh. `f` fetch, `p` pull fast-forward saja, `P` push. |
| `F5` / `F6` / `F7` | Terminal baru / pindah tab / tutup tab |
| `F8` / `F9` | Salin seleksi atau pratinjau / tempel dari clipboard |
| `F10` | Keluar |
| `Tab` / `Esc` di sidebar | Fokus ke terminal / sembunyikan sidebar |
| `Ctrl+C` di terminal | Teruskan interrupt ke proses aktif |
| `Shift+PgUp` / `Shift+PgDn` | Gulir riwayat terminal |

Mouse dapat membuka folder, memilih tab, mengaktifkan tombol, menggulir, dan menyeleksi teks terminal dengan drag. Mengganti folder workspace hanya memengaruhi sesi baru. Untuk Git yang perlu autentikasi interaktif, gunakan Git langsung dari tab terminal; panel Git tidak membuka prompt kredensial.

## Konfigurasi

Opsional. `tala --config-path` menampilkan lokasi file `config.toml`. Contoh:

```toml
# Pakai ID shell dari tala --list-profiles, misalnya "zsh" pada macOS.
# default_shell = "zsh"
scrollback = 2000
mouse = true
```

Tanpa `default_shell`, Tala memilih `$SHELL` jika terdeteksi, atau profil shell pertama yang tersedia. `scrollback` boleh diatur ke 0 sampai 20.000 baris. Set `mouse = false` agar terminal host menangani mouse sendiri.

## Syarat rilis

Setiap PR menjalankan build dan pengujian native untuk Windows, macOS, Linux, serta x64 dan ARM64. Sebelum menjalankan workflow `Release` untuk tag yang tepat, periksa pada tiap OS:

- Buka dua shell PTY, pindah tab, resize terminal, jalankan perintah, interrupt, lalu tutup sesi.
- Salin dan tempel teks Unicode via clipboard native.
- Buka folder berspasi, lihat pratinjau, ganti workspace, lalu periksa folder awal sesi baru.
- Pada repo uji, periksa status dan diff, stage/unstage, commit, serta fetch/pull/push ke remote uji.
- Deteksi dan jalankan CLI agen yang mendukung platform tersebut. Catat agen yang memang tidak tersedia atau tidak didukung oleh vendornya.

Workflow menghasilkan arsip GitHub Releases setelah keenam build dan tes lulus. Paket winget dan Homebrew, installer `curl` untuk Linux, dan publikasi `cargo install tala` dari crates.io masih merupakan target distribusi, bukan kanal yang sudah tersedia.

# openfortivpn-tui

TUI sederhana untuk menjalankan `openfortivpn` dengan dukungan profile, OTP, dan debug log.

## Prasyarat

Pastikan `openfortivpn` sudah terpasang di sistem.

Contoh install:

```bash
sudo apt update && sudo apt install openfortivpn
```

Binary biasanya berada di salah satu path berikut:

- `/usr/bin/openfortivpn`
- `/usr/sbin/openfortivpn`

## Menjalankan Aplikasi

Mode normal:

```bash
cargo run
```

Atau jika binary sudah dibuild:

```bash
./target/debug/openfortivpn-tui
```

## Debug Log ke File

Secara default, output mentah dari proses `openfortivpn` tidak ditampilkan di panel log aplikasi.

Jika ingin menyimpan debug log ke file, jalankan dengan mode debug:

```bash
cargo run -- -d
```

Atau:

```bash
./target/debug/openfortivpn-tui -d
```

File log debug akan ditulis ke:

```bash
/tmp/openfortivpn-tui.log
```

Mode `-d` berguna untuk troubleshooting saat koneksi gagal, OTP bermasalah, atau ingin melihat output asli dari `openfortivpn`.

## Setup sudoers untuk openfortivpn

`openfortivpn` memerlukan privilege root. Cara paling aman adalah menambahkan rule `sudoers` khusus untuk binary `openfortivpn`, bukan memberi akses `NOPASSWD` untuk semua command.

Edit sudoers dengan:

```bash
sudo visudo
```

Lalu tambahkan salah satu rule berikut sesuai lokasi binary:

Jika binary ada di `/usr/bin/openfortivpn`:

```sudoers
<username> ALL=(root) NOPASSWD: /usr/bin/openfortivpn
```

Jika binary ada di `/usr/sbin/openfortivpn`:

```sudoers
<username> ALL=(root) NOPASSWD: /usr/sbin/openfortivpn
```

Ganti `<username>` dengan user Linux yang menjalankan aplikasi ini.

Contoh:

```sudoers
asepimam ALL=(root) NOPASSWD: /usr/bin/openfortivpn
```

## Catatan Penting

- Lebih aman memakai rule per-user dibanding `%sudo ALL=(ALL) NOPASSWD: ...`
- Jangan edit `/etc/sudoers` langsung tanpa `visudo`
- Jika tidak ingin `NOPASSWD`, aplikasi tetap bisa memakai `sudo -S` dan meminta password sudo
- Aplikasi akan mencoba mendeteksi apakah `openfortivpn` sudah terpasang dan apakah akses privilege tersedia

## Ringkasan Perilaku Log

- Mode normal: panel log hanya menampilkan status penting dari aplikasi
- Mode debug `-d`: output mentah `openfortivpn` ikut disimpan ke `/tmp/openfortivpn-tui.log`
- Token OTP tidak ditulis ke log UI

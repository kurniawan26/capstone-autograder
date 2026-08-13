# Capture Node — bukti visual proyek web siswa

Satu servis HTTP. Beri ia URL project siswa (atau ZIP source code-nya), ia
membalas screenshot WebP yang siap dikirim ke model multimodal.

Node ini **berhenti pada bukti**. Ia tidak menilai, tidak menyimpan skor, dan
tidak mengingat apa pun antar request. Penilaian, penyimpanan hasil, retry, dan
notifikasi dikerjakan platform otomasi yang memanggilnya — n8n dan sejenisnya.

```
n8n  ──POST /v1/capture──▶  node ini  ──screenshot WebP──▶  n8n  ──▶  LLM  ──▶  skor
```

## Struktur

| Direktori         | Isi                                                        |
| ----------------- | ---------------------------------------------------------- |
| `docker-sandbox/` | Seluruh node: HTTP API, build ephemeral, capture, optimizer |

`docker-compose.yml` menyediakan MinIO dan BuildKit.

## Prasyarat

Go 1.24 · Docker · Node 20 (hanya untuk mengunduh driver Playwright)

Encoder WebP-nya pure Go — `gen2brain/webp` memuat libwebp lewat purego, jadi
cgo tidak diperlukan.

## Setup

```bash
# 1. Infrastruktur — MinIO :9000/:9001, BuildKit
docker compose up -d

# 2. Driver Playwright + Chromium
./docker-sandbox/scripts/install-playwright-driver.sh

# 3. Railpack — builder zero-config, hanya dipakai jalur ZIP
gh release download v0.36.0 --repo railwayapp/railpack \
  --pattern 'railpack-*-x86_64-unknown-linux-musl.tar.gz' --dir /tmp/rp
tar -xzf /tmp/rp/railpack-*.tar.gz -C /tmp/rp
install -m 0755 /tmp/rp/railpack ~/.local/bin/railpack
```

## Menjalankan

```bash
cd docker-sandbox
BUILDKIT_HOST=docker-container://autograder-buildkit go run ./cmd/worker
```

Worker mendengarkan di `127.0.0.1:8090`. Alamatnya sengaja loopback — lihat
[Keamanan](#keamanan).

## API

### `POST /v1/capture`

```bash
curl -X POST http://127.0.0.1:8090/v1/capture \
  -H 'Content-Type: application/json' \
  -d '{
    "submission_id": "n8n-run-123",
    "live_url": "https://portfolio-budi.vercel.app"
  }'
```

| Field                 | Wajib | Keterangan                                                   |
| --------------------- | ----- | ------------------------------------------------------------ |
| `submission_id`       | ya    | Identitas milik pemanggil; jadi prefix key di object storage  |
| `live_url`            | *     | URL project siswa yang sudah online                          |
| `source_key`          | *     | Object key ZIP di bucket `submissions`                       |
| `scan_routes`         | —     | Default `false`; telusuri navigasi dan potret tiap halaman   |
| `max_routes`          | —     | Plafon jumlah halaman termasuk beranda, default 8            |
| `scan_budget_seconds` | —     | Plafon waktu pemindaian, default 90                          |
| `webp_quality`        | —     | 1–100, default 78                                            |
| `inline_images`       | —     | Default `true`; set `false` bila gambar cukup diambil dari storage |

\* Minimal salah satu dari `live_url` atau `source_key`. Bila keduanya ada,
`live_url` yang menang — deployment yang jalan lebih murah daripada membangun
ulang proyek yang sama.

Respons:

```json
{
  "submission_id": "n8n-run-123",
  "duration_ms": 8421,
  "build": {
    "strategy": "live_url",
    "live_url": "https://portfolio-budi.vercel.app",
    "notes": ["Captured directly from the submitted URL; no container was built."]
  },
  "screenshots": [
    {
      "name": "main",
      "width": 1440,
      "height": 1080,
      "bucket": "screenshots",
      "key": "n8n-run-123/main.png",
      "webp_key": "n8n-run-123/main.webp",
      "png_bytes": 1842301,
      "webp_bytes": 214880,
      "reduction_pct": 88.3,
      "downscaled": true,
      "webp_base64": "UklGRi..."
    }
  ]
}
```

`webp_base64` ada supaya node LLM di n8n bisa langsung memakainya tanpa
round-trip kedua ke object storage — yang bahkan mungkin tidak terjangkau dari
sana. Set `inline_images: false` bila body-nya terlalu besar dan MinIO memang
dapat dijangkau.

`name` menentukan label gambar pada prompt, dan `url` memberi tahu model halaman
mana yang sedang dilihatnya. `main` selalu ada; `interaction` hanya muncul bila
klik navigasi benar-benar menghasilkan tampilan berbeda.

### Memindai seluruh halaman

Brief yang meminta "halaman kontak" tidak bisa dinilai dari screenshot beranda.
`scan_routes: true` menelusuri navigasi proyek dan memotret setiap halamannya:

```bash
curl -X POST http://127.0.0.1:8090/v1/capture \
  -H 'Content-Type: application/json' \
  -d '{
    "submission_id": "n8n-run-124",
    "live_url": "https://portfolio-budi.vercel.app",
    "scan_routes": true,
    "max_routes": 6
  }'
```

Hasilnya satu entri per halaman, dinamai dari path-nya:

```json
"screenshots": [
  { "name": "main",    "url": "https://portfolio-budi.vercel.app/" },
  { "name": "about",   "url": "https://portfolio-budi.vercel.app/about.html" },
  { "name": "produk",  "url": "https://portfolio-budi.vercel.app/produk.html" },
  { "name": "kontak",  "url": "https://portfolio-budi.vercel.app/kontak.html" }
]
```

**Yang dipindai hanyalah navigasi, bukan semua link.** Setiap `<a href>` di
halaman adalah himpunan yang salah: body copy menaut ke repo GitHub, grid toko
menaut ke lima puluh halaman produk, footer menaut ke kebijakan privasi yang
tidak pernah diminta brief. Pencarian karenanya berjalan pada landmark navigasi
dan berhenti di yang pertama berisi:

```
<nav> / role="navigation"   →   <header>   →   semua link di halaman
```

Yang terakhir adalah fallback untuk markup tanpa landmark sama sekali, dan
pemakaiannya dilaporkan di `build.notes`.

Kedalamannya satu tingkat: navigasi milik halaman utama saja. Nav itu sama di
setiap halaman proyek normal, jadi tingkat kedua sebagian besar hanya mengunjungi
ulang yang sudah ditemukan — dan pada portfolio yang punya indeks blog, rekursi
berubah jadi crawl tanpa batas.

Yang ikut disaring: link lintas-origin, `mailto:`/`tel:`/`javascript:`, tautan
unduhan (`.pdf`, `.zip`, gambar, font), dan anchor `#bagian` yang cuma menggulir
halaman yang sama. Sebaliknya `#/kontak` **tidak** disaring — begitulah SPA
hash-router mengeja tampilan yang benar-benar berbeda.

Halaman yang merender piksel identik dengan halaman yang sudah dipotret dibuang
sebagai duplikat, karena `/` dan `/index.html` rutin ditaut berdua.

**Apa pun yang tidak jadi dilakukan pemindai dilaporkan di `build.notes`** —
rute yang melewati `max_routes`, pemindaian yang mentok di plafon waktu, halaman
yang gagal dibuka, duplikat yang dibuang. Daftar rute yang dipotong diam-diam
tidak dapat dibedakan dari daftar yang lengkap, dan penilai akan membaca
selisihnya sebagai "halamannya memang tidak ada".

Biayanya nyata: setiap rute tambahan adalah satu lagi gambar full-page untuk
model yang membacanya. Enam halaman panjang bisa berarti 1,5 MB WebP dan tagihan
token yang setara. `max_routes` ada supaya angka itu jadi pilihan sadar.

### Kegagalan

Setiap respons non-2xx membawa `stage`, supaya pemanggil bisa bercabang tanpa
mencocokkan string pesan:

| Stage                                                | Artinya                             | Layak diulang? |
| ---------------------------------------------------- | ----------------------------------- | -------------- |
| `live_url`                                            | URL ditolak validasi                | tidak          |
| `capture`                                             | Situs tidak menjawab / navigasi gagal | tidak         |
| `fetch_source`, `detect`, `build`, `launch`           | Proyek siswa tidak bisa dibangun    | tidak          |
| `decode`, `validate`                                  | Request salah bentuk                | tidak          |
| `optimize`, `upload`                                  | Masalah di node ini                 | ya             |

Lima yang pertama deterministik: ZIP yang sama gagal dengan cara yang sama, dan
deployment yang mati akan tetap mati. Atur retry di n8n sesuai tabel ini.

### `GET /healthz`

```json
{ "status": "ok", "railpack_available": true }
```

## Alur intake

Formulir di sisi n8n sebaiknya menerima URL **atau** ZIP. Bila URL diisi,
seluruh rantai build dilewati:

```
URL  →  validasi + resolve host  →  Playwright: full-page + scroll + satu interaksi
     →  PNG → WebP  →  unggah keduanya  →  balas
```

Nol image, nol container, nol menit BuildKit. Rantai ZIP di bawah ini ada
semata-mata untuk menghasilkan sebuah URL yang bisa dibuka; kalau siswa sudah
punya satu, membangunnya ulang murni biaya.

ZIP dipakai bila URL tidak diisi. Karena bentuk proyek siswa sangat beragam,
worker menentukan cara build secara berjenjang dan berhenti di kecocokan
pertama:

```
ZIP  →  unzip  →  1. Dockerfile milik siswa?      → pakai apa adanya
                  2. Procfile (proses `web:`)?    → base image dari manifest + perintah siswa
                  3. Railpack tersedia?           → serahkan inferensi build ke Railpack
                  4. Heuristik                    → generate Dockerfile dari package.json /
                                                     requirements.txt / composer.json / index.html
        ↓
   build image ephemeral  →  spawn container (PORT diinject, 512MB, 0.5 vCPU, loopback-only)
        ↓
   health-check semua port terpublikasi  →  capture  →  hancurkan container DAN image
```

ZIP-nya sendiri harus sudah ada di bucket `submissions`. n8n punya node S3 bawaan
yang bisa mengunggahnya ke MinIO, lalu `source_key` diisi key hasil unggahan itu.

`$PORT` diinject dan dipublikasikan 1:1, tetapi sejumlah port umum (3000, 5173,
8080, …) ikut dipublikasikan dan diprobe. Ini menangani aplikasi yang
meng-hardcode portnya dan mengabaikan `$PORT`.

## Optimizer WebP

PNG dari Playwright didekode, diturunkan bila melewati plafon, lalu dienkode
WebP pada kualitas 78 — pita di mana WebP berhenti membuang byte dengan cepat
dan mulai membuang detail.

### Plafonnya lebar, bukan tinggi

Plafon lebar 1920px adalah penghematan token yang murah: model tidak mendapat
apa pun dari screenshot selebar 2400px yang tidak ia dapat dari 1920px.

Plafon tingginya sengaja 8000px. Screenshot full-page rutin empat sampai sepuluh
kali lebih tinggi daripada lebarnya, jadi batas tinggi 1080px yang konvensional
akan menurunkan capture 1440×3951 menjadi **393×1080** — rasio aspek terjaga
sempurna, teksnya hancur total. Keterbacaan ada di lebar, jadi lebar yang
dilindungi.

Bila sebuah halaman tetap melewati plafon tinggi, `MinWidth` 960px yang menang:
gambar dibiarkan lebih tinggi dari plafon daripada teksnya diperas sampai tak
terbaca.

Resampler-nya CatmullRom. Pada screenshot full-page, kernel yang lebih tajam
memakan beberapa ratus milidetik dan tidak membeli apa pun yang bisa dilihat
penilai, karena sumbernya UI yang dirender 1x, bukan foto.

### Rasio reduksi bergantung pada isi, bukan pada encoder

Terukur pada uji asap, semuanya `downscaled=false`:

| Halaman                        | Ukuran      | PNG    | WebP   | Reduksi | Waktu  |
| ------------------------------ | ----------- | ------ | ------ | ------- | ------ |
| `example.com` (teks minimal)   | 1440×900    | 19 KB  | 13 KB  | 33,4%   | —      |
| `go.dev` main (full-page)      | 1440×3951   | 489 KB | 270 KB | 44,6%   | 436 ms |
| `go.dev` interaction           | 1440×3892   | 442 KB | 212 KB | 51,9%   | 351 ms |

Halaman yang datar dan hemat sebagai PNG hanya menyusut sepertiga; yang berisi
foto atau gradien bergranul jauh melampaui itu. Reduksi 70–85% tidak realistis
untuk UI web pada umumnya.

## Keamanan

**Tidak ada autentikasi.** Node ini bind ke `127.0.0.1` dan mengandalkan
jaringan, bukan kode, untuk menjaganya. Siapa pun yang bisa menjangkau port ini
dapat menyuruh browser membuka URL pilihannya. Bila n8n berjalan di mesin lain,
tempatkan node ini di belakang jaringan privat atau VPN — jangan sekadar
mengganti alamat bind-nya.

**Proteksi SSRF pada `live_url`.** Pada jalur ZIP, Playwright diarahkan ke
alamat loopback yang worker pilih sendiri. Pada jalur URL, siswalah yang memilih
ke mana browser pergi — dan browser itu duduk di host network, dengan jangkauan
ke MinIO, BuildKit, serta endpoint metadata instance di mesin cloud.
`internal/urlguard` karenanya menuntut skema http/https, menolak URL
berkredensial, me-resolve host, dan menolak setiap alamat loopback, privat,
link-local (termasuk `169.254.169.254`), CGNAT dan rentang reserved. Host yang
punya satu record publik dan satu privat ikut ditolak — kalau tidak, hasilnya
lotere saat connect.

Validasi sekali di awal tidak cukup, karena situs mana pun bisa membalas 302 ke
`127.0.0.1`. Jadi ada tiga lapis: pemeriksaan sebelum navigasi, route handler
Playwright yang memvalidasi setiap request yang halaman itu keluarkan, dan
pemeriksaan ulang terhadap alamat tempat halaman benar-benar mendarat.

Satu celah tersisa dan tidak bisa ditutup dari sisi ini: hostname yang record
DNS-nya berubah ke alamat privat di antara pemeriksaan kami dan lookup milik
browser (DNS rebinding). Menutupnya butuh browser dirutekan lewat proxy yang
memeriksa ulang saat connect.

**Egress kode siswa belum diblokir.** Container dipublikasikan hanya ke loopback
host sehingga tidak dapat dijangkau dari luar mesin, tetapi kode siswa masih bisa
menghubungi internet. Menutup ini butuh network Docker `internal: true` dengan
Playwright ikut berjalan di dalam network tersebut, karena port publishing tidak
berfungsi pada network internal. Jalur URL tidak terpengaruh — tidak ada satu
baris pun kode siswa yang dieksekusi di mesin ini.

## Object storage

Kedua versi gambar diunggah ke bucket `screenshots`:
`<submission_id>/<name>.png` dan `<submission_id>/<name>.webp`.

Pemanggil mendapat WebP, tetapi PNG tetap disimpan sebagai bukti yang tidak
pernah diresample — sengketa nilai diperiksa terhadap gambar aslinya. Isinya
dilihat lewat konsol MinIO di :9001 bila diperlukan.

Tidak ada pembersihan otomatis. Sebuah lifecycle rule di MinIO adalah tempat
yang tepat untuk itu, dan belum dipasang.

## Performa dan batasan

Terverifikasi lewat uji asap terhadap situs sungguhan:

- **Jalur URL: 2,8 detik** untuk halaman sederhana satu screenshot, **9,8 detik**
  untuk halaman panjang dengan dua screenshot.
- **Jalur ZIP:** build dingin lewat Railpack untuk situs statis 4 halaman
  146 detik; request kedua dengan image ter-cache **17,8 detik** termasuk
  pemindaian 4 rute.
- **Kompresi: 350–440 ms** per screenshot full-page 1440×~3900 tanpa downscale.
- **Proteksi SSRF menolak** loopback, `169.254.169.254`, `localhost`, skema
  `file:`, dan URL berkredensial.
- **Pemindaian rute** terbukti menyaring link body/footer/unduhan/lintas-origin;
  pada `go.dev` hanya 12 rute nav yang terbaca dari halaman berisi puluhan link.

Belum diukur:

- Perilaku pada halaman yang benar-benar ekstrem (>8000px) di dunia nyata,
  yang memicu jalur `MinWidth`.
- Concurrency. Setiap request memakai satu browser context dari satu instance
  Chromium bersama; belum diuji di bawah beban paralel.
- SPA hash-router sungguhan. Logika `#/rute` ada dan teruji pada unit test,
  tetapi belum pernah dijalankan terhadap SPA nyata.

## Catatan Environtment

Driver `playwright-go` v0.6000.0 mengunduh dari `playwright.azureedge.net` yang
sudah dipensiunkan Microsoft (semua mirror 404). `scripts/install-playwright-driver.sh`
merakit driver itu dari npm sebagai gantinya. Jangan pakai `go run ./cmd/installdeps`.

Railpack dijalankan dengan `DOCKER_CONFIG` terisolasi. `~/.docker/config.json`
di host Docker Desktop menunjuk credential helper `desktop.exe` yang tidak bisa
dieksekusi BuildKit sisi Linux, sehingga pull image publik dari ghcr.io ditolak
dengan `denied`. Set `RAILPACK_DOCKER_CONFIG` bila nanti butuh registry privat.

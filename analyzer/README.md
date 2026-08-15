# Conformance Analyzer

Memeriksa apakah source code submission memenuhi ketentuan rubrik, secara
deterministik lewat AST — bukan lewat LLM.

```
n8n ──┬──POST /v1/capture───▶ worker    ──▶ screenshot
      └──POST /v1/analyze───▶ analyzer  ──▶ laporan conformance
                                              │
                                     digabung ▼
                                            LLM ──▶ skor
```

Kedua panggilan itu tidak saling bergantung, jadi n8n bisa menjalankannya
paralel. Analyzer mengambil ZIP-nya sendiri dari object storage.

Beda dengan linter: linter mencari yang salah, analyzer ini membuktikan yang
**harus ada**. Rubrik memilih capability; analyzer menjawab ada/tidak, berapa
kali, dan di baris mana.

## Kenapa AST, bukan regex

`grep -r "fetch"` kena komentar `// nanti pakai fetch()`, kena string
`"gunakan fetch untuk..."`, dan kena `node_modules/axios/` yang memang penuh
`fetch` asli. Ketiganya menghasilkan nilai lulus untuk submission yang tidak
mengimplementasikan apa pun.

Analyzer mem-parse file jadi AST lalu mencari *call expression*, jadi teks di
komentar dan string tidak pernah ikut terhitung. Direktori dependency dan bundle
hasil build juga dibuang sebelum analisis. Lihat test `ignores_comments_and_strings`
dan `skips_dependencies_and_bundles`.

## API

### `GET /healthz`

```json
{ "status": "ok", "capabilities": 32 }
```

### `GET /v1/capabilities`

Katalog capability yang didukung — pakai ini saat menyusun rubrik.

### `POST /v1/analyze`

```bash
curl -X POST http://127.0.0.1:8092/v1/analyze \
  -H 'Content-Type: application/json' \
  -d '{
    "submission_id": "n8n-run-300",
    "source_key": "submission.zip",
    "checks": [
      {"id": "c1", "title": "Mengambil data dari API", "capability": "http_request"},
      {"id": "c2", "title": "Menangani kegagalan request", "capability": "error_handling"}
    ]
  }'
```

| Field                      | Wajib | Keterangan                                                  |
| -------------------------- | ----- | ----------------------------------------------------------- |
| `submission_id`            | ya    | Identitas milik pemanggil                                   |
| `source_key`               | ya    | Object key ZIP di bucket `submissions`                       |
| `checks[].id`              | ya    | Identitas check milik pemanggil, muncul di respons          |
| `checks[].capability`      | ya    | Salah satu id dari `GET /v1/capabilities`                   |
| `checks[].title`           | —     | Diteruskan apa adanya, memudahkan pembacaan hasil           |
| `checks[].track`           | —     | Pengelompokan, mis. `"Artificial Intelligence"`             |
| `checks[].expect`          | —     | `present` (default) atau `absent` untuk butir larangan      |
| `checks[].min_occurrences` | —     | Default 1                                                   |
| `max_evidence_per_check`   | —     | Default 5                                                   |

### Butir larangan

Rubrik capstone memuat butir "Tidak menggunakan ...". Nyatakan dengan
`expect: "absent"` — check lulus bila capability itu **tidak** ditemukan, dan
bila ditemukan, evidence menunjukkan persis di mana pelanggarannya:

```json
{"id": "mq-ai-6", "title": "Tidak menggunakan model dari layanan API",
 "capability": "llm_api_client", "expect": "absent"}
```

## Tiga hasil, bukan dua

`status` bernilai `passed`, `failed`, atau `inconclusive`. Field `passed`
menyertainya sebagai `true` / `false` / `null`.

`inconclusive` berarti **analyzer tidak dapat memutuskan**, bukan bahwa
submission gagal. Ini muncul ketika submission memuat bahasa atau manifest yang
belum bisa dibaca, sehingga ketiadaan bukti tidak membuktikan apa pun.

Butir `inconclusive` **harus** diteruskan ke penilai manusia atau LLM. Kalau
diperlakukan sebagai `failed`, siswa dinyatakan gagal karena keterbatasan
analyzer — bukan karena pekerjaannya.

Respons:

```json
{
  "submission_id": "n8n-run-300",
  "source": {
    "files_scanned": 2,
    "files_skipped": 2,
    "bytes_scanned": 753,
    "root_stripped": null,
    "parse_failures": [],
    "truncated": false,
    "coverage": {
      "languages_present": ["javascript"],
      "languages_analysed": ["javascript"],
      "languages_unsupported": [],
      "manifests_read": ["package.json"],
      "manifests_unsupported": []
    }
  },
  "checks": [
    {
      "id": "c1",
      "title": "Mengambil data dari API",
      "capability": "http_request",
      "expect": "present",
      "status": "passed",
      "passed": true,
      "occurrences": 1,
      "evidence": [
        {
          "file": "js/api.js",
          "line": 5,
          "column": 23,
          "matched": "fetch",
          "snippet": "fetch(`${BASE}/produk`)"
        }
      ]
    }
  ],
  "capabilities_detected": ["async_await", "error_handling", "http_request"],
  "duration_ms": 9
}
```

`evidence` membawa `file:line` supaya hasilnya bisa diaudit dan siswa bisa
ditunjukkan buktinya saat membanding nilai. `capabilities_detected` berisi
seluruh capability yang terdeteksi, termasuk yang tidak diminta rubrik — berguna
untuk menyusun rubrik berikutnya.

`parse_failures` diisi bila ada file yang gagal di-parse; file lain tetap
dianalisis, jadi satu file rusak tidak menggagalkan seluruh penilaian.

## Batasan

Analyzer hanya melihat source code, jadi ia butuh `source_key`. Submission yang
hanya mengirim `live_url` tidak bisa diperiksa dengan cara ini — untuk itu
sinyalnya harus datang dari jalur runtime di worker.

Sebaliknya, source code tidak membuktikan kode itu benar-benar berjalan. Kode
yang ada tapi tidak pernah terpanggil tetap dihitung `passed`. Menggabungkan
hasil analyzer dengan network request yang tercatat saat capture memberi
gambaran yang lebih jujur:

| Statis | Runtime | Artinya                                        |
| ------ | ------- | ---------------------------------------------- |
| ada    | ada     | Terimplementasi dan berfungsi                  |
| ada    | tidak   | Dead code, atau butuh interaksi yang tak dipicu |
| tidak  | ada     | Lewat library yang belum terdaftar              |
| tidak  | tidak   | Ketentuan tidak dipenuhi                       |

## Cakupan bahasa

Submission capstone boleh polyglot, jadi setiap respons melaporkan apa yang
benar-benar terbaca lewat `source.coverage`:

```json
"coverage": {
  "languages_present": ["go", "php", "python"],
  "languages_analysed": [],
  "languages_unsupported": ["go", "php", "python"],
  "manifests_read": ["composer.json", "go.mod", "requirements.txt"],
  "manifests_unsupported": []
}
```

| Lapisan            | Didukung                                                                       |
| ------------------ | ------------------------------------------------------------------------------ |
| AST                | JavaScript, TypeScript, JSX, TSX (Oxc); Python (tree-sitter)                     |
| Notebook           | `.ipynb` — sel kode diurai sebagai Python, sel markdown dihitung                |
| Markup             | HTML, CSS, SCSS, Sass (komentar dibuang sebelum pencocokan)                     |
| Manifest           | package.json, requirements.txt, pyproject.toml, Pipfile, go.mod, composer.json  |
| Keberadaan berkas  | log TensorBoard, model terlatih, notebook                                       |
| Belum ada          | AST Go, PHP, Dart, Kotlin, Java                                                 |

Manifest memberi bukti **positif** lintas bahasa: `guzzlehttp/guzzle` di
`composer.json` membuktikan HTTP client dipakai walau file PHP-nya tak terbaca.
Tapi ketiadaannya tidak membuktikan sebaliknya — Go bisa memakai `net/http`,
PHP memakai PDO, dan Python memakai `sqlite3` tanpa satu pun entri manifest.
Karena itu hanya `module_bundler`, `streamlit_dashboard`, dan `ml_in_app`
dianggap konklusif dari manifest saja; sisanya jatuh ke `inconclusive` bila ada
bahasa yang belum terbaca. Lihat test `capabilities_reachable_via_stdlib_need_ast`.

## Empat lapisan bukti

Satu capability bisa dibuktikan dari beberapa lapisan sekaligus, dan `evidence`
menunjukkan lapisan mana yang menjawabnya:

| Lapisan           | Contoh capability                                                  | Ketiadaan bukti berarti           |
| ----------------- | ------------------------------------------------------------------ | --------------------------------- |
| AST               | `http_request`, `rest_route_definition`, `custom_keras_component`   | konklusif untuk JS/TS/Python      |
| Markup            | `responsive_layout`, `css_framework`, `semantic_html`               | konklusif                         |
| Manifest          | `module_bundler`, `express_framework`, `ml_serving_api`             | konklusif untuk yang tanpa stdlib |
| Keberadaan berkas | `saved_model_artifact`, `tensorboard_integration`, `notebook_present` | konklusif                       |

## Yang belum bisa dinilai

Butir rubrik berikut tidak akan pernah dijawab oleh analyzer ini, dan sebaiknya
tidak dikirim sebagai `checks`:

| Butir                                          | Alasan                                          |
| ---------------------------------------------- | ----------------------------------------------- |
| Akurasi ≥ 85%, MAE ≤ 0,02                      | angkanya ada di output notebook; belum diekstrak |
| Fitur utama berjalan tanpa crash               | perlu bukti runtime dari worker                 |
| AI/ML sebagai fitur **utama**                  | penilaian, bukan fakta                          |
| Kualitas pertanyaan bisnis, kesimpulan analisis | penilaian, bukan fakta                          |
| Tidak menggunakan Web Generator                | belum ada detektornya                           |
| Mockup, deployment, laporan PDF                | dinilai lewat jalur lain, bukan oleh analyzer    |
| Backend Go / PHP                               | perlu AST bahasa tersebut; manifest sudah dibaca |

## Notebook

Berkas `.ipynb` tidak dikonversi ke `.py` di disk — sel kodenya diurai langsung
sebagai Python, per sel. Karena itu evidence menunjuk sel yang tepat:

```json
{"file": "notebooks/eda.ipynb (cell 7)", "line": 3, "matched": "StandardScaler"}
```

Baris `!pip install` dan `%matplotlib` dibuang sebelum parsing supaya tidak
merusak AST. Sel markdown dihitung untuk capability `notebook_narrative` —
butir rubrik "tidak melakukan analisis tanpa penjelasan markdown" dapat dinilai
dengan `min_occurrences`, misalnya minimal 5 sel penjelasan.

## Batas keamanan

Analyzer tidak pernah menjalankan kode submission dan tidak menulis apa pun ke
disk: ZIP dibaca langsung dari memori. Tidak ada akses ke `docker.sock`, tidak
ada network sandbox. Batasnya:

| Batas                  | Nilai  |
| ---------------------- | ------ |
| Berkas dianalisis      | 3.000  |
| Ukuran per berkas      | 1 MB   |
| Total byte dianalisis  | 64 MB  |
| Panjang baris (bundle) | 5.000  |

Berkas yang melewati batas dilewati dan dihitung di `files_skipped`;
`truncated: true` menandakan ada yang tidak dianalisis.

## Contoh nyata

`examples/` memuat satu putaran penuh terhadap submission capstone sungguhan:

| Berkas                             | Isi                                                     |
| ---------------------------------- | ------------------------------------------------------- |
| `request-rubrik-capstone.json`     | 28 check Main Quest + Side Quest, lengkap dengan `track` dan butir larangan |
| `response-glucognito.json`         | respons apa adanya: 17 lulus, 11 gagal, 30 ms            |

Pakai yang pertama sebagai titik awal menyusun rubrik:

```bash
curl -X POST http://127.0.0.1:8092/v1/analyze \
  -H 'Content-Type: application/json' \
  -d @examples/request-rubrik-capstone.json
```

## Pengembangan

```bash
cargo test
cargo run
```

Perlu `S3_ENDPOINT_HOST` dan kredensialnya. Untuk stack dev, MinIO sudah
disediakan `docker-compose.dev.yml`:

```bash
docker compose -f docker-compose.dev.yml up -d analyzer
```

Menambah capability: daftarkan di `src/capability/mod.rs`, lalu deteksi
node-nya di `src/capability/js.rs`. Setiap capability sebaiknya punya test
positif dan satu test yang membuktikan ia tidak kena false positive.

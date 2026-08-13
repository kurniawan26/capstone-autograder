package capture

import (
	"net/url"
	"testing"
)

func TestRouteName(t *testing.T) {
	cases := []struct {
		path string
		want string
	}{
		{"/about", "about"},
		{"/about.html", "about"},
		{"/produk/detail.html", "produk-detail"},
		{"/Kontak", "kontak"},
		{"/blog/post-satu/", "blog-post-satu"},
		{"/#/kontak", "kontak"},
		{"/#/produk/detail", "produk-detail"},
		{"/a_b c", "a-b-c"},
		{"/", "route-1"},
		{"/---", "route-1"},
	}

	for _, tc := range cases {
		t.Run(tc.path, func(t *testing.T) {
			got := routeName(route{Path: tc.path}, 0)
			if got != tc.want {
				t.Fatalf("routeName(%q) = %q, want %q", tc.path, got, tc.want)
			}
		})
	}
}

func TestRouteNameTruncatesLongPaths(t *testing.T) {
	long := "/kategori/produk/elektronik/komputer/laptop/gaming/spesifikasi-lengkap-terbaru"

	got := routeName(route{Path: long}, 0)
	if len(got) > 40 {
		t.Fatalf("name %q is %d chars, want <= 40", got, len(got))
	}
	if got != "spesifikasi-lengkap-terbaru" && len(got) == 0 {
		t.Fatalf("truncation produced an unusable name: %q", got)
	}
}

func TestUniqueNameDisambiguates(t *testing.T) {
	taken := map[string]bool{"main": true, "interaction": true}

	if got := uniqueName("about", taken); got != "about" {
		t.Fatalf("first use should keep the plain name, got %q", got)
	}
	if got := uniqueName("about", taken); got != "about-2" {
		t.Fatalf("second use should be disambiguated, got %q", got)
	}
	if got := uniqueName("about", taken); got != "about-3" {
		t.Fatalf("third use should be disambiguated, got %q", got)
	}
	if got := uniqueName("main", taken); got != "main-2" {
		t.Fatalf("a route must not collide with the landing page, got %q", got)
	}
}

func TestDownloadableFiltersNonPages(t *testing.T) {
	skip := []string{
		"/cv.pdf", "/assets/source.zip", "/img/hero.png", "/photo.JPEG",
		"/doc/laporan.docx", "/data/nilai.csv", "/video/demo.mp4", "/font/inter.woff2",
	}
	for _, p := range skip {
		if !downloadable.MatchString(p) {
			t.Fatalf("%q should have been filtered out of the route list", p)
		}
	}

	keep := []string{"/about", "/about.html", "/produk/detail.php", "/kontak/", "/blog.htm"}
	for _, p := range keep {
		if downloadable.MatchString(p) {
			t.Fatalf("%q is a page and should have been kept", p)
		}
	}
}

func TestDisplayPath(t *testing.T) {
	cases := []struct{ raw, want string }{
		{"https://x.test/about", "/about"},
		{"https://x.test/", "/"},
		{"https://x.test", "/"},
		{"https://x.test/#/kontak", "/#/kontak"},
	}

	for _, tc := range cases {
		u, err := url.Parse(tc.raw)
		if err != nil {
			t.Fatalf("parse %q: %v", tc.raw, err)
		}
		if got := displayPath(u); got != tc.want {
			t.Fatalf("displayPath(%q) = %q, want %q", tc.raw, got, tc.want)
		}
	}
}

func TestDuplicateOfMatchesIdenticalPixels(t *testing.T) {
	shots := []Shot{
		{Name: "main", PNG: []byte{1, 2, 3}},
		{Name: "about", PNG: []byte{4, 5, 6}},
	}

	if got := duplicateOf(shots, []byte{4, 5, 6}); got != "about" {
		t.Fatalf("expected a duplicate of %q, got %q", "about", got)
	}
	if got := duplicateOf(shots, []byte{7, 8, 9}); got != "" {
		t.Fatalf("expected no duplicate, got %q", got)
	}
}

func TestOptionDefaults(t *testing.T) {
	var zero Options
	if zero.maxRoutes() != defaultMaxRoutes {
		t.Fatalf("maxRoutes() = %d, want the default %d", zero.maxRoutes(), defaultMaxRoutes)
	}
	if zero.budget() != defaultScanBudget {
		t.Fatalf("budget() = %s, want the default %s", zero.budget(), defaultScanBudget)
	}

	set := Options{MaxRoutes: 3}
	if set.maxRoutes() != 3 {
		t.Fatalf("an explicit MaxRoutes was ignored: %d", set.maxRoutes())
	}
}

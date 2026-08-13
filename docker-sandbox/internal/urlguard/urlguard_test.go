package urlguard

import (
	"context"
	"net/netip"
	"strings"
	"testing"
)

func TestParseRejectsNonHTTP(t *testing.T) {
	cases := []struct {
		name string
		raw  string
	}{
		{"empty", ""},
		{"blank", "   "},
		{"file scheme", "file:///etc/passwd"},
		{"data scheme", "data:text/html,<h1>hi</h1>"},
		{"no scheme", "project.vercel.app"},
		{"no host", "http://"},
		{"credentials", "https://admin:secret@example.com"},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if _, err := Parse(tc.raw); err == nil {
				t.Fatalf("Parse(%q) accepted a URL it should have refused", tc.raw)
			}
		})
	}
}

func TestParseAcceptsOrdinaryDeploymentURLs(t *testing.T) {
	for _, raw := range []string{
		"https://project.vercel.app",
		"http://project.netlify.app/portfolio",
		"  https://example.com/path?q=1  ",
		"HTTPS://Example.com",
	} {
		if _, err := Parse(raw); err != nil {
			t.Fatalf("Parse(%q) refused a valid URL: %v", raw, err)
		}
	}
}

func TestCheckAddrBlocksInternalAddresses(t *testing.T) {
	blocked := []string{
		"127.0.0.1",
		"::1",
		"::ffff:127.0.0.1",
		"10.0.0.5",
		"172.16.3.4",
		"192.168.1.10",
		"fd00::1",
		"169.254.169.254",
		"0.0.0.0",
		"100.64.0.1",
	}

	for _, raw := range blocked {
		addr := netip.MustParseAddr(raw)
		if err := checkAddr(addr); err == nil {
			t.Fatalf("checkAddr(%s) allowed an internal address", raw)
		}
	}
}

func TestCheckAddrAllowsPublicAddresses(t *testing.T) {
	for _, raw := range []string{"93.184.216.34", "8.8.8.8", "2606:2800:220:1:248:1893:25c8:1946"} {
		if err := checkAddr(netip.MustParseAddr(raw)); err != nil {
			t.Fatalf("checkAddr(%s) refused a public address: %v", raw, err)
		}
	}
}

func TestCheckRejectsLiteralLoopbackURL(t *testing.T) {
	_, err := Check(context.Background(), "http://127.0.0.1:9000/screenshots")
	if err == nil {
		t.Fatal("Check allowed a loopback URL")
	}
	if !strings.Contains(err.Error(), "loopback") {
		t.Fatalf("expected a loopback error, got: %v", err)
	}
}

func TestCheckRejectsMetadataEndpoint(t *testing.T) {
	if _, err := Check(context.Background(), "http://169.254.169.254/latest/meta-data/"); err == nil {
		t.Fatal("Check allowed the cloud metadata endpoint")
	}
}

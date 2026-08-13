package urlguard

import (
	"context"
	"fmt"
	"net"
	"net/netip"
	"net/url"
	"strings"
	"time"
)

const lookupTimeout = 5 * time.Second

var reservedPrefixes = []netip.Prefix{
	netip.MustParsePrefix("0.0.0.0/8"),
	netip.MustParsePrefix("100.64.0.0/10"),
	netip.MustParsePrefix("192.0.0.0/24"),
	netip.MustParsePrefix("198.18.0.0/15"),
	netip.MustParsePrefix("64:ff9b::/96"),
	netip.MustParsePrefix("100::/64"),
}

func Check(ctx context.Context, raw string) (*url.URL, error) {
	u, err := Parse(raw)
	if err != nil {
		return nil, err
	}
	if err := checkHost(ctx, u.Hostname()); err != nil {
		return nil, err
	}
	return u, nil
}

func Allow(raw string) error {
	_, err := Check(context.Background(), raw)
	return err
}

func Parse(raw string) (*url.URL, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil, fmt.Errorf("no URL given")
	}

	u, err := url.Parse(raw)
	if err != nil {
		return nil, fmt.Errorf("not a valid URL: %w", err)
	}

	switch strings.ToLower(u.Scheme) {
	case "http", "https":
	default:
		return nil, fmt.Errorf("unsupported URL scheme %q; only http and https are accepted", u.Scheme)
	}
	if u.Hostname() == "" {
		return nil, fmt.Errorf("URL has no host")
	}
	if u.User != nil {
		return nil, fmt.Errorf("URLs carrying credentials are not accepted")
	}
	return u, nil
}

func checkHost(ctx context.Context, host string) error {
	if addr, err := netip.ParseAddr(host); err == nil {
		return checkAddr(addr)
	}

	ctx, cancel := context.WithTimeout(ctx, lookupTimeout)
	defer cancel()

	addrs, err := net.DefaultResolver.LookupNetIP(ctx, "ip", host)
	if err != nil {
		return fmt.Errorf("cannot resolve %s: %w", host, err)
	}
	if len(addrs) == 0 {
		return fmt.Errorf("%s resolves to no addresses", host)
	}

	for _, addr := range addrs {
		if err := checkAddr(addr); err != nil {
			return err
		}
	}
	return nil
}

func checkAddr(addr netip.Addr) error {
	addr = addr.Unmap()

	switch {
	case addr.IsLoopback():
		return fmt.Errorf("%s is a loopback address", addr)
	case addr.IsPrivate():
		return fmt.Errorf("%s is a private address", addr)
	case addr.IsLinkLocalUnicast(), addr.IsLinkLocalMulticast():
		return fmt.Errorf("%s is a link-local address", addr)
	case addr.IsUnspecified(), addr.IsMulticast(), addr.IsInterfaceLocalMulticast():
		return fmt.Errorf("%s is not a routable public address", addr)
	}

	for _, prefix := range reservedPrefixes {
		if prefix.Contains(addr) {
			return fmt.Errorf("%s falls in reserved range %s", addr, prefix)
		}
	}
	return nil
}

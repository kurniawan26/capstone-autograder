package capture

import (
	"bytes"
	"fmt"
	"log/slog"
	"net/url"
	"path"
	"regexp"
	"strings"
	"time"

	"github.com/playwright-community/playwright-go"
)

const (
	viewportWidth  = 1440
	viewportHeight = 900
	navTimeoutMs   = 20_000
	settleDelayMs  = 1_200
)

const (
	defaultMaxRoutes  = 8
	defaultScanBudget = 90 * time.Second
)

type Shot struct {
	Name string
	URL  string
	PNG  []byte
}

type Options struct {
	Guard func(rawURL string) error

	ScanRoutes bool

	MaxRoutes int

	Budget time.Duration
}

func (o Options) maxRoutes() int {
	if o.MaxRoutes <= 0 {
		return defaultMaxRoutes
	}
	return o.MaxRoutes
}

func (o Options) budget() time.Duration {
	if o.Budget <= 0 {
		return defaultScanBudget
	}
	return o.Budget
}

type Result struct {
	Shots []Shot
	Notes []string
}

type Capturer struct {
	pw      *playwright.Playwright
	browser playwright.Browser
	log     *slog.Logger
}

func New(log *slog.Logger) (*Capturer, error) {
	pw, err := playwright.Run()
	if err != nil {
		return nil, fmt.Errorf("start playwright: %w", err)
	}
	browser, err := pw.Chromium.Launch(playwright.BrowserTypeLaunchOptions{
		Headless: playwright.Bool(true),
		Args:     []string{"--disable-dev-shm-usage", "--no-sandbox"},
	})
	if err != nil {
		pw.Stop()
		return nil, fmt.Errorf("launch chromium: %w", err)
	}
	return &Capturer{pw: pw, browser: browser, log: log}, nil
}

func (c *Capturer) Close() {
	if c.browser != nil {
		c.browser.Close()
	}
	if c.pw != nil {
		c.pw.Stop()
	}
}

func (c *Capturer) Capture(baseURL string, opts Options) (Result, error) {
	started := time.Now()

	ctx, err := c.browser.NewContext(playwright.BrowserNewContextOptions{
		Viewport:          &playwright.Size{Width: viewportWidth, Height: viewportHeight},
		IgnoreHttpsErrors: playwright.Bool(true),
	})
	if err != nil {
		return Result{}, fmt.Errorf("new browser context: %w", err)
	}
	defer ctx.Close()

	page, err := ctx.NewPage()
	if err != nil {
		return Result{}, fmt.Errorf("new page: %w", err)
	}

	if opts.Guard != nil {
		if err := page.Route("**/*", func(route playwright.Route) {
			target := route.Request().URL()
			if err := opts.Guard(target); err != nil {
				c.log.Warn("blocked a request from the captured page", "url", target, "err", err)
				route.Abort("blockedbyclient")
				return
			}
			route.Continue()
		}); err != nil {
			return Result{}, fmt.Errorf("install request guard: %w", err)
		}
	}

	if err := c.navigate(page, baseURL, opts); err != nil {
		return Result{}, err
	}

	main, err := c.capturePage(page)
	if err != nil {
		return Result{}, fmt.Errorf("main screenshot: %w", err)
	}

	result := Result{Shots: []Shot{{Name: "main", URL: page.URL(), PNG: main}}}

	if opts.ScanRoutes {
		scanned := c.scanRoutes(page, &result, opts, started)
		if scanned > 0 {
			return result, nil
		}
		result.Notes = append(result.Notes,
			"Route scan found no other same-origin pages; fell back to a single interaction.")
	}

	c.captureInteraction(page, &result, main)
	return result, nil
}

func (c *Capturer) captureInteraction(page playwright.Page, result *Result, main []byte) {
	clicked, err := clickPrimaryNav(page)
	if err != nil {
		c.log.Warn("interaction step failed", "err", err)
		return
	}
	if !clicked {
		return
	}

	page.WaitForTimeout(settleDelayMs)
	inter, err := page.Screenshot(playwright.PageScreenshotOptions{
		FullPage: playwright.Bool(true),
		Type:     playwright.ScreenshotTypePng,
	})
	switch {
	case err != nil:
		c.log.Warn("interaction screenshot failed", "err", err)
	case bytes.Equal(inter, main):
		c.log.Info("interaction produced an identical view; dropping the duplicate")
	default:
		result.Shots = append(result.Shots,
			Shot{Name: "interaction", URL: page.URL(), PNG: inter})
	}
}

func (c *Capturer) scanRoutes(page playwright.Page, result *Result, opts Options, started time.Time) int {
	routes, source, err := discoverRoutes(page)
	if err != nil {
		c.log.Warn("route discovery failed", "err", err)
		result.Notes = append(result.Notes, "Route discovery failed: "+err.Error())
		return 0
	}
	if len(routes) == 0 {
		return 0
	}

	c.log.Info("route scan", "source", source, "found", len(routes))

	if source == "all" {
		result.Notes = append(result.Notes,
			"No <nav> or <header> found; routes were taken from every link on the page.")
	}

	limit := min(opts.maxRoutes()-1, len(routes))
	if limit < 0 {
		limit = 0
	}

	captured, visited := 0, 0
	hitLimit := false
	taken := map[string]bool{"main": true, "interaction": true}

	for i, route := range routes {
		if captured >= limit {
			hitLimit = true
			break
		}
		if elapsed := time.Since(started); elapsed > opts.budget() {
			result.Notes = append(result.Notes, fmt.Sprintf(
				"Route scan stopped at the %s budget after visiting %d of %d linked pages.",
				opts.budget(), visited, len(routes)))
			break
		}
		visited++

		if err := c.navigate(page, route.URL, opts); err != nil {
			c.log.Warn("route navigation failed", "url", route.URL, "err", err)
			result.Notes = append(result.Notes,
				fmt.Sprintf("Route %s could not be opened: %v", route.Path, err))
			continue
		}

		png, err := c.capturePage(page)
		if err != nil {
			c.log.Warn("route screenshot failed", "url", route.URL, "err", err)
			result.Notes = append(result.Notes,
				fmt.Sprintf("Route %s could not be captured: %v", route.Path, err))
			continue
		}

		if dup := duplicateOf(result.Shots, png); dup != "" {
			c.log.Info("route renders identically to an earlier shot; dropping",
				"url", route.URL, "same_as", dup)
			result.Notes = append(result.Notes,
				fmt.Sprintf("Route %s renders identically to %q; duplicate dropped.", route.Path, dup))
			continue
		}

		name := uniqueName(routeName(route, i), taken)
		result.Shots = append(result.Shots, Shot{Name: name, URL: page.URL(), PNG: png})
		captured++
	}

	if unvisited := len(routes) - visited; hitLimit && unvisited > 0 {
		result.Notes = append(result.Notes, fmt.Sprintf(
			"Navigation lists %d pages; captured %d of them (max_routes=%d), %d never opened.",
			len(routes)+1, captured+1, opts.maxRoutes(), unvisited))
	}

	return captured
}

func (c *Capturer) navigate(page playwright.Page, rawURL string, opts Options) error {
	if _, err := page.Goto(rawURL, playwright.PageGotoOptions{
		WaitUntil: playwright.WaitUntilStateNetworkidle,
		Timeout:   playwright.Float(navTimeoutMs),
	}); err != nil {
		if _, err2 := page.Goto(rawURL, playwright.PageGotoOptions{
			WaitUntil: playwright.WaitUntilStateDomcontentloaded,
			Timeout:   playwright.Float(navTimeoutMs),
		}); err2 != nil {
			return fmt.Errorf("navigate to %s: %w", rawURL, err2)
		}
	}

	if opts.Guard != nil {
		if err := opts.Guard(page.URL()); err != nil {
			return fmt.Errorf("navigation landed on a blocked address: %w", err)
		}
	}
	return nil
}

func (c *Capturer) capturePage(page playwright.Page) ([]byte, error) {
	if err := autoScroll(page); err != nil {
		c.log.Warn("auto-scroll failed", "err", err)
	}
	page.WaitForTimeout(settleDelayMs)

	return page.Screenshot(playwright.PageScreenshotOptions{
		FullPage: playwright.Bool(true),
		Type:     playwright.ScreenshotTypePng,
	})
}

type route struct {
	URL  string
	Path string
}

var downloadable = regexp.MustCompile(`(?i)\.(pdf|zip|rar|7z|tar|gz|docx?|xlsx?|pptx?|csv|png|jpe?g|gif|svg|webp|ico|mp[34]|wav|avi|mov|woff2?|ttf)$`)

func discoverRoutes(page playwright.Page) ([]route, string, error) {
	raw, err := page.Evaluate(`() => {
		const cur = new URL(location.href);
		const curKey = cur.pathname + cur.search;

		const collect = (roots) => {
			const seen = new Set();
			const out = [];
			for (const root of roots) {
				for (const a of root.querySelectorAll('a[href]')) {
					const href = a.getAttribute('href') || '';
					if (!href || /^(javascript|mailto|tel|data|blob):/i.test(href)) continue;

					let u;
					try { u = new URL(a.href, location.href); } catch { continue; }
					if (u.origin !== cur.origin) continue;

					// "#contact" scrolls the page we already have. "#/contact" is
					// how a hash-router SPA spells a genuinely different view.
					const hashRoute = u.hash.startsWith('#/');
					const key = u.pathname + u.search + (hashRoute ? u.hash : '');
					if (key === curKey && !hashRoute) continue;
					if (seen.has(key)) continue;
					seen.add(key);

					out.push(u.origin + key);
				}
			}
			return out;
		};

		const sources = [
			['nav', [...document.querySelectorAll('nav, [role="navigation"]')]],
			['header', [...document.querySelectorAll('header')]],
			['all', [document]],
		];

		for (const [source, roots] of sources) {
			if (!roots.length) continue;
			const urls = collect(roots);
			if (urls.length) return { source, urls };
		}
		return { source: 'none', urls: [] };
	}`)
	if err != nil {
		return nil, "", fmt.Errorf("scan navigation links: %w", err)
	}

	payload, ok := raw.(map[string]any)
	if !ok {
		return nil, "", nil
	}
	source, _ := payload["source"].(string)
	list, _ := payload["urls"].([]any)

	routes := make([]route, 0, len(list))
	for _, item := range list {
		href, ok := item.(string)
		if !ok {
			continue
		}
		u, err := url.Parse(href)
		if err != nil {
			continue
		}
		if downloadable.MatchString(u.Path) {
			continue
		}
		routes = append(routes, route{URL: href, Path: displayPath(u)})
	}
	return routes, source, nil
}

func displayPath(u *url.URL) string {
	p := u.Path
	if u.Fragment != "" {
		p += "#" + u.Fragment
	}
	if p == "" {
		p = "/"
	}
	return p
}

var nonName = regexp.MustCompile(`[^a-z0-9]+`)

func routeName(r route, index int) string {
	raw := r.Path
	if hash := strings.Index(raw, "#"); hash >= 0 {
		if frag := raw[hash+1:]; strings.Trim(frag, "/") != "" {
			raw = frag
		} else {
			raw = raw[:hash]
		}
	}

	raw = strings.TrimSuffix(raw, path.Ext(raw))
	name := strings.Trim(nonName.ReplaceAllString(strings.ToLower(raw), "-"), "-")

	if len(name) > 40 {
		name = strings.Trim(name[len(name)-40:], "-")
	}
	if name == "" {
		name = fmt.Sprintf("route-%d", index+1)
	}
	return name
}

func uniqueName(name string, taken map[string]bool) string {
	candidate := name
	for i := 2; taken[candidate]; i++ {
		candidate = fmt.Sprintf("%s-%d", name, i)
	}
	taken[candidate] = true
	return candidate
}

func duplicateOf(shots []Shot, png []byte) string {
	for _, s := range shots {
		if bytes.Equal(s.PNG, png) {
			return s.Name
		}
	}
	return ""
}

func autoScroll(page playwright.Page) error {
	_, err := page.Evaluate(`async () => {
		const step = window.innerHeight;
		const max = document.body.scrollHeight;
		for (let y = 0; y < max; y += step) {
			window.scrollTo(0, y);
			await new Promise(r => setTimeout(r, 120));
		}
		window.scrollTo(0, 0);
		await new Promise(r => setTimeout(r, 200));
	}`)
	return err
}

func clickPrimaryNav(page playwright.Page) (bool, error) {
	idx, err := page.Evaluate(`() => {
		const cur = new URL(location.href);
		const anchors = [...document.querySelectorAll('a[href]')];
		for (let i = 0; i < anchors.length; i++) {
			const a = anchors[i];
			const box = a.getBoundingClientRect();
			if (box.width === 0 || box.height === 0) continue;
			const raw = a.getAttribute('href') || '';
			if (!raw || raw.startsWith('#') || /^(javascript|mailto|tel):/i.test(raw)) continue;
			let u;
			try { u = new URL(a.href, location.href); } catch { continue; }
			if (u.origin !== cur.origin) continue;
			if (u.pathname === cur.pathname && u.search === cur.search) continue;
			return i;
		}
		return -1;
	}`)
	if err != nil {
		return false, fmt.Errorf("scan navigation links: %w", err)
	}

	if i, ok := toInt(idx); ok && i >= 0 {
		err := page.Locator("a[href]").Nth(i).Click(playwright.LocatorClickOptions{
			Timeout: playwright.Float(3_000),
		})
		if err == nil {
			page.WaitForLoadState(playwright.PageWaitForLoadStateOptions{
				State:   playwright.LoadStateDomcontentloaded,
				Timeout: playwright.Float(5_000),
			})
			return true, nil
		}
	}

	buttons := page.Locator("button:visible")
	if count, err := buttons.Count(); err == nil && count > 0 {
		if err := buttons.First().Click(playwright.LocatorClickOptions{
			Timeout: playwright.Float(3_000),
		}); err == nil {
			return true, nil
		}
	}

	return false, nil
}

func toInt(v any) (int, bool) {
	switch n := v.(type) {
	case int:
		return n, true
	case float64:
		return int(n), true
	default:
		return 0, false
	}
}

func InstallDrivers() error {
	return playwright.Install(&playwright.RunOptions{
		Browsers: []string{"chromium"},
	})
}

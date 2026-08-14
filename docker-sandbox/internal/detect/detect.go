package detect

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

type Strategy string

const (
	StrategyDockerfile Strategy = "dockerfile"
	StrategyProcfile   Strategy = "procfile"
	StrategyRailpack   Strategy = "railpack"
	StrategyHeuristic  Strategy = "heuristic"
)

type Plan struct {
	Strategy            Strategy
	DockerfileName      string
	GeneratedDockerfile string
	Framework           string
	CandidatePorts      []int
	Notes               []string
}

type Options struct {
	RailpackAvailable bool
}

var commonPorts = []int{3000, 5173, 4173, 8080, 8000, 5000, 4200, 80}

func Detect(srcDir string, opts Options) (Plan, error) {
	if name, ok := findFile(srcDir, "Dockerfile", "dockerfile"); ok {
		return Plan{
			Strategy:       StrategyDockerfile,
			DockerfileName: name,
			Framework:      "custom (student Dockerfile)",
			CandidatePorts: commonPorts,
			Notes:          []string{"Using the Dockerfile shipped with the submission."},
		}, nil
	}

	if name, ok := findFile(srcDir, "Procfile", "procfile"); ok {
		webCmd, err := procfileWebCommand(filepath.Join(srcDir, name))
		if err == nil && webCmd != "" {
			base, framework := baseImageFor(srcDir)
			return Plan{
				Strategy:            StrategyProcfile,
				DockerfileName:      "Dockerfile.autograder",
				GeneratedDockerfile: procfileDockerfile(base, webCmd),
				Framework:           framework + " (Procfile web process)",
				CandidatePorts:      commonPorts,
				Notes: []string{
					fmt.Sprintf("Procfile web command: %s", webCmd),
					fmt.Sprintf("Base image chosen by manifest sniffing: %s", base),
				},
			}, nil
		}
	}

	if opts.RailpackAvailable {
		return Plan{
			Strategy:       StrategyRailpack,
			Framework:      "railpack auto-detected",
			CandidatePorts: commonPorts,
			Notes:          []string{"No Dockerfile or Procfile found; delegating build inference to Railpack."},
		}, nil
	}

	return heuristicPlan(srcDir)
}

func heuristicPlan(srcDir string) (Plan, error) {
	switch {
	case exists(srcDir, "package.json"):
		return nodePlan(srcDir)

	case exists(srcDir, "requirements.txt") || exists(srcDir, "pyproject.toml"):
		return Plan{
			Strategy:       StrategyHeuristic,
			DockerfileName: "Dockerfile.autograder",
			GeneratedDockerfile: dockerfile("python:3.12-slim", []string{
				`RUN if [ -f requirements.txt ]; then pip install --no-cache-dir -r requirements.txt; fi`,
			}, `sh -c "python app.py 2>/dev/null || python main.py 2>/dev/null || python -m http.server $PORT"`),
			Framework:      "python",
			CandidatePorts: append([]int{8000, 5000}, commonPorts...),
			Notes:          []string{"Detected a Python project from requirements.txt / pyproject.toml."},
		}, nil

	case exists(srcDir, "composer.json") || hasExt(srcDir, ".php"):
		return Plan{
			Strategy:       StrategyHeuristic,
			DockerfileName: "Dockerfile.autograder",
			GeneratedDockerfile: dockerfile("php:8.3-cli-alpine", nil,
				`sh -c "php -S 0.0.0.0:$PORT -t ${DOCROOT:-.}"`),
			Framework:      "php",
			CandidatePorts: append([]int{8080, 8000}, commonPorts...),
			Notes:          []string{"Detected a PHP project."},
		}, nil

	case exists(srcDir, "index.html") || exists(srcDir, "public/index.html"):
		return staticPlan(), nil
	}

	return Plan{}, fmt.Errorf(
		"could not determine how to run this project: no Dockerfile, Procfile, package.json, requirements.txt, composer.json or index.html found at the archive root")
}

func nodePlan(srcDir string) (Plan, error) {
	pkg := readPackageJSON(filepath.Join(srcDir, "package.json"))

	install := `RUN if [ -f package-lock.json ]; then npm ci --no-audit --no-fund || npm install --no-audit --no-fund; ` +
		`elif [ -f yarn.lock ]; then yarn install --frozen-lockfile; ` +
		`elif [ -f pnpm-lock.yaml ]; then corepack enable && pnpm install --frozen-lockfile; ` +
		`else npm install --no-audit --no-fund; fi`

	steps := []string{install}
	notes := []string{"Detected a Node project from package.json."}

	if pkg.hasScript("build") {
		steps = append(steps, `RUN npm run build --if-present`)
		notes = append(notes, "Ran the 'build' script.")
	}

	start := `sh -c "` +
		`if [ -d dist ]; then npx --yes serve -s dist -l $PORT; ` +
		`elif [ -d build ]; then npx --yes serve -s build -l $PORT; ` +
		`elif [ -d out ]; then npx --yes serve -s out -l $PORT; ` +
		`else npm run start 2>/dev/null || npm run dev 2>/dev/null || npx --yes serve -l $PORT .; fi"`

	if pkg.hasScript("start") && !pkg.looksStatic() {
		start = `sh -c "npm run start || npm run dev || npx --yes serve -l $PORT ."`
		notes = append(notes, "Using the 'start' script; the app is expected to honour $PORT.")
	}

	return Plan{
		Strategy:            StrategyHeuristic,
		DockerfileName:      "Dockerfile.autograder",
		GeneratedDockerfile: dockerfile("node:20-alpine", steps, start),
		Framework:           "node" + pkg.frameworkSuffix(),
		CandidatePorts:      commonPorts,
		Notes:               notes,
	}, nil
}

func staticPlan() Plan {
	const tmpl = `server {
    listen ${PORT};
    server_name localhost;
    root /usr/share/nginx/html;
    index index.html;
    location / { try_files $uri $uri/ /index.html; }
}
`
	df := `FROM nginx:1.27-alpine
WORKDIR /usr/share/nginx/html
COPY . .
# If the site lives under public/, flatten it so nginx's root still resolves.
RUN if [ -d public ] && [ -f public/index.html ] && [ ! -f index.html ]; then cp -r public/. .; fi
RUN mkdir -p /etc/nginx/templates && printf '%s' ` + shellQuote(tmpl) + ` > /etc/nginx/templates/default.conf.template
RUN rm -f /etc/nginx/conf.d/default.conf
`
	return Plan{
		Strategy:            StrategyHeuristic,
		DockerfileName:      "Dockerfile.autograder",
		GeneratedDockerfile: df,
		Framework:           "static html",
		CandidatePorts:      append([]int{80}, commonPorts...),
		Notes:               []string{"Detected a static site; serving it with nginx."},
	}
}

func dockerfile(base string, steps []string, cmd string) string {
	var b strings.Builder
	fmt.Fprintf(&b, "FROM %s\nWORKDIR /app\nENV PORT=8080\nCOPY . .\n", base)
	for _, s := range steps {
		b.WriteString(s)
		b.WriteString("\n")
	}
	fmt.Fprintf(&b, "CMD %s\n", cmd)
	return b.String()
}

func procfileDockerfile(base, webCmd string) string {
	var b strings.Builder
	fmt.Fprintf(&b, "FROM %s\nWORKDIR /app\nENV PORT=8080\nCOPY . .\n", base)
	b.WriteString(`RUN if [ -f package.json ]; then npm install --no-audit --no-fund; fi` + "\n")
	b.WriteString(`RUN if [ -f requirements.txt ]; then pip install --no-cache-dir -r requirements.txt || true; fi` + "\n")
	fmt.Fprintf(&b, "CMD sh -c %s\n", shellQuote(webCmd))
	return b.String()
}

func baseImageFor(srcDir string) (image, framework string) {
	switch {
	case exists(srcDir, "package.json"):
		return "node:20-alpine", "node"
	case exists(srcDir, "requirements.txt"), exists(srcDir, "pyproject.toml"):
		return "python:3.12-slim", "python"
	case exists(srcDir, "composer.json"):
		return "php:8.3-cli-alpine", "php"
	default:
		return "node:20-alpine", "unknown"
	}
}

func procfileWebCommand(path string) (string, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	for _, line := range strings.Split(string(raw), "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(strings.ToLower(line), "web:") {
			return strings.TrimSpace(line[4:]), nil
		}
	}
	return "", fmt.Errorf("no web process in Procfile")
}

type packageJSON struct {
	Scripts      map[string]string `json:"scripts"`
	Dependencies map[string]string `json:"dependencies"`
	DevDeps      map[string]string `json:"devDependencies"`
}

func readPackageJSON(path string) packageJSON {
	var pkg packageJSON
	raw, err := os.ReadFile(path)
	if err != nil {
		return pkg
	}
	_ = json.Unmarshal(raw, &pkg)
	return pkg
}

func (p packageJSON) hasScript(name string) bool {
	_, ok := p.Scripts[name]
	return ok
}

func (p packageJSON) hasDep(name string) bool {
	if _, ok := p.Dependencies[name]; ok {
		return true
	}
	_, ok := p.DevDeps[name]
	return ok
}

func (p packageJSON) looksStatic() bool {
	return p.hasDep("vite") || p.hasDep("react-scripts") || p.hasDep("parcel") ||
		p.hasDep("@angular/cli") || p.hasDep("astro")
}

func (p packageJSON) frameworkSuffix() string {
	for _, dep := range []string{"next", "nuxt", "vite", "react-scripts", "@angular/cli", "astro", "svelte", "express"} {
		if p.hasDep(dep) {
			return " / " + dep
		}
	}
	return ""
}

func exists(dir, rel string) bool {
	_, err := os.Stat(filepath.Join(dir, filepath.FromSlash(rel)))
	return err == nil
}

func findFile(dir string, names ...string) (string, bool) {
	for _, n := range names {
		if exists(dir, n) {
			return n, true
		}
	}
	return "", false
}

func hasExt(dir, ext string) bool {
	found := false
	filepath.WalkDir(dir, func(path string, d os.DirEntry, err error) error {
		if err != nil || found {
			return nil
		}
		rel, _ := filepath.Rel(dir, path)
		if strings.Count(rel, string(os.PathSeparator)) > 2 {
			return filepath.SkipDir
		}
		if !d.IsDir() && strings.EqualFold(filepath.Ext(path), ext) {
			found = true
		}
		return nil
	})
	return found
}

func shellQuote(s string) string {
	return "'" + strings.ReplaceAll(s, "'", `'"'"'`) + "'"
}

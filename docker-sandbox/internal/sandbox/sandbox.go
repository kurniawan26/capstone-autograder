package sandbox

import (
	"context"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/network"
	"github.com/docker/docker/client"
	"github.com/docker/go-connections/nat"
)

const (
	memoryLimitBytes = 512 * 1024 * 1024
	cpuQuota         = 50_000
	cpuPeriod        = 100_000
	pidsLimit        = 256

	healthCheckTimeout  = 15 * time.Second
	healthCheckInterval = 300 * time.Millisecond
	probeTimeout        = 1500 * time.Millisecond
)

type Manager struct {
	docker  *client.Client
	log     *slog.Logger
	netName string
}

func NewManager(log *slog.Logger) (*Manager, error) {
	cli, err := client.NewClientWithOpts(client.FromEnv, client.WithAPIVersionNegotiation())
	if err != nil {
		return nil, fmt.Errorf("connect to docker: %w", err)
	}

	netName := os.Getenv("SANDBOX_NETWORK")
	if netName != "" {
		log.Info("sandbox containers will join a shared network", "network", netName)
	}

	return &Manager{docker: cli, log: log, netName: netName}, nil
}

func (m *Manager) Docker() *client.Client { return m.docker }

func (m *Manager) Close() error { return m.docker.Close() }

type Spec struct {
	SubmissionID   string
	ImageTag       string
	CandidatePorts []int
}

type Sandbox struct {
	ID           string
	BaseURL      string
	InjectedPort int
	ServingPort  int

	docker *client.Client
	log    *slog.Logger
}

type target struct {
	containerPort int
	baseURL       string
}

func (m *Manager) Launch(ctx context.Context, spec Spec) (sb *Sandbox, err error) {
	injected, err := freePort()
	if err != nil {
		return nil, fmt.Errorf("allocate injected port: %w", err)
	}

	name := sandboxName(spec.SubmissionID)

	ports := []int{injected}
	seen := map[int]bool{injected: true}
	for _, cp := range spec.CandidatePorts {
		if seen[cp] {
			continue
		}
		seen[cp] = true
		ports = append(ports, cp)
	}

	exposed := nat.PortSet{}
	bindings := nat.PortMap{}
	targets := make([]target, 0, len(ports))

	for _, cp := range ports {
		p, err := nat.NewPort("tcp", strconv.Itoa(cp))
		if err != nil {
			return nil, fmt.Errorf("invalid container port %d: %w", cp, err)
		}
		exposed[p] = struct{}{}

		if m.netName != "" {
			targets = append(targets, target{cp, fmt.Sprintf("http://%s:%d", name, cp)})
			continue
		}

		hp := cp
		if cp != injected {
			if hp, err = freePort(); err != nil {
				return nil, fmt.Errorf("allocate host port for %d: %w", cp, err)
			}
		}
		bindings[p] = []nat.PortBinding{{HostIP: "127.0.0.1", HostPort: strconv.Itoa(hp)}}
		targets = append(targets, target{cp, fmt.Sprintf("http://127.0.0.1:%d", hp)})
	}

	var netCfg *network.NetworkingConfig
	if m.netName != "" {
		netCfg = &network.NetworkingConfig{
			EndpointsConfig: map[string]*network.EndpointSettings{m.netName: {}},
		}
	}

	created, err := m.docker.ContainerCreate(ctx,
		&container.Config{
			Image:        spec.ImageTag,
			Env:          []string{fmt.Sprintf("PORT=%d", injected), "HOST=0.0.0.0", "NODE_ENV=production"},
			ExposedPorts: exposed,
			Labels: map[string]string{
				"autograder.sandbox":       "true",
				"autograder.submission_id": spec.SubmissionID,
			},
		},
		&container.HostConfig{
			PortBindings: bindings,
			Resources: container.Resources{
				Memory:    memoryLimitBytes,
				CPUQuota:  cpuQuota,
				CPUPeriod: cpuPeriod,
				PidsLimit: ptr(int64(pidsLimit)),
			},
			SecurityOpt:   []string{"no-new-privileges"},
			RestartPolicy: container.RestartPolicy{Name: "no"},
			AutoRemove:    false,
		},
		netCfg, nil, name)
	if err != nil {
		return nil, fmt.Errorf("create container: %w", err)
	}

	sb = &Sandbox{
		ID:           created.ID,
		InjectedPort: injected,
		docker:       m.docker,
		log:          m.log,
	}

	defer func() {
		if err != nil {
			sb.Destroy(context.WithoutCancel(ctx))
			sb = nil
		}
	}()

	if err = m.docker.ContainerStart(ctx, created.ID, container.StartOptions{}); err != nil {
		return nil, fmt.Errorf("start container: %w", err)
	}

	if err = sb.waitHealthy(ctx, targets); err != nil {
		return nil, err
	}

	return sb, nil
}

func (s *Sandbox) waitHealthy(ctx context.Context, targets []target) error {
	deadline := time.Now().Add(healthCheckTimeout)
	exited, waitErr := s.docker.ContainerWait(ctx, s.ID, container.WaitConditionNotRunning)

	var fallbackTarget *target

	for time.Now().Before(deadline) {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case st := <-exited:
			return fmt.Errorf("container exited before serving (exit status %d)", st.StatusCode)
		case werr := <-waitErr:
			if werr != nil && !errors.Is(werr, context.Canceled) {
				return fmt.Errorf("watching container: %w", werr)
			}
		case <-time.After(healthCheckInterval):
		}

		ready, fallback := probeAll(targets)
		if ready != nil {
			s.adopt(*ready)
			return nil
		}
		if fallback != nil {
			fallbackTarget = fallback
		}
	}

	if fallbackTarget != nil {
		s.adopt(*fallbackTarget)
		s.log.Warn("sandbox served a non-2xx/3xx response; capturing it anyway",
			"container", short(s.ID), "url", s.BaseURL)
		return nil
	}

	return fmt.Errorf("no port answered HTTP within %s (probed container ports %v)",
		healthCheckTimeout, containerPorts(targets))
}

func (s *Sandbox) adopt(t target) {
	s.ServingPort = t.containerPort
	s.BaseURL = t.baseURL
	s.log.Info("sandbox healthy",
		"container", short(s.ID), "url", s.BaseURL, "container_port", t.containerPort)
}

func probeAll(targets []target) (ready, fallback *target) {
	type outcome struct {
		t      target
		status int
	}

	results := make(chan outcome, len(targets))
	var wg sync.WaitGroup
	httpClient := &http.Client{
		Timeout:       probeTimeout,
		CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
	}

	for _, t := range targets {
		wg.Add(1)
		go func(t target) {
			defer wg.Done()
			resp, err := httpClient.Get(t.baseURL + "/")
			if err != nil {
				return
			}
			io.Copy(io.Discard, io.LimitReader(resp.Body, 4096))
			resp.Body.Close()
			results <- outcome{t, resp.StatusCode}
		}(t)
	}
	wg.Wait()
	close(results)

	for r := range results {
		hit := r.t
		if r.status >= 200 && r.status < 400 {
			if ready == nil || hit.containerPort < ready.containerPort {
				ready = &hit
			}
		} else if fallback == nil || hit.containerPort < fallback.containerPort {
			fallback = &hit
		}
	}
	return ready, fallback
}

func (s *Sandbox) Destroy(ctx context.Context) {
	if s == nil || s.ID == "" {
		return
	}
	ctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()

	err := s.docker.ContainerRemove(ctx, s.ID, container.RemoveOptions{
		Force:         true,
		RemoveVolumes: true,
	})
	if err != nil && !client.IsErrNotFound(err) {
		s.log.Error("destroy container failed", "container", short(s.ID), "err", err)
		return
	}
	s.log.Info("sandbox destroyed", "container", short(s.ID))
}

func (s *Sandbox) Logs(ctx context.Context, tail string) string {
	rc, err := s.docker.ContainerLogs(ctx, s.ID, container.LogsOptions{
		ShowStdout: true, ShowStderr: true, Tail: tail,
	})
	if err != nil {
		return ""
	}
	defer rc.Close()
	b, _ := io.ReadAll(io.LimitReader(rc, 32*1024))
	return string(b)
}

func freePort() (int, error) {
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return 0, err
	}
	defer l.Close()
	return l.Addr().(*net.TCPAddr).Port, nil
}

func containerPorts(targets []target) []int {
	out := make([]int, 0, len(targets))
	for _, t := range targets {
		out = append(out, t.containerPort)
	}
	return out
}

func sandboxName(submissionID string) string {
	var b strings.Builder
	b.WriteString("autograder-sbx-")
	for _, r := range strings.ToLower(submissionID) {
		if (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9') {
			b.WriteRune(r)
		} else {
			b.WriteByte('-')
		}
	}

	name := strings.Trim(b.String(), "-")
	if len(name) > 63 { // batas satu label DNS
		name = strings.TrimRight(name[:63], "-")
	}
	return name
}

func short(id string) string {
	if len(id) > 12 {
		return id[:12]
	}
	return id
}

func ptr[T any](v T) *T { return &v }

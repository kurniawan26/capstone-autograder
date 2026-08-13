package sandbox

import (
	"context"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net"
	"net/http"
	"sync"
	"time"

	"github.com/docker/docker/api/types/container"
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
	docker *client.Client
	log    *slog.Logger
}

func NewManager(log *slog.Logger) (*Manager, error) {
	cli, err := client.NewClientWithOpts(client.FromEnv, client.WithAPIVersionNegotiation())
	if err != nil {
		return nil, fmt.Errorf("connect to docker: %w", err)
	}
	return &Manager{docker: cli, log: log}, nil
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

func (m *Manager) Launch(ctx context.Context, spec Spec) (sb *Sandbox, err error) {
	injected, err := freePort()
	if err != nil {
		return nil, fmt.Errorf("allocate injected port: %w", err)
	}

	mapping := map[int]int{injected: injected}
	for _, cp := range spec.CandidatePorts {
		if _, seen := mapping[cp]; seen {
			continue
		}
		hp, err := freePort()
		if err != nil {
			return nil, fmt.Errorf("allocate host port for %d: %w", cp, err)
		}
		mapping[cp] = hp
	}

	exposed := nat.PortSet{}
	bindings := nat.PortMap{}
	for cp, hp := range mapping {
		p, err := nat.NewPort("tcp", fmt.Sprintf("%d", cp))
		if err != nil {
			return nil, fmt.Errorf("invalid container port %d: %w", cp, err)
		}
		exposed[p] = struct{}{}
		bindings[p] = []nat.PortBinding{{HostIP: "127.0.0.1", HostPort: fmt.Sprintf("%d", hp)}}
	}

	name := fmt.Sprintf("autograder-sbx-%s", spec.SubmissionID)

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
		nil, nil, name)
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

	if err = sb.waitHealthy(ctx, mapping); err != nil {
		return nil, err
	}

	return sb, nil
}

func (s *Sandbox) waitHealthy(ctx context.Context, mapping map[int]int) error {
	deadline := time.Now().Add(healthCheckTimeout)
	exited, waitErr := s.docker.ContainerWait(ctx, s.ID, container.WaitConditionNotRunning)

	var fallbackPort int

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

		ready, fallback := probeAll(mapping)
		if ready > 0 {
			s.adopt(ready, mapping[ready])
			return nil
		}
		if fallback > 0 {
			fallbackPort = fallback
		}
	}

	if fallbackPort > 0 {
		s.adopt(fallbackPort, mapping[fallbackPort])
		s.log.Warn("sandbox served a non-2xx/3xx response; capturing it anyway",
			"container", short(s.ID), "url", s.BaseURL)
		return nil
	}

	return fmt.Errorf("no published port answered HTTP within %s (probed container ports %v)",
		healthCheckTimeout, containerPorts(mapping))
}

func (s *Sandbox) adopt(containerPort, hostPort int) {
	s.ServingPort = containerPort
	s.BaseURL = fmt.Sprintf("http://127.0.0.1:%d", hostPort)
	s.log.Info("sandbox healthy",
		"container", short(s.ID), "url", s.BaseURL, "container_port", containerPort)
}

func probeAll(mapping map[int]int) (ready int, fallback int) {
	type outcome struct {
		containerPort int
		status        int
	}

	results := make(chan outcome, len(mapping))
	var wg sync.WaitGroup
	httpClient := &http.Client{
		Timeout:       probeTimeout,
		CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
	}

	for cp, hp := range mapping {
		wg.Add(1)
		go func(cp, hp int) {
			defer wg.Done()
			resp, err := httpClient.Get(fmt.Sprintf("http://127.0.0.1:%d/", hp))
			if err != nil {
				return
			}
			io.Copy(io.Discard, io.LimitReader(resp.Body, 4096))
			resp.Body.Close()
			results <- outcome{cp, resp.StatusCode}
		}(cp, hp)
	}
	wg.Wait()
	close(results)

	for r := range results {
		if r.status >= 200 && r.status < 400 {
			if ready == 0 || r.containerPort < ready {
				ready = r.containerPort
			}
		} else if fallback == 0 || r.containerPort < fallback {
			fallback = r.containerPort
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

func containerPorts(mapping map[int]int) []int {
	out := make([]int, 0, len(mapping))
	for cp := range mapping {
		out = append(out, cp)
	}
	return out
}

func short(id string) string {
	if len(id) > 12 {
		return id[:12]
	}
	return id
}

func ptr[T any](v T) *T { return &v }

package builder

import (
	"archive/tar"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/docker/docker/api/types/build"
	"github.com/docker/docker/api/types/image"
	"github.com/docker/docker/client"

	"github.com/dicoding/capstone-autograder/docker-sandbox/internal/detect"
)

const ImageLabel = "autograder.ephemeral"
const buildBudget = 8 * time.Minute

var contextExcludes = map[string]bool{
	".git":         true,
	"node_modules": true,
	"__MACOSX":     true,
	".next":        true,
	".venv":        true,
	"__pycache__":  true,
	"vendor":       true,
}

type Builder struct {
	docker          *client.Client
	log             *slog.Logger
	railpackPath    string
	buildkitHost    string
	dockerConfigDir string
}

func New(docker *client.Client, log *slog.Logger) *Builder {
	b := &Builder{docker: docker, log: log, buildkitHost: os.Getenv("BUILDKIT_HOST")}

	path, err := exec.LookPath("railpack")
	switch {
	case err != nil:
		log.Info("railpack not found on PATH; falling back to heuristic Dockerfile generation")
	case b.buildkitHost == "":
		log.Warn("railpack found but BUILDKIT_HOST is unset; falling back to heuristic Dockerfile generation",
			"hint", "docker compose up -d buildkit && export BUILDKIT_HOST=docker-container://autograder-buildkit")
	default:
		b.railpackPath = path
		b.dockerConfigDir = isolatedDockerConfig(log)
		log.Info("railpack available", "path", path,
			"buildkit_host", b.buildkitHost, "docker_config", b.dockerConfigDir)
	}
	return b
}

func isolatedDockerConfig(log *slog.Logger) string {
	if override := os.Getenv("RAILPACK_DOCKER_CONFIG"); override != "" {
		return override
	}
	dir, err := os.MkdirTemp("", "autograder-dockercfg-*")
	if err != nil {
		log.Warn("could not create isolated docker config; using ambient credentials", "err", err)
		return ""
	}
	if err := os.WriteFile(filepath.Join(dir, "config.json"), []byte("{}\n"), 0o600); err != nil {
		log.Warn("could not write isolated docker config; using ambient credentials", "err", err)
		return ""
	}
	return dir
}

func (b *Builder) RailpackAvailable() bool { return b.railpackPath != "" }

type Result struct {
	Tag      string
	Log      string
	Strategy detect.Strategy
}

func (b *Builder) Build(ctx context.Context, submissionID, srcDir string, plan detect.Plan) (Result, error) {
	ctx, cancel := context.WithTimeout(ctx, buildBudget)
	defer cancel()

	tag := fmt.Sprintf("autograder/ephemeral:%s", sanitizeTag(submissionID))

	if plan.Strategy == detect.StrategyRailpack {
		out, err := b.buildWithRailpack(ctx, srcDir, tag)
		return Result{Tag: tag, Log: out, Strategy: plan.Strategy}, err
	}

	if plan.GeneratedDockerfile != "" {
		path := filepath.Join(srcDir, plan.DockerfileName)
		if err := os.WriteFile(path, []byte(plan.GeneratedDockerfile), 0o644); err != nil {
			return Result{}, fmt.Errorf("write generated Dockerfile: %w", err)
		}
	}

	tarCtx, err := tarDirectory(srcDir)
	if err != nil {
		return Result{}, fmt.Errorf("build context: %w", err)
	}

	resp, err := b.docker.ImageBuild(ctx, tarCtx, build.ImageBuildOptions{
		Tags:        []string{tag},
		Dockerfile:  plan.DockerfileName,
		Remove:      true,
		ForceRemove: true,
		PullParent:  false,
		Labels:      map[string]string{ImageLabel: "true", "autograder.submission_id": submissionID},
	})
	if err != nil {
		return Result{}, fmt.Errorf("image build: %w", err)
	}
	defer resp.Body.Close()

	buildLog, err := drainBuildOutput(resp.Body)
	if err != nil {
		return Result{Tag: tag, Log: buildLog, Strategy: plan.Strategy}, err
	}

	b.log.Info("image built", "tag", tag, "strategy", plan.Strategy, "framework", plan.Framework)
	return Result{Tag: tag, Log: buildLog, Strategy: plan.Strategy}, nil
}

func (b *Builder) Remove(ctx context.Context, tag string) {
	if tag == "" {
		return
	}
	ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()

	_, err := b.docker.ImageRemove(ctx, tag, image.RemoveOptions{Force: true, PruneChildren: true})
	if err != nil {
		b.log.Warn("remove ephemeral image failed", "tag", tag, "err", err)
	}
}

func (b *Builder) buildWithRailpack(ctx context.Context, srcDir, tag string) (string, error) {
	cmd := exec.CommandContext(ctx, b.railpackPath,
		"build", srcDir,
		"--name", tag,
		"--progress", "plain",
	)
	cmd.Env = append(os.Environ(), "BUILDKIT_HOST="+b.buildkitHost)
	if b.dockerConfigDir != "" {
		cmd.Env = append(cmd.Env, "DOCKER_CONFIG="+b.dockerConfigDir)
	}

	var out bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = &out
	if err := cmd.Run(); err != nil {
		return truncate(out.String()), fmt.Errorf("railpack build: %w", err)
	}

	if err := b.labelImage(ctx, tag); err != nil {
		b.log.Warn("could not label railpack image", "tag", tag, "err", err)
	}

	b.log.Info("image built via railpack", "tag", tag)
	return truncate(out.String()), nil
}

func (b *Builder) labelImage(ctx context.Context, tag string) error {
	df := fmt.Sprintf("FROM %s\nLABEL %s=true\n", tag, ImageLabel)

	var buf bytes.Buffer
	tw := tar.NewWriter(&buf)
	if err := tw.WriteHeader(&tar.Header{
		Name: "Dockerfile", Mode: 0o644, Size: int64(len(df)),
	}); err != nil {
		return err
	}
	if _, err := tw.Write([]byte(df)); err != nil {
		return err
	}
	if err := tw.Close(); err != nil {
		return err
	}

	resp, err := b.docker.ImageBuild(ctx, &buf, build.ImageBuildOptions{
		Tags:       []string{tag},
		Dockerfile: "Dockerfile",
		Remove:     true,
		PullParent: false,
	})
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	_, err = drainBuildOutput(resp.Body)
	return err
}

func tarDirectory(dir string) (io.Reader, error) {
	var buf bytes.Buffer
	tw := tar.NewWriter(&buf)

	err := filepath.Walk(dir, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(dir, path)
		if err != nil {
			return err
		}
		if rel == "." {
			return nil
		}
		if info.IsDir() && contextExcludes[info.Name()] {
			return filepath.SkipDir
		}

		if info.Mode()&os.ModeSymlink != 0 {
			return nil
		}

		hdr, err := tar.FileInfoHeader(info, "")
		if err != nil {
			return err
		}
		hdr.Name = filepath.ToSlash(rel)
		if err := tw.WriteHeader(hdr); err != nil {
			return err
		}
		if info.IsDir() {
			return nil
		}

		f, err := os.Open(path)
		if err != nil {
			return err
		}
		defer f.Close()
		_, err = io.Copy(tw, f)
		return err
	})
	if err != nil {
		return nil, err
	}
	if err := tw.Close(); err != nil {
		return nil, err
	}
	return &buf, nil
}

func drainBuildOutput(r io.Reader) (string, error) {
	var sb strings.Builder
	dec := json.NewDecoder(r)

	for {
		var msg struct {
			Stream      string `json:"stream"`
			Error       string `json:"error"`
			ErrorDetail *struct {
				Message string `json:"message"`
			} `json:"errorDetail"`
		}
		if err := dec.Decode(&msg); err == io.EOF {
			break
		} else if err != nil {
			return truncate(sb.String()), fmt.Errorf("read build stream: %w", err)
		}

		if msg.Stream != "" {
			sb.WriteString(msg.Stream)
		}
		if msg.Error != "" {
			detail := msg.Error
			if msg.ErrorDetail != nil && msg.ErrorDetail.Message != "" {
				detail = msg.ErrorDetail.Message
			}
			return truncate(sb.String()), fmt.Errorf("build failed: %s", strings.TrimSpace(detail))
		}
	}
	return truncate(sb.String()), nil
}

const maxLogBytes = 16 * 1024

func truncate(s string) string {
	if len(s) <= maxLogBytes {
		return s
	}
	return "...(truncated)...\n" + s[len(s)-maxLogBytes:]
}

func sanitizeTag(s string) string {
	var b strings.Builder
	for _, r := range strings.ToLower(s) {
		switch {
		case r >= 'a' && r <= 'z', r >= '0' && r <= '9', r == '-', r == '.', r == '_':
			b.WriteRune(r)
		default:
			b.WriteByte('-')
		}
	}
	out := b.String()
	if len(out) > 100 {
		out = out[:100]
	}
	if out == "" {
		out = "unknown"
	}
	return out
}

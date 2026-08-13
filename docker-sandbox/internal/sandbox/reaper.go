package sandbox

import (
	"context"
	"log/slog"
	"time"

	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/filters"
	"github.com/docker/docker/api/types/image"
)

const maxSandboxAge = 45 * time.Second

type Reaper struct {
	m        *Manager
	interval time.Duration
	log      *slog.Logger
}

func NewReaper(m *Manager, interval time.Duration, log *slog.Logger) *Reaper {
	return &Reaper{m: m, interval: interval, log: log}
}

func (r *Reaper) Run(ctx context.Context) {
	ticker := time.NewTicker(r.interval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			r.sweep(ctx)
			r.sweepImages(ctx)
		}
	}
}

func (r *Reaper) sweep(ctx context.Context) {
	list, err := r.m.docker.ContainerList(ctx, container.ListOptions{
		All:     true,
		Filters: filters.NewArgs(filters.Arg("label", "autograder.sandbox=true")),
	})
	if err != nil {
		r.log.Error("reaper list failed", "err", err)
		return
	}

	cutoff := time.Now().Add(-maxSandboxAge).Unix()
	for _, c := range list {
		if c.Created > cutoff {
			continue
		}
		err := r.m.docker.ContainerRemove(ctx, c.ID, container.RemoveOptions{
			Force: true, RemoveVolumes: true,
		})
		if err != nil {
			r.log.Error("reaper remove failed", "container", short(c.ID), "err", err)
			continue
		}
		r.log.Warn("reaped stale sandbox",
			"container", short(c.ID),
			"submission", c.Labels["autograder.submission_id"],
			"age_s", time.Now().Unix()-c.Created)
	}
}

const maxImageAge = 30 * time.Minute

func (r *Reaper) sweepImages(ctx context.Context) {
	imgs, err := r.m.docker.ImageList(ctx, image.ListOptions{
		Filters: filters.NewArgs(filters.Arg("label", "autograder.ephemeral=true")),
	})
	if err != nil {
		r.log.Error("reaper image list failed", "err", err)
		return
	}

	cutoff := time.Now().Add(-maxImageAge).Unix()
	for _, img := range imgs {
		if img.Created > cutoff {
			continue
		}
		if _, err := r.m.docker.ImageRemove(ctx, img.ID, image.RemoveOptions{
			Force: true, PruneChildren: true,
		}); err != nil {
			r.log.Debug("reaper image remove skipped", "image", short(img.ID), "err", err)
			continue
		}
		r.log.Warn("reaped stale ephemeral image", "image", short(img.ID), "tags", img.RepoTags)
	}
}

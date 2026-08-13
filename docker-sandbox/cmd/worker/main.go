package main

import (
	"context"
	"errors"
	"log/slog"
	"net"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/dicoding/capstone-autograder/docker-sandbox/internal/api"
	"github.com/dicoding/capstone-autograder/docker-sandbox/internal/builder"
	"github.com/dicoding/capstone-autograder/docker-sandbox/internal/capture"
	"github.com/dicoding/capstone-autograder/docker-sandbox/internal/sandbox"
	"github.com/dicoding/capstone-autograder/docker-sandbox/internal/storage"
)

func main() {
	log := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))

	if err := run(log); err != nil {
		log.Error("worker exited", "err", err)
		os.Exit(1)
	}
}

func run(log *slog.Logger) error {
	mgr, err := sandbox.NewManager(log)
	if err != nil {
		return err
	}
	defer mgr.Close()

	store, err := storage.New(storage.Config{
		Endpoint:          env("S3_ENDPOINT_HOST", "localhost:9000"),
		AccessKey:         env("S3_ACCESS_KEY_ID", "minioadmin"),
		SecretKey:         env("S3_SECRET_ACCESS_KEY", "minioadmin"),
		UseSSL:            env("S3_USE_SSL", "false") == "true",
		SubmissionsBucket: env("S3_SUBMISSIONS_BUCKET", "submissions"),
		ScreenshotsBucket: env("S3_SCREENSHOTS_BUCKET", "screenshots"),
	})
	if err != nil {
		return err
	}

	cap, err := capture.New(log)
	if err != nil {
		return err
	}
	defer cap.Close()

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	go sandbox.NewReaper(mgr, 15*time.Second, log).Run(ctx)

	bld := builder.New(mgr.Docker(), log)

	addr := net.JoinHostPort(
		env("SANDBOX_WORKER_HOST", "127.0.0.1"),
		env("SANDBOX_WORKER_PORT", "8090"),
	)

	srv := &http.Server{
		Addr:              addr,
		Handler:           api.NewServer(mgr, bld, cap, store, log).Routes(),
		ReadHeaderTimeout: 10 * time.Second,
		WriteTimeout:      20 * time.Minute,
	}

	errCh := make(chan error, 1)
	go func() {
		log.Info("sandbox worker listening", "addr", srv.Addr, "railpack", bld.RailpackAvailable())
		if err := srv.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			errCh <- err
		}
	}()

	select {
	case err := <-errCh:
		return err
	case <-ctx.Done():
		log.Info("shutting down")
	}

	shutdownCtx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	return srv.Shutdown(shutdownCtx)
}

func env(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}

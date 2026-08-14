package api

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"log/slog"
	"math"
	"net/http"
	"os"
	"time"

	"github.com/dicoding/capstone-autograder/docker-sandbox/internal/builder"
	"github.com/dicoding/capstone-autograder/docker-sandbox/internal/capture"
	"github.com/dicoding/capstone-autograder/docker-sandbox/internal/detect"
	"github.com/dicoding/capstone-autograder/docker-sandbox/internal/optimize"
	"github.com/dicoding/capstone-autograder/docker-sandbox/internal/sandbox"
	"github.com/dicoding/capstone-autograder/docker-sandbox/internal/storage"
	"github.com/dicoding/capstone-autograder/docker-sandbox/internal/urlguard"
)

const runBudget = 30 * time.Second

type CaptureRequest struct {
	SubmissionID      string `json:"submission_id"`
	SourceKey         string `json:"source_key"`
	LiveURL           string `json:"live_url"`
	WebPQuality       int    `json:"webp_quality"`
	ScanRoutes        bool   `json:"scan_routes"`
	MaxRoutes         int    `json:"max_routes"`
	ScanBudgetSeconds int    `json:"scan_budget_seconds"`
	InlineImages      *bool  `json:"inline_images"`
}

func (r CaptureRequest) inlineImages() bool {
	return r.InlineImages == nil || *r.InlineImages
}

const maxScanBudget = 10 * time.Minute

func (r CaptureRequest) captureOptions() capture.Options {
	budget := time.Duration(r.ScanBudgetSeconds) * time.Second
	if budget > maxScanBudget {
		budget = maxScanBudget
	}

	return capture.Options{
		ScanRoutes: r.ScanRoutes,
		MaxRoutes:  r.MaxRoutes,
		Budget:     budget,
	}
}

type CaptureResponse struct {
	SubmissionID string          `json:"submission_id"`
	Screenshots  []ScreenshotRef `json:"screenshots"`
	Build        BuildInfo       `json:"build"`
	DurationMS   int64           `json:"duration_ms"`
}

type BuildInfo struct {
	Strategy     string   `json:"strategy"`
	Framework    string   `json:"framework"`
	Notes        []string `json:"notes,omitempty"`
	ServingPort  int      `json:"serving_port"`
	InjectedPort int      `json:"injected_port"`
	LiveURL      string   `json:"live_url,omitempty"`
}

type ScreenshotRef struct {
	Name   string `json:"name"`
	URL    string `json:"url"`
	Width  int    `json:"width"`
	Height int    `json:"height"`

	Bucket  string `json:"bucket"`
	Key     string `json:"key"`
	WebPKey string `json:"webp_key"`

	PNGBytes          int     `json:"png_bytes"`
	WebPBytes         int     `json:"webp_bytes"`
	ReductionPct      float64 `json:"reduction_pct"`
	Downscaled        bool    `json:"downscaled"`
	CroppedFromHeight int     `json:"cropped_from_height,omitempty"`
	WebPBase64        string  `json:"webp_base64,omitempty"`
}

type ErrorResponse struct {
	Error string    `json:"error"`
	Stage string    `json:"stage"`
	Logs  string    `json:"logs,omitempty"`
	Build BuildInfo `json:"build"`
}

type Server struct {
	mgr     *sandbox.Manager
	builder *builder.Builder
	cap     *capture.Capturer
	store   *storage.Client
	log     *slog.Logger
}

func NewServer(mgr *sandbox.Manager, bld *builder.Builder, cap *capture.Capturer, store *storage.Client, log *slog.Logger) *Server {
	return &Server{mgr: mgr, builder: bld, cap: cap, store: store, log: log}
}

func (s *Server) Routes() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", s.handleHealth)
	mux.HandleFunc("POST /v1/capture", s.handleCapture)
	return mux
}

func (s *Server) handleHealth(w http.ResponseWriter, _ *http.Request) {
	writeJSON(w, http.StatusOK, map[string]any{
		"status":             "ok",
		"railpack_available": s.builder.RailpackAvailable(),
	})
}

func (s *Server) handleCapture(w http.ResponseWriter, r *http.Request) {
	var req CaptureRequest
	if err := json.NewDecoder(http.MaxBytesReader(w, r.Body, 1<<20)).Decode(&req); err != nil {
		writeErr(w, http.StatusBadRequest, "decode", err.Error(), "", BuildInfo{})
		return
	}
	if req.SubmissionID == "" {
		writeErr(w, http.StatusBadRequest, "validate", "submission_id is required", "", BuildInfo{})
		return
	}
	if req.SourceKey == "" && req.LiveURL == "" {
		writeErr(w, http.StatusBadRequest, "validate",
			"either source_key or live_url is required", "", BuildInfo{})
		return
	}

	started := time.Now()
	ctx := r.Context()
	log := s.log.With("submission", req.SubmissionID)

	if req.LiveURL != "" {
		s.captureLive(ctx, w, req, started, log)
		return
	}

	srcDir, err := s.store.FetchAndExtract(ctx, req.SourceKey)
	if err != nil {
		log.Error("fetch source failed", "err", err)
		writeErr(w, http.StatusBadRequest, "fetch_source", err.Error(), "", BuildInfo{})
		return
	}
	defer os.RemoveAll(srcDir)

	plan, err := detect.Detect(srcDir, detect.Options{
		RailpackAvailable: s.builder.RailpackAvailable(),
	})
	if err != nil {
		log.Warn("detection failed", "err", err)
		writeErr(w, http.StatusUnprocessableEntity, "detect", err.Error(), "", BuildInfo{})
		return
	}
	info := BuildInfo{
		Strategy:  string(plan.Strategy),
		Framework: plan.Framework,
		Notes:     plan.Notes,
	}
	log.Info("build plan", "strategy", plan.Strategy, "framework", plan.Framework)

	built, err := s.builder.Build(ctx, req.SubmissionID, srcDir, plan)
	if built.Tag != "" {
		defer s.builder.Remove(context.WithoutCancel(ctx), built.Tag)
	}
	if err != nil {
		log.Error("build failed", "err", err)
		writeErr(w, http.StatusUnprocessableEntity, "build", err.Error(), built.Log, info)
		return
	}

	runCtx, cancel := context.WithTimeout(ctx, runBudget)
	defer cancel()

	sb, err := s.mgr.Launch(runCtx, sandbox.Spec{
		SubmissionID:   req.SubmissionID,
		ImageTag:       built.Tag,
		CandidatePorts: plan.CandidatePorts,
	})
	if err != nil {
		log.Error("launch failed", "err", err)
		writeErr(w, http.StatusUnprocessableEntity, "launch", err.Error(), built.Log, info)
		return
	}
	defer sb.Destroy(context.WithoutCancel(ctx))

	info.ServingPort = sb.ServingPort
	info.InjectedPort = sb.InjectedPort

	shots, err := s.cap.Capture(sb.BaseURL, req.captureOptions())
	if err != nil {
		logs := sb.Logs(context.WithoutCancel(ctx), "100")
		log.Error("capture failed", "err", err)
		writeErr(w, http.StatusUnprocessableEntity, "capture", err.Error(), logs, info)
		return
	}
	info.Notes = append(info.Notes, shots.Notes...)

	s.finish(ctx, w, req, shots.Shots, info, started, log)
}

func (s *Server) captureLive(ctx context.Context, w http.ResponseWriter, req CaptureRequest, started time.Time, log *slog.Logger) {
	info := BuildInfo{
		Strategy: "live_url",
		Notes:    []string{"Captured directly from the submitted URL; no container was built."},
	}

	target, err := urlguard.Check(ctx, req.LiveURL)
	if err != nil {
		log.Warn("live url rejected", "url", req.LiveURL, "err", err)
		writeErr(w, http.StatusUnprocessableEntity, "live_url", err.Error(), "", info)
		return
	}
	info.LiveURL = target.String()

	log.Info("capturing live url", "url", info.LiveURL)

	opts := req.captureOptions()
	opts.Guard = urlguard.Allow

	shots, err := s.cap.Capture(info.LiveURL, opts)
	if err != nil {
		log.Error("live capture failed", "url", info.LiveURL, "err", err)
		writeErr(w, http.StatusUnprocessableEntity, "capture", err.Error(), "", info)
		return
	}
	info.Notes = append(info.Notes, shots.Notes...)

	s.finish(ctx, w, req, shots.Shots, info, started, log)
}

func (s *Server) finish(ctx context.Context, w http.ResponseWriter, req CaptureRequest, shots []capture.Shot, info BuildInfo, started time.Time, log *slog.Logger) {
	refs := make([]ScreenshotRef, 0, len(shots))

	for _, shot := range shots {
		optimized, err := optimize.Compress(shot.PNG, req.WebPQuality)
		if err != nil {
			log.Error("optimize failed; skipping this screenshot",
				"name", shot.Name, "err", err)
			info.Notes = append(info.Notes, fmt.Sprintf(
				"Screenshot %q tidak dapat dioptimasi dan tidak disertakan: %v", shot.Name, err))
			continue
		}

		pngKey, err := s.store.PutScreenshot(ctx, req.SubmissionID, shot.Name, shot.PNG)
		if err != nil {
			log.Error("upload screenshot failed", "name", shot.Name, "err", err)
			writeErr(w, http.StatusInternalServerError, "upload", err.Error(), "", info)
			return
		}

		webpKey, err := s.store.PutWebP(ctx, req.SubmissionID, shot.Name, optimized.WebP)
		if err != nil {
			log.Error("upload webp failed", "name", shot.Name, "err", err)
			writeErr(w, http.StatusInternalServerError, "upload", err.Error(), "", info)
			return
		}

		if optimized.CroppedFromHeight > 0 {
			log.Warn("screenshot cropped to fit the webp format limit",
				"name", shot.Name, "from_height", optimized.CroppedFromHeight,
				"to_height", optimized.Height)
			info.Notes = append(info.Notes, fmt.Sprintf(
				"Screenshot %q dipotong dari %dpx ke %dpx — batas format WebP; bagian bawah halaman tidak terlihat.",
				shot.Name, optimized.CroppedFromHeight, optimized.Height))
		}

		log.Info("optimized screenshot",
			"name", shot.Name,
			"png_bytes", optimized.OriginalBytes,
			"webp_bytes", optimized.CompressedByte,
			"reduction_pct", math.Round(optimized.ReductionPct()*10)/10,
			"downscaled", optimized.Downscaled,
			"ms", optimized.Duration.Milliseconds())

		ref := ScreenshotRef{
			Name:         shot.Name,
			URL:          shot.URL,
			Width:        optimized.Width,
			Height:       optimized.Height,
			Bucket:       s.store.ScreenshotsBucket,
			Key:          pngKey,
			WebPKey:      webpKey,
			PNGBytes:     optimized.OriginalBytes,
			WebPBytes:    optimized.CompressedByte,
			ReductionPct: math.Round(optimized.ReductionPct()*10) / 10,
			Downscaled:   optimized.Downscaled,

			CroppedFromHeight: optimized.CroppedFromHeight,
		}
		if req.inlineImages() {
			ref.WebPBase64 = base64.StdEncoding.EncodeToString(optimized.WebP)
		}

		refs = append(refs, ref)
	}

	if len(refs) == 0 {
		log.Error("no screenshot survived optimization")
		writeErr(w, http.StatusInternalServerError, "optimize",
			"no screenshot could be optimized", "", info)
		return
	}

	elapsed := time.Since(started)
	log.Info("capture cycle complete",
		"strategy", info.Strategy, "shots", len(refs), "duration_ms", elapsed.Milliseconds())

	writeJSON(w, http.StatusOK, CaptureResponse{
		SubmissionID: req.SubmissionID,
		Screenshots:  refs,
		Build:        info,
		DurationMS:   elapsed.Milliseconds(),
	})
}

func writeJSON(w http.ResponseWriter, status int, body any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(body)
}

func writeErr(w http.ResponseWriter, status int, stage, msg, logs string, info BuildInfo) {
	writeJSON(w, status, ErrorResponse{Error: msg, Stage: stage, Logs: logs, Build: info})
}

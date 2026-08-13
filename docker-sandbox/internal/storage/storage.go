package storage

import (
	"archive/zip"
	"bytes"
	"context"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"

	"github.com/minio/minio-go/v7"
	"github.com/minio/minio-go/v7/pkg/credentials"
)

const (
	maxUncompressedBytes = 512 * 1024 * 1024
	maxFileCount         = 20_000
)

type Client struct {
	mc                *minio.Client
	SubmissionsBucket string
	ScreenshotsBucket string
}

type Config struct {
	Endpoint          string
	AccessKey         string
	SecretKey         string
	UseSSL            bool
	SubmissionsBucket string
	ScreenshotsBucket string
}

func New(cfg Config) (*Client, error) {
	mc, err := minio.New(cfg.Endpoint, &minio.Options{
		Creds:  credentials.NewStaticV4(cfg.AccessKey, cfg.SecretKey, ""),
		Secure: cfg.UseSSL,
	})
	if err != nil {
		return nil, fmt.Errorf("minio client: %w", err)
	}
	return &Client{
		mc:                mc,
		SubmissionsBucket: cfg.SubmissionsBucket,
		ScreenshotsBucket: cfg.ScreenshotsBucket,
	}, nil
}

func (c *Client) FetchAndExtract(ctx context.Context, objectKey string) (string, error) {
	obj, err := c.mc.GetObject(ctx, c.SubmissionsBucket, objectKey, minio.GetObjectOptions{})
	if err != nil {
		return "", fmt.Errorf("get %s/%s: %w", c.SubmissionsBucket, objectKey, err)
	}
	defer obj.Close()

	raw, err := io.ReadAll(io.LimitReader(obj, maxUncompressedBytes))
	if err != nil {
		return "", fmt.Errorf("read %s: %w", objectKey, err)
	}

	dir, err := os.MkdirTemp("", "autograder-src-*")
	if err != nil {
		return "", fmt.Errorf("temp dir: %w", err)
	}

	if err := unzip(bytes.NewReader(raw), int64(len(raw)), dir); err != nil {
		os.RemoveAll(dir)
		return "", err
	}
	return dir, nil
}

func (c *Client) PutScreenshot(ctx context.Context, submissionID, name string, png []byte) (string, error) {
	return c.put(ctx, fmt.Sprintf("%s/%s.png", submissionID, name), png, "image/png")
}

func (c *Client) PutWebP(ctx context.Context, submissionID, name string, data []byte) (string, error) {
	return c.put(ctx, fmt.Sprintf("%s/%s.webp", submissionID, name), data, "image/webp")
}

func (c *Client) put(ctx context.Context, key string, data []byte, contentType string) (string, error) {
	_, err := c.mc.PutObject(ctx, c.ScreenshotsBucket, key,
		bytes.NewReader(data), int64(len(data)),
		minio.PutObjectOptions{ContentType: contentType})
	if err != nil {
		return "", fmt.Errorf("put %s/%s: %w", c.ScreenshotsBucket, key, err)
	}
	return key, nil
}

func unzip(r io.ReaderAt, size int64, dest string) error {
	zr, err := zip.NewReader(r, size)
	if err != nil {
		return fmt.Errorf("open zip: %w", err)
	}
	if len(zr.File) > maxFileCount {
		return fmt.Errorf("archive has %d entries, limit is %d", len(zr.File), maxFileCount)
	}

	prefix := commonRootDir(zr.File)

	var written int64
	for _, f := range zr.File {
		name := strings.TrimPrefix(f.Name, prefix)
		if name == "" || strings.HasSuffix(name, "/") {
			if name != "" {
				os.MkdirAll(filepath.Join(dest, filepath.FromSlash(name)), 0o755)
			}
			continue
		}
		target := filepath.Join(dest, filepath.FromSlash(name))
		if !strings.HasPrefix(target, filepath.Clean(dest)+string(os.PathSeparator)) {
			return fmt.Errorf("archive entry escapes destination: %q", f.Name)
		}
		if f.Mode()&os.ModeSymlink != 0 {
			continue
		}

		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return err
		}
		rc, err := f.Open()
		if err != nil {
			return fmt.Errorf("open %s: %w", f.Name, err)
		}
		out, err := os.OpenFile(target, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o644)
		if err != nil {
			rc.Close()
			return fmt.Errorf("create %s: %w", target, err)
		}
		n, err := io.Copy(out, io.LimitReader(rc, maxUncompressedBytes-written))
		out.Close()
		rc.Close()
		if err != nil {
			return fmt.Errorf("extract %s: %w", f.Name, err)
		}
		written += n
		if written >= maxUncompressedBytes {
			return fmt.Errorf("archive exceeds %d uncompressed bytes", maxUncompressedBytes)
		}
	}
	return nil
}

func commonRootDir(files []*zip.File) string {
	var root string
	for _, f := range files {
		name := f.Name
		if strings.HasPrefix(name, "__MACOSX/") || strings.HasPrefix(name, ".") {
			continue
		}
		idx := strings.Index(name, "/")
		if idx < 0 {
			return ""
		}
		top := name[:idx+1]
		if root == "" {
			root = top
		} else if root != top {
			return ""
		}
	}
	return root
}

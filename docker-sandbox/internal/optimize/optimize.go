package optimize

import (
	"bytes"
	"fmt"
	"image"
	"image/png"
	"time"

	"github.com/gen2brain/webp"
	"golang.org/x/image/draw"
)

const (
	MaxWidth  = 1920
	MaxHeight = 8000
	MinWidth  = 960
)

const DefaultQuality = 78

const webpMaxDimension = 16383

type Result struct {
	WebP              []byte
	Width             int
	Height            int
	OriginalBytes     int
	CompressedByte    int
	Duration          time.Duration
	Downscaled        bool
	CroppedFromHeight int
}

func (r Result) ReductionPct() float64 {
	if r.OriginalBytes == 0 {
		return 0
	}
	return (1 - float64(r.CompressedByte)/float64(r.OriginalBytes)) * 100
}

func Compress(pngData []byte, quality int) (Result, error) {
	started := time.Now()

	if quality <= 0 {
		quality = DefaultQuality
	}

	src, err := png.Decode(bytes.NewReader(pngData))
	if err != nil {
		return Result{}, fmt.Errorf("decode png: %w", err)
	}

	img, downscaled := fit(src)
	img, croppedFrom := clampToEncodable(img)

	bounds := img.Bounds()

	var buf bytes.Buffer
	if err := webp.Encode(&buf, img, webp.Options{Quality: quality}); err != nil {
		return Result{}, fmt.Errorf("encode webp at %dx%d: %w", bounds.Dx(), bounds.Dy(), err)
	}

	return Result{
		WebP:              buf.Bytes(),
		Width:             bounds.Dx(),
		Height:            bounds.Dy(),
		OriginalBytes:     len(pngData),
		CompressedByte:    buf.Len(),
		Duration:          time.Since(started),
		Downscaled:        downscaled,
		CroppedFromHeight: croppedFrom,
	}, nil
}

func clampToEncodable(img image.Image) (image.Image, int) {
	b := img.Bounds()
	if b.Dx() <= webpMaxDimension && b.Dy() <= webpMaxDimension {
		return img, 0
	}

	w := min(b.Dx(), webpMaxDimension)
	h := min(b.Dy(), webpMaxDimension)

	dst := image.NewRGBA(image.Rect(0, 0, w, h))
	draw.Draw(dst, dst.Bounds(), img, b.Min, draw.Src)

	return dst, b.Dy()
}

func fit(src image.Image) (image.Image, bool) {
	bounds := src.Bounds()
	w, h := bounds.Dx(), bounds.Dy()

	if w <= MaxWidth && h <= MaxHeight {
		return src, false
	}

	scale := min(float64(MaxWidth)/float64(w), float64(MaxHeight)/float64(h))

	if w >= MinWidth && float64(w)*scale < MinWidth {
		scale = float64(MinWidth) / float64(w)
	}

	dstW := max(int(float64(w)*scale), 1)
	dstH := max(int(float64(h)*scale), 1)

	dst := image.NewRGBA(image.Rect(0, 0, dstW, dstH))

	draw.CatmullRom.Scale(dst, dst.Bounds(), src, bounds, draw.Over, nil)

	return dst, true
}

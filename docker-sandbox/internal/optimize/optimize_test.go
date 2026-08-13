package optimize

import (
	"bytes"
	"image"
	"image/color"
	"image/png"
	"math/rand/v2"
	"testing"
)

func pngFixture(t *testing.T, w, h int) []byte {
	t.Helper()

	img := image.NewRGBA(image.Rect(0, 0, w, h))
	for y := range h {
		for x := range w {
			img.Set(x, y, color.RGBA{
				R: uint8((x * 7) % 256),
				G: uint8((y * 13) % 256),
				B: uint8((x + y) % 256),
				A: 255,
			})
		}
	}

	return encodePNG(t, img)
}

func noisyFixture(t *testing.T, w, h int) []byte {
	t.Helper()

	rng := rand.New(rand.NewPCG(42, 1024))
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	for y := range h {
		for x := range w {
			img.Set(x, y, color.RGBA{
				R: uint8(rng.UintN(256)),
				G: uint8(rng.UintN(256)),
				B: uint8(rng.UintN(256)),
				A: 255,
			})
		}
	}

	return encodePNG(t, img)
}

func encodePNG(t *testing.T, img image.Image) []byte {
	t.Helper()

	var buf bytes.Buffer
	if err := png.Encode(&buf, img); err != nil {
		t.Fatalf("encode fixture: %v", err)
	}
	return buf.Bytes()
}

func TestCompressProducesValidWebP(t *testing.T) {
	src := pngFixture(t, 800, 600)

	result, err := Compress(src, DefaultQuality)
	if err != nil {
		t.Fatalf("Compress: %v", err)
	}

	if result.Width != 800 || result.Height != 600 {
		t.Fatalf("dimensions changed on an image already inside the ceiling: %dx%d",
			result.Width, result.Height)
	}
	if result.Downscaled {
		t.Fatal("an 800x600 image should not be downscaled")
	}

	if !bytes.HasPrefix(result.WebP, []byte("RIFF")) || !bytes.Contains(result.WebP[:16], []byte("WEBP")) {
		t.Fatal("output is not a RIFF/WEBP container")
	}
}

func TestCompressShrinksUnpredictableContent(t *testing.T) {
	src := noisyFixture(t, 800, 600)

	result, err := Compress(src, DefaultQuality)
	if err != nil {
		t.Fatalf("Compress: %v", err)
	}

	if result.CompressedByte >= result.OriginalBytes {
		t.Fatalf("webp (%d B) is not smaller than the png (%d B)",
			result.CompressedByte, result.OriginalBytes)
	}
	if result.ReductionPct() <= 0 {
		t.Fatalf("expected a positive reduction, got %.1f%%", result.ReductionPct())
	}
}

func TestCompressDownscalesOversizedImages(t *testing.T) {
	src := pngFixture(t, 2400, 1400)

	result, err := Compress(src, 0)
	if err != nil {
		t.Fatalf("Compress: %v", err)
	}

	if !result.Downscaled {
		t.Fatal("a 2400x1400 image should have been downscaled")
	}
	if result.Width > MaxWidth {
		t.Fatalf("result width %d exceeds the %d ceiling", result.Width, MaxWidth)
	}

	srcRatio := 2400.0 / 1400.0
	gotRatio := float64(result.Width) / float64(result.Height)
	if diff := srcRatio - gotRatio; diff > 0.01 || diff < -0.01 {
		t.Fatalf("aspect ratio drifted: %.3f -> %.3f", srcRatio, gotRatio)
	}
}

func TestCompressKeepsTallPagesLegible(t *testing.T) {
	src := pngFixture(t, 1440, 3960)

	result, err := Compress(src, 0)
	if err != nil {
		t.Fatalf("Compress: %v", err)
	}

	if result.Width != 1440 {
		t.Fatalf("a 1440px-wide full-page capture was resized to %dpx", result.Width)
	}
	if result.Downscaled {
		t.Fatal("a tall page inside the width ceiling should not be downscaled at all")
	}
}

func TestCompressNeverShrinksBelowTheWidthFloor(t *testing.T) {
	src := pngFixture(t, 1440, 20000)

	result, err := Compress(src, 0)
	if err != nil {
		t.Fatalf("Compress: %v", err)
	}

	if !result.Downscaled {
		t.Fatal("a 20000px-tall image should have been downscaled")
	}
	if result.Width < MinWidth {
		t.Fatalf("width %d fell below the %d floor", result.Width, MinWidth)
	}
	if result.Height <= MaxHeight {
		t.Fatalf("expected height to be allowed past %d to protect width, got %d",
			MaxHeight, result.Height)
	}
}

func TestCompressRejectsNonPNG(t *testing.T) {
	if _, err := Compress([]byte("this is not an image"), DefaultQuality); err == nil {
		t.Fatal("Compress accepted a non-PNG payload")
	}
}

func TestReductionPctHandlesEmptyInput(t *testing.T) {
	if got := (Result{}).ReductionPct(); got != 0 {
		t.Fatalf("expected 0%% for an empty result, got %.1f", got)
	}
}

func TestCompressCropsPastTheFormatLimit(t *testing.T) {
	src := pngFixture(t, 960, 17000)

	result, err := Compress(src, DefaultQuality)
	if err != nil {
		t.Fatalf("Compress: %v", err)
	}

	if result.Height > webpMaxDimension {
		t.Fatalf("height %d exceeds the format limit %d", result.Height, webpMaxDimension)
	}
	if result.CroppedFromHeight == 0 {
		t.Fatal("a cropped image must report the height it was cut from")
	}
	if result.Width < MinWidth {
		t.Fatalf("cropping must not cost width: %d is below the %d floor", result.Width, MinWidth)
	}
	if !bytes.HasPrefix(result.WebP, []byte("RIFF")) {
		t.Fatal("output is not a WebP container")
	}
}

func TestCompressDoesNotReportCropsItDidNotMake(t *testing.T) {
	result, err := Compress(pngFixture(t, 800, 600), DefaultQuality)
	if err != nil {
		t.Fatalf("Compress: %v", err)
	}
	if result.CroppedFromHeight != 0 {
		t.Fatalf("nothing was cropped, but CroppedFromHeight = %d", result.CroppedFromHeight)
	}
}

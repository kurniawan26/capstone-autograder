package main

import (
	"fmt"
	"os"

	"github.com/dicoding/capstone-autograder/docker-sandbox/internal/capture"
)

func main() {
	fmt.Println("installing Playwright driver + Chromium...")
	if err := capture.InstallDrivers(); err != nil {
		fmt.Fprintln(os.Stderr, "install failed:", err)
		os.Exit(1)
	}
	fmt.Println("done")
}
